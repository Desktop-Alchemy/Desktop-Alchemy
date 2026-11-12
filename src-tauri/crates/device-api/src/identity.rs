use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Devices::{DeviceAndDriverInstallation::*, Properties::*},
    Foundation::*,
};
pub fn usb_identity(instance: &str, vendor: u16, product: u16) -> Option<String> {
    let instance = instance.to_lowercase();
    let prefix = format!("usb\\vid_{vendor:04x}&pid_{product:04x}\\");
    instance
        .starts_with(&prefix)
        .then(|| format!("usb-{:x}", Sha256::digest(instance.as_bytes())))
}
/// Resolve the owning USB instance for a HID interface path.
///
/// # Safety
/// Runs Windows SetupAPI calls. The caller must use this on Windows without
/// concurrently unloading system libraries used by SetupAPI; no raw pointers
/// or ownership obligations are transferred through this interface.
pub unsafe fn physical_id(path: &str, vendor: u16, product: u16) -> Option<String> {
    struct Set(HDEVINFO);
    impl Drop for Set {
        fn drop(&mut self) {
            unsafe {
                SetupDiDestroyDeviceInfoList(self.0);
            }
        }
    }
    let set = SetupDiCreateDeviceInfoList(std::ptr::null(), std::ptr::null_mut());
    if set == INVALID_HANDLE_VALUE as isize {
        return None;
    }
    let set = Set(set);
    let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let mut interface = SP_DEVICE_INTERFACE_DATA {
        cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
        ..Default::default()
    };
    if SetupDiOpenDeviceInterfaceW(set.0, path.as_ptr(), 0, &mut interface) == 0 {
        return None;
    }
    let mut needed = 0;
    SetupDiGetDeviceInterfaceDetailW(
        set.0,
        &interface,
        std::ptr::null_mut(),
        0,
        &mut needed,
        std::ptr::null_mut(),
    );
    if !(8..=65536).contains(&needed) {
        return None;
    }
    let mut detail = vec![0u64; (needed as usize).div_ceil(8)];
    let detail = detail
        .as_mut_ptr()
        .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
    (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
    let mut info = SP_DEVINFO_DATA {
        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    if SetupDiGetDeviceInterfaceDetailW(set.0, &interface, detail, needed, &mut needed, &mut info)
        == 0
    {
        return None;
    }
    // The composite USB parent is shared by all collections and distinguishes
    // two physical units even when a vendor supplies the same ContainerId.
    let mut node = info.DevInst;
    for _ in 0..32 {
        let mut length = 0;
        if CM_Get_Device_ID_Size(&mut length, node, 0) == CR_SUCCESS && length < 32768 {
            let mut value = vec![0u16; length as usize + 1];
            if CM_Get_Device_IDW(node, value.as_mut_ptr(), value.len() as u32, 0) == CR_SUCCESS {
                let name = String::from_utf16_lossy(&value[..length as usize]);
                if let Some(id) = usb_identity(&name, vendor, product) {
                    return Some(id);
                }
            }
        }
        let mut parent = 0;
        if CM_Get_Parent(&mut parent, node, 0) != CR_SUCCESS || parent == node {
            break;
        }
        node = parent;
    }
    let mut kind = 0;
    let mut guid = [0u8; 16];
    if SetupDiGetDevicePropertyW(
        set.0,
        &info,
        &DEVPKEY_Device_ContainerId,
        &mut kind,
        guid.as_mut_ptr(),
        16,
        &mut needed,
        0,
    ) == 0
        || kind != DEVPROP_TYPE_GUID
        || guid == [0; 16]
    {
        return None;
    }
    Some(guid.iter().map(|byte| format!("{byte:02x}")).collect())
}
