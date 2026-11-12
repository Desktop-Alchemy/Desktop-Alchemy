//! Windows HID class transport. Handles never escape this module.
use super::protocol::{self, Transport};
use anyhow::{bail, ensure, Context, Result};
use std::{
    mem::size_of,
    ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Devices::{DeviceAndDriverInstallation::*, HumanInterfaceDevice::*},
    Foundation::*,
    Storage::FileSystem::*,
    System::{Threading::*, IO::*},
};
const IOCTL_HID_SET_FEATURE: u32 = (0x0b << 16) | (100 << 2) | 1;
// WDK hidclass.h: HID_OUT_CTL_CODE(104).
const IOCTL_HID_GET_INPUT_REPORT: u32 = (0x0b << 16) | (104 << 2) | 2;
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Set(HDEVINFO);
impl Drop for Set {
    fn drop(&mut self) {
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

pub(super) struct Session<'a> {
    handle: Handle,
    _owner: alchemy_device_api::ownership::NamedGuard,
    current: &'a (dyn Fn() -> bool + Sync),
    failed: bool,
}
impl<'a> Session<'a> {
    pub fn open(device_id: &str, current: &'a (dyn Fn() -> bool + Sync)) -> Result<Self> {
        ensure!(current(), alchemy_device_api::UiFailure::new("backend.e031", "设备连接已改变"));
        let owner = alchemy_device_api::ownership::NamedGuard::acquire(&format!(
            "Local\\DesktopAlchemy.AudioDock.Settings.{device_id}"
        ))?;
        let path = find_path(device_id)?;
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let e = std::io::Error::last_os_error();
            if matches!(e.raw_os_error(), Some(5 | 32)) {
                bail!(alchemy_device_api::UiFailure::new("backend.b326", "其他程序正在占用此设备，请关闭相关程序后重新读取"));
            }
            bail!("无法打开设备设置通道：{e}");
        }
        let handle = Handle(handle);
        let caps = capabilities(handle.0)?;
        ensure!(
            caps.FeatureReportByteLength == 64 && caps.InputReportByteLength == 64, alchemy_device_api::UiFailure::new("backend.e032", "设备报告长度不受支持"));
        ensure!(current(), alchemy_device_api::UiFailure::new("backend.e031", "设备连接已改变"));
        Ok(Self {
            handle,
            _owner: owner,
            current,
            failed: false,
        })
    }
    fn io(&mut self, code: u32, data: &mut [u8; 64]) -> Result<usize> {
        ensure!(
            !self.failed && (self.current)(), alchemy_device_api::UiFailure::new("backend.e033", "设置会话已失效，请重新读取"));
        unsafe {
            let event = CreateEventW(ptr::null(), 1, 0, ptr::null());
            ensure!(!event.is_null(), alchemy_device_api::UiFailure::new("backend.e034", "无法创建设备请求事件"));
            let event = Handle(event);
            let mut overlapped = OVERLAPPED {
                hEvent: event.0,
                ..Default::default()
            };
            let mut count = 0;
            let ok = DeviceIoControl(
                self.handle.0,
                code,
                data.as_ptr().cast(),
                64,
                data.as_mut_ptr().cast(),
                64,
                &mut count,
                &mut overlapped,
            );
            if ok == 0 && GetLastError() != ERROR_IO_PENDING {
                self.failed = true;
                bail!("设备请求失败：{}", std::io::Error::last_os_error());
            }
            if ok == 0 {
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    if !(self.current)() || Instant::now() >= deadline {
                        self.failed = true;
                        CancelIoEx(self.handle.0, &overlapped);
                        // Keep OVERLAPPED, buffer and event alive until cancellation completes.
                        GetOverlappedResult(self.handle.0, &overlapped, &mut count, 1);
                        bail!(alchemy_device_api::UiFailure::new("backend.b327", "设备请求超时或连接已改变；请重新读取，勿重复提交写入"));
                    }
                    match WaitForSingleObject(event.0, 50) {
                        WAIT_OBJECT_0 => break,
                        WAIT_TIMEOUT => {}
                        _ => {
                            self.failed = true;
                            CancelIoEx(self.handle.0, &overlapped);
                            GetOverlappedResult(self.handle.0, &overlapped, &mut count, 1);
                            bail!(alchemy_device_api::UiFailure::new("backend.b328", "设备请求等待失败"));
                        }
                    }
                }
                if GetOverlappedResult(self.handle.0, &overlapped, &mut count, 0) == 0 {
                    self.failed = true;
                    bail!("设备请求未完成：{}", std::io::Error::last_os_error());
                }
            }
            ensure!((self.current)(), alchemy_device_api::UiFailure::new("backend.e031", "设备连接已改变"));
            Ok(count as usize)
        }
    }
}
impl Transport for Session<'_> {
    fn exchange(&mut self, command: u8, payload: &[u8]) -> Result<Vec<u8>> {
        let mut report = protocol::request(command, payload)?;
        self.io(IOCTL_HID_SET_FEATURE, &mut report)?;
        // One solicited input report per command, never a continuous polling loop.
        let mut reply = [0; 64];
        reply[0] = 0x27;
        let count = self.io(IOCTL_HID_GET_INPUT_REPORT, &mut reply)?;
        let result = protocol::response(command, &reply[..count.min(64)]).with_context(|| {
            format!(
                "设置命令 {command:02X} 回复长度 {count}，报告头 {:02X?}",
                &reply[..4]
            )
        });
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
fn capabilities(handle: HANDLE) -> Result<HIDP_CAPS> {
    unsafe {
        let mut data = 0isize;
        ensure!(
            HidD_GetPreparsedData(handle, &mut data), alchemy_device_api::UiFailure::new("backend.e035", "无法读取设备 HID 描述符"));
        let mut caps = HIDP_CAPS::default();
        let status = HidP_GetCaps(data, &mut caps);
        HidD_FreePreparsedData(data);
        ensure!(status == HIDP_STATUS_SUCCESS, alchemy_device_api::UiFailure::new("backend.e036", "设备 HID 描述符无效"));
        Ok(caps)
    }
}
fn find_path(device_id: &str) -> Result<String> {
    unsafe {
        let mut guid = std::mem::zeroed();
        HidD_GetHidGuid(&mut guid);
        let set = SetupDiGetClassDevsW(
            &guid,
            ptr::null(),
            ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        ensure!(set != INVALID_HANDLE_VALUE as isize, alchemy_device_api::UiFailure::new("backend.e037", "无法枚举设备配置通道"));
        let set = Set(set);
        let mut found = vec![];
        for index in 0..4096 {
            let mut interface = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInterfaces(set.0, ptr::null(), &guid, index, &mut interface) == 0 {
                ensure!(GetLastError() == ERROR_NO_MORE_ITEMS, alchemy_device_api::UiFailure::new("backend.e038", "设备枚举失败"));
                break;
            }
            let mut needed = 0;
            SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &interface,
                ptr::null_mut(),
                0,
                &mut needed,
                ptr::null_mut(),
            );
            if !(8..=65536).contains(&needed) {
                continue;
            }
            let mut storage = vec![0u64; (needed as usize).div_ceil(8)];
            let detail = storage
                .as_mut_ptr()
                .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &interface,
                detail,
                needed,
                ptr::null_mut(),
                ptr::null_mut(),
            ) == 0
            {
                continue;
            }
            let chars = std::slice::from_raw_parts(
                (*detail).DevicePath.as_ptr(),
                (needed as usize - 4) / 2,
            );
            let end = chars
                .iter()
                .position(|v| *v == 0)
                .context(alchemy_device_api::UiFailure::new("backend.b329", "设备路径未终止"))?;
            let path = String::from_utf16(&chars[..end])?;
            if !path.to_lowercase().contains("vid_045e&pid_084d") {
                continue;
            }
            let h = CreateFileW(
                (*detail).DevicePath.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            );
            if h == INVALID_HANDLE_VALUE {
                continue;
            }
            let h = Handle(h);
            let mut attrs = HIDD_ATTRIBUTES {
                Size: size_of::<HIDD_ATTRIBUTES>() as u32,
                ..Default::default()
            };
            if !HidD_GetAttributes(h.0, &mut attrs)
                || attrs.VendorID != 0x045e
                || attrs.ProductID != 0x084d
            {
                continue;
            }
            let Ok(caps) = capabilities(h.0) else {
                continue;
            };
            if caps.UsagePage != 0xff07 || caps.Usage != 0x0222 {
                continue;
            }
            let Some(identity) = alchemy_device_api::identity::physical_id(&path, 0x045e, 0x084d)
            else {
                continue;
            };
            if format!(
                "rawinput:{}",
                alchemy_device_api::device_id(super::ID, &identity)
            ) == device_id
            {
                found.push(path);
            }
        }
        ensure!(found.len() == 1, alchemy_device_api::UiFailure::new("backend.e039", "未找到唯一匹配的 Audio Dock 设置通道"));
        Ok(found.remove(0))
    }
}
