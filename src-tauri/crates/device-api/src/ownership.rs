use anyhow::{bail, Result};
pub struct NamedGuard(usize);
impl NamedGuard {
    pub fn acquire(name: &str) -> Result<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS},
                System::Threading::CreateMutexW,
            };
            let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(handle) };
                bail!(crate::UiFailure::new("backend.b338", "其他程序正在占用此设备，请关闭相关程序后重试"))
            }
            Ok(Self(handle as usize))
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Ok(Self(0))
        }
    }
}
impl Drop for NamedGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(
                self.0 as windows_sys::Win32::Foundation::HANDLE,
            );
        }
    }
}
