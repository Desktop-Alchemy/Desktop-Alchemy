//! One process-wide Raw Input receiver. No HID output, polling or Shell calls.
use crate::{devices::DeviceManager, input::RawInputSpec, model::*, process_lock::NamedGuard};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{Input::*, WindowsAndMessaging::*},
};
const WAKE: u32 = WM_APP + 91;
const MAX_INPUT: usize = 64 * 1024;
#[derive(Default)]
struct Metrics {
    registrations: AtomicU64,
    scans: AtomicU64,
    reports: AtomicU64,
}
#[derive(Debug, serde::Serialize)]
pub struct RawInputStats {
    pub registrations: u64,
    pub scans: u64,
    pub reports: u64,
}
pub struct RawInputService {
    metrics: Arc<Metrics>,
    thread_id: u32,
    task: thread::JoinHandle<()>,
}
impl RawInputService {
    pub fn start(manager: Arc<DeviceManager>, specs: Vec<RawInputSpec>) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(1);
        let metrics = Arc::new(Metrics::default());
        let receiver_metrics = metrics.clone();
        let task = thread::spawn(move || {
            if let Err(error) = unsafe { run(manager.clone(), specs, &tx, receiver_metrics) } {
                let _ = tx.send(Err(format!("{error:#}")));
                manager.host.log("error", format!("Raw Input：{error:#}"));
            }
        });
        match rx.recv().context(alchemy_device_api::UiFailure::new("backend.b203", "输入线程未启动"))? {
            Ok(thread_id) => Ok(Self {
                thread_id,
                task,
                metrics,
            }),
            Err(error) => {
                let _ = task.join();
                bail!("{error}")
            }
        }
    }
    pub fn stats(&self) -> RawInputStats {
        RawInputStats {
            registrations: self.metrics.registrations.load(Ordering::Relaxed),
            scans: self.metrics.scans.load(Ordering::Relaxed),
            reports: self.metrics.reports.load(Ordering::Relaxed),
        }
    }
    pub fn wake(&self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WAKE, 0, 0);
        }
    }
    pub fn stop(self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        let _ = self.task.join();
    }
}
#[derive(Clone)]
struct Device {
    id: String,
    path: String,
    spec: usize,
    epoch: u64,
    core: Option<Arc<crate::core::Core>>,
}
struct Receiver {
    metrics: Arc<Metrics>,
    manager: Arc<DeviceManager>,
    specs: Vec<RawInputSpec>,
    devices: BTreeMap<usize, Device>,
    owners: BTreeMap<&'static str, NamedGuard>,
}
impl Receiver {
    fn invalidate_handle(&mut self, handle: usize) {
        if let Some(device) = self.devices.get(&handle).cloned() {
            if let Some(core) = device
                .core
                .or_else(|| self.manager.device_profile(&device.id).ok())
            {
                core.clear_connection();
                core.connection("disconnected", None);
            }
            self.devices.retain(|_, other| other.id != device.id);
        }
    }
    unsafe fn rescan(&mut self) {
        self.metrics.scans.fetch_add(1, Ordering::Relaxed);
        for spec in &self.specs {
            if !self.owners.contains_key(spec.adapter_id) {
                if let Some(name) = spec.owner_mutex {
                    if let Ok(owner) = NamedGuard::acquire(name) {
                        self.owners.insert(spec.adapter_id, owner);
                    }
                }
            }
        }
        let mut count = 0;
        if GetRawInputDeviceList(
            std::ptr::null_mut(),
            &mut count,
            std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
        ) == u32::MAX
            || count > 4096
        {
            return;
        }
        let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
        let received = GetRawInputDeviceList(
            list.as_mut_ptr(),
            &mut count,
            std::mem::size_of::<RAWINPUTDEVICELIST>() as u32,
        );
        if received == u32::MAX {
            return;
        }
        list.truncate(received as usize);
        let mut found = BTreeMap::new();
        let mut descriptors: BTreeMap<&'static str, BTreeMap<String, DeviceDescriptor>> =
            BTreeMap::new();
        for (index, spec) in self.specs.iter().enumerate() {
            descriptors.entry(spec.adapter_id).or_default();
            for entry in list.iter().filter(|entry| entry.dwType == RIM_TYPEHID) {
                let mut info = RID_DEVICE_INFO {
                    cbSize: std::mem::size_of::<RID_DEVICE_INFO>() as u32,
                    ..Default::default()
                };
                let mut size = info.cbSize;
                if GetRawInputDeviceInfoW(
                    entry.hDevice,
                    RIDI_DEVICEINFO,
                    (&mut info as *mut RID_DEVICE_INFO).cast(),
                    &mut size,
                ) == u32::MAX
                {
                    continue;
                }
                let hid = info.Anonymous.hid;
                if !spec.matches(
                    hid.dwVendorId,
                    hid.dwProductId,
                    hid.usUsagePage,
                    hid.usUsage,
                ) {
                    continue;
                }
                let Some(path) = device_name(entry.hDevice) else {
                    continue;
                };
                let stable = physical_id(&path, spec.vendor, spec.product).unwrap_or_else(|| {
                    format!("path-{:x}", Sha256::digest(path.to_lowercase().as_bytes()))
                });
                let id = alchemy_device_api::device_id(spec.adapter_id, &stable);
                let logical_path = format!("rawinput:{id}");
                found.insert(
                    entry.hDevice as usize,
                    Device {
                        id: id.clone(),
                        path: logical_path,
                        spec: index,
                        epoch: 0,
                        core: None,
                    },
                );
                let Ok(definition) = self.manager.adapter_definition(spec.adapter_id) else {
                    continue;
                };
                let layout = definition.layout;
                descriptors.get_mut(spec.adapter_id).unwrap().insert(
                    id.clone(),
                    DeviceDescriptor {
                        adapter_id: spec.adapter_id.into(),
                        layout: layout.into(),
                        id,
                        model: spec.name.into(),
                        serial: String::new(),
                        firmware: String::new(),
                        vendor_id: u32::from(spec.vendor),
                        product_id: u32::from(spec.product),
                        capabilities: BTreeMap::from([(
                            "key_input".into(),
                            Capability {
                                available: true,
                                verification: Verification::Unverified,
                                reason: "只读 Raw Input 监听".into(),
                            },
                        )]),
                    },
                );
            }
        }
        let old_ids: BTreeSet<_> = self
            .devices
            .values()
            .map(|device| device.id.clone())
            .collect();
        let new_ids: BTreeSet<_> = found.values().map(|device| device.id.clone()).collect();
        for id in old_ids.difference(&new_ids) {
            if let Ok(core) = self.manager.device_profile(id) {
                core.clear_connection();
                core.connection("disconnected", None);
            }
        }
        for (adapter_id, devices) in descriptors {
            if let Err(error) = self
                .manager
                .discovered_for(adapter_id, devices.values().cloned().collect())
            {
                self.manager.host.log("error", error.to_string());
                continue;
            }
            for (id, descriptor) in devices {
                let Ok(core) = self.manager.device_profile(&id) else {
                    continue;
                };
                let spec = &self.specs[found.values().find(|device| device.id == id).unwrap().spec];
                if spec.owner_mutex.is_some() && !self.owners.contains_key(adapter_id) {
                    core.connection(
                        "busy",
                        Some("其他程序正在占用此设备，请关闭相关程序后重试连接".into()),
                    );
                    continue;
                }
                let old_handles: BTreeSet<_> = self
                    .devices
                    .iter()
                    .filter(|(_, device)| device.id == id)
                    .map(|(handle, _)| *handle)
                    .collect();
                let new_handles: BTreeSet<_> = found
                    .iter()
                    .filter(|(_, device)| device.id == id)
                    .map(|(handle, _)| *handle)
                    .collect();
                if !old_handles.is_empty() && old_handles != new_handles {
                    core.clear_connection();
                }
                core.attach_input_device(descriptor, format!("rawinput:{id}"));
                let epoch = core.inner.lock().unwrap().connection_epoch;
                for device in found.values_mut().filter(|device| device.id == id) {
                    device.epoch = epoch;
                    device.core = Some(core.clone());
                }
            }
        }
        self.devices = found;
    }
    unsafe fn input(&self, handle: HRAWINPUT) {
        let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;
        let mut size = 0;
        if GetRawInputData(handle, RID_INPUT, std::ptr::null_mut(), &mut size, header) == u32::MAX
            || size as usize > MAX_INPUT
        {
            return;
        }
        // GetRawInputData requires DWORD-aligned storage.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let bytes = std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), size as usize);
        let read = GetRawInputData(
            handle,
            RID_INPUT,
            bytes.as_mut_ptr().cast(),
            &mut size,
            header,
        );
        if read == u32::MAX || read as usize > bytes.len() {
            return;
        }
        let Some((source, reports)) = parse_reports(&bytes[..read as usize], header as usize)
        else {
            return;
        };
        let Some(device) = self.devices.get(&source) else {
            return;
        };
        let spec = &self.specs[device.spec];
        if spec.owner_mutex.is_some() && !self.owners.contains_key(spec.adapter_id) {
            return;
        }
        self.metrics
            .reports
            .fetch_add(reports.len() as u64, Ordering::Relaxed);
        if let Some(core) = &device.core {
            for report in reports {
                for input in (spec.decode)(report) {
                    core.dispatch_input(
                        &alchemy_device_api::Connection {
                            device_id: device.id.clone(),
                            path: device.path.clone(),
                            epoch: device.epoch,
                        },
                        input,
                    );
                }
            }
        }
    }
}
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let context = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Receiver;
    if !context.is_null() {
        match msg {
            WM_INPUT => (*context).input(lparam as HRAWINPUT),
            WM_INPUT_DEVICE_CHANGE => {
                if wparam as u32 == GIDC_REMOVAL {
                    (*context).invalidate_handle(lparam as usize);
                }
                (*context).rescan()
            }
            WAKE => (*context).rescan(),
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
unsafe fn run(
    manager: Arc<DeviceManager>,
    specs: Vec<RawInputSpec>,
    ready: &mpsc::SyncSender<std::result::Result<u32, String>>,
    metrics: Arc<Metrics>,
) -> Result<()> {
    let _receiver_owner = NamedGuard::acquire(&format!(
        "Local\\DesktopAlchemy.RawInput.{}",
        std::process::id()
    ))?;
    let class: Vec<u16> = "DesktopAlchemy.RawInput"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let instance = GetModuleHandleW(std::ptr::null());
    let class_info = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    RegisterClassW(&class_info);
    let window = CreateWindowExW(
        0,
        class.as_ptr(),
        std::ptr::null(),
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if window.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let usages: BTreeSet<_> = specs
        .iter()
        .map(|spec| (spec.usage_page, spec.usage))
        .collect();
    let registrations: Vec<_> = usages
        .iter()
        .map(|(page, usage)| RAWINPUTDEVICE {
            usUsagePage: *page,
            usUsage: *usage,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: window,
        })
        .collect();
    if RegisterRawInputDevices(
        registrations.as_ptr(),
        registrations.len() as u32,
        std::mem::size_of::<RAWINPUTDEVICE>() as u32,
    ) == 0
    {
        let error = std::io::Error::last_os_error();
        DestroyWindow(window);
        return Err(error.into());
    }
    metrics.registrations.fetch_add(1, Ordering::Relaxed);
    let context = Box::into_raw(Box::new(Receiver {
        metrics,
        manager,
        specs,
        devices: BTreeMap::new(),
        owners: BTreeMap::new(),
    }));
    SetWindowLongPtrW(window, GWLP_USERDATA, context as isize);
    let _ = ready.send(Ok(GetCurrentThreadId()));
    (*context).rescan();
    let mut message = MSG::default();
    while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
        if message.message == WAKE {
            (*context).rescan();
        } else {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    let removals: Vec<_> = usages
        .iter()
        .map(|(page, usage)| RAWINPUTDEVICE {
            usUsagePage: *page,
            usUsage: *usage,
            dwFlags: RIDEV_REMOVE,
            hwndTarget: std::ptr::null_mut(),
        })
        .collect();
    RegisterRawInputDevices(
        removals.as_ptr(),
        removals.len() as u32,
        std::mem::size_of::<RAWINPUTDEVICE>() as u32,
    );
    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
    DestroyWindow(window);
    drop(Box::from_raw(context));
    Ok(())
}
unsafe fn device_name(handle: HANDLE) -> Option<String> {
    let mut length = 0;
    GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, std::ptr::null_mut(), &mut length);
    if length == 0 || length > 32768 {
        return None;
    }
    let mut data = vec![0u16; length as usize];
    if GetRawInputDeviceInfoW(
        handle,
        RIDI_DEVICENAME,
        data.as_mut_ptr().cast(),
        &mut length,
    ) == u32::MAX
    {
        return None;
    }
    Some(String::from_utf16_lossy(
        &data[..data
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(data.len())],
    ))
}
pub use alchemy_device_api::identity::physical_id;
#[cfg(test)]
use alchemy_device_api::identity::usb_identity;

pub fn parse_reports(bytes: &[u8], header: usize) -> Option<(usize, Vec<&[u8]>)> {
    if header < 16
        || bytes.len() > MAX_INPUT
        || bytes.len() < header.checked_add(8)?
        || u32::from_ne_bytes(bytes[0..4].try_into().ok()?) != RIM_TYPEHID
    {
        return None;
    }
    let source = usize::from_ne_bytes(bytes[8..8 + std::mem::size_of::<usize>()].try_into().ok()?);
    let width = u32::from_ne_bytes(bytes[header..header + 4].try_into().ok()?) as usize;
    let count = u32::from_ne_bytes(bytes[header + 4..header + 8].try_into().ok()?) as usize;
    if width == 0 || count == 0 || count > 4096 {
        return None;
    }
    let start = header + 8;
    let end = start.checked_add(width.checked_mul(count)?)?;
    let declared = u32::from_ne_bytes(bytes[4..8].try_into().ok()?) as usize;
    if declared > bytes.len() || end > declared {
        return None;
    }
    let reports = bytes.get(start..end)?.chunks_exact(width).collect();
    Some((source, reports))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_identity_groups_collections_without_merging_distinct_usb_units() {
        let a = usb_identity("USB\\VID_045E&PID_084D\\serial-a", 0x045e, 0x084d).unwrap();
        assert_eq!(
            Some(a.clone()),
            usb_identity("usb\\vid_045e&pid_084d\\SERIAL-A", 0x045e, 0x084d)
        );
        assert_ne!(
            Some(a),
            usb_identity("USB\\VID_045E&PID_084D\\serial-b", 0x045e, 0x084d)
        );
        assert!(usb_identity("USB\\VID_045E&PID_084D&MI_00\\collection", 0x045e, 0x084d).is_none());
        assert!(usb_identity("USB\\VID_045E&PID_084C\\serial-a", 0x045e, 0x084d).is_none());
    }
    #[test]
    #[cfg(feature = "device-ms-audio-dock")]
    fn removal_invalidates_all_collections_before_a_reused_handle_can_reconnect() {
        use crate::devices::{
            ms_audio_dock::{Adapter, ID},
            DeviceAdapter,
        };
        let descriptor = |id: &str| DeviceDescriptor {
            adapter_id: ID.into(),
            layout: Adapter::default().layout().into(),
            id: id.into(),
            model: "Audio Dock".into(),
            serial: String::new(),
            firmware: String::new(),
            vendor_id: 0x045e,
            product_id: 0x084d,
            capabilities: Default::default(),
        };
        let root = std::env::temp_dir().join(format!("alchemy-remove-{}", uuid::Uuid::new_v4()));
        let manager = DeviceManager::new(crate::core::Core::new(root.clone()).unwrap()).unwrap();
        manager
            .discovered_for(ID, vec![descriptor("a"), descriptor("b")])
            .unwrap();
        let a = manager.device_profile("a").unwrap();
        let b = manager.device_profile("b").unwrap();
        a.attach_input_device(descriptor("a"), "a".into());
        b.attach_input_device(descriptor("b"), "b".into());
        let epoch = a.inner.lock().unwrap().connection_epoch;
        let device = |id: &str, core: Arc<crate::core::Core>| Device {
            id: id.into(),
            path: id.into(),
            spec: 0,
            epoch,
            core: Some(core),
        };
        let mut receiver = Receiver {
            manager: manager.clone(),
            specs: Adapter::default().raw_input(),
            devices: BTreeMap::from([
                (1, device("a", a.clone())),
                (2, device("a", a.clone())),
                (3, device("b", b.clone())),
            ]),
            owners: Default::default(),
            metrics: Default::default(),
        };
        receiver.invalidate_handle(1);
        assert_eq!(receiver.devices.len(), 1);
        assert_eq!(a.snapshot().connection, "disconnected");
        assert_eq!(b.snapshot().connection, "ready");
        a.attach_input_device(descriptor("a"), "a".into());
        a.input_event("a", "a", epoch, 1, true);
        assert!(a.snapshot().pressed.is_empty());
        drop(receiver);
        drop(a);
        drop(b);
        drop(manager);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_reports_are_bounded() {
        for length in 0..40 {
            assert!(parse_reports(&vec![0; length], 24).is_none());
        }
        let mut bytes = vec![0; 36];
        bytes[..4].copy_from_slice(&RIM_TYPEHID.to_ne_bytes());
        bytes[4..8].copy_from_slice(&36u32.to_ne_bytes());
        bytes[24..28].copy_from_slice(&2u32.to_ne_bytes());
        bytes[28..32].copy_from_slice(&2u32.to_ne_bytes());
        bytes[32..].copy_from_slice(&[0x9b, 1, 0x9b, 0]);
        assert_eq!(parse_reports(&bytes, 24).unwrap().1.len(), 2);
        bytes[28..32].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert!(parse_reports(&bytes, 24).is_none());
    }
    #[test]
    #[cfg(feature = "device-ms-audio-dock")]
    fn registers_without_connected_device_and_never_reregisters_on_rescan() {
        use crate::devices::DeviceAdapter;
        let root = std::env::temp_dir().join(format!("alchemy-raw-{}", uuid::Uuid::new_v4()));
        let host = crate::core::Core::new(root.clone()).unwrap();
        let manager = DeviceManager::new(host).unwrap();
        let mut spec = crate::devices::ms_audio_dock::Adapter::default()
            .raw_input()
            .remove(0);
        spec.vendor = 0xfffe;
        spec.owner_mutex = None;
        let service = RawInputService::start(manager.clone(), vec![spec]).unwrap();
        for _ in 0..3 {
            service.wake();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while service.stats().scans < 4 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(service.stats().registrations, 1);
        assert!(manager.snapshot().unwrap().discovered.is_empty());
        let count = service.stats().scans;
        std::thread::sleep(std::time::Duration::from_millis(80));
        assert_eq!(service.stats().scans, count);
        service.stop();
        drop(manager);
        std::fs::remove_dir_all(root).unwrap();
    }
}
