use anyhow::Result;
pub fn hidden(command: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command.kill_on_drop(true)
}
pub use alchemy_device_api::ownership::NamedGuard;

/// OS-owned lifetime boundary: children are terminated if the application exits
/// even while an async handshake or shutdown is still pending.
pub struct ChildJob(usize);
impl ChildJob {
    pub fn new() -> Result<Self> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::JobObjects::*;
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let guard = Self(job as usize);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(guard)
        }
        #[cfg(not(windows))]
        {
            Ok(Self(0))
        }
    }

    pub fn terminate(&self) -> Result<()> {
        #[cfg(windows)]
        unsafe {
            if windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0 as _, 1) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> Result<bool> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::JobObjects::*;
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            if QueryInformationJobObject(
                self.0 as _,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as _,
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(info.ActiveProcesses == 0)
        }
        #[cfg(not(windows))]
        {
            Ok(true)
        }
    }

    pub fn attach(pid: u32) -> Result<Self> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::{
                    JobObjects::*,
                    Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
                },
            };
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let guard = Self(job as usize);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let assigned = AssignProcessToJobObject(job, process);
            let error = std::io::Error::last_os_error();
            CloseHandle(process);
            if assigned == 0 {
                return Err(error.into());
            }
            Ok(guard)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            Ok(Self(0))
        }
    }
}

impl Default for ChildJob {
    fn default() -> Self {
        Self::new().expect("创建插件 Job Object")
    }
}

/// Anonymous stdio pipes use Tokio's blocking-file adapter. Cancellation drops the
/// protocol future; terminating the Job also releases blocked pipe reads/writes.
#[cfg(windows)]
pub struct PluginProcess {
    pub job: ChildJob,
    process: std::os::windows::io::OwnedHandle,
    pub pid: u32,
}

#[cfg(windows)]
pub struct SpawnedPlugin {
    pub process: PluginProcess,
    pub input: tokio::fs::File,
    pub output: tokio::fs::File,
    pub error: tokio::fs::File,
}

#[cfg(windows)]
impl PluginProcess {
    pub fn exit_code(&self) -> Result<u32> {
        use std::os::windows::io::AsRawHandle;
        let mut code = 0;
        if unsafe {
            windows_sys::Win32::System::Threading::GetExitCodeProcess(
                self.process.as_raw_handle(),
                &mut code,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(code)
    }
    pub fn exited(&self) -> Result<bool> {
        use std::os::windows::io::AsRawHandle;
        let result = unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(
                self.process.as_raw_handle(),
                0,
            )
        };
        if result == windows_sys::Win32::Foundation::WAIT_FAILED {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(result == windows_sys::Win32::Foundation::WAIT_OBJECT_0)
    }

    pub fn spawn(exe: &std::path::Path, dir: &std::path::Path) -> Result<SpawnedPlugin> {
        use std::os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        };
        use windows_sys::Win32::{
            Foundation::*,
            Security::SECURITY_ATTRIBUTES,
            System::{Pipes::CreatePipe, Threading::*},
        };
        unsafe fn pipe() -> Result<(OwnedHandle, OwnedHandle)> {
            let mut read = std::ptr::null_mut();
            let mut write = std::ptr::null_mut();
            let security = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: std::ptr::null_mut(),
                bInheritHandle: 1,
            };
            if CreatePipe(&mut read, &mut write, &security, 0) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok((
                OwnedHandle::from_raw_handle(read),
                OwnedHandle::from_raw_handle(write),
            ))
        }
        unsafe {
            let job = ChildJob::new()?;
            let (child_in, parent_in) = pipe()?;
            let (parent_out, child_out) = pipe()?;
            let (parent_err, child_err) = pipe()?;
            for handle in [&parent_in, &parent_out, &parent_err] {
                if SetHandleInformation(handle.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) == 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            let mut size = 0;
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut size);
            let mut buffer = vec![0_usize; size.div_ceil(std::mem::size_of::<usize>())];
            let list = buffer.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
            if InitializeProcThreadAttributeList(list, 2, 0, &mut size) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);
            impl Drop for Attributes {
                fn drop(&mut self) {
                    unsafe { DeleteProcThreadAttributeList(self.0) }
                }
            }
            let _attributes = Attributes(list);
            let jobs = [job.0 as HANDLE];
            let handles = [
                child_in.as_raw_handle(),
                child_out.as_raw_handle(),
                child_err.as_raw_handle(),
            ];
            for (attribute, pointer, length) in [
                (
                    PROC_THREAD_ATTRIBUTE_JOB_LIST,
                    jobs.as_ptr() as *const std::ffi::c_void,
                    std::mem::size_of_val(&jobs),
                ),
                (
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                    handles.as_ptr() as *const std::ffi::c_void,
                    std::mem::size_of_val(&handles),
                ),
            ] {
                if UpdateProcThreadAttribute(
                    list,
                    0,
                    attribute as usize,
                    pointer,
                    length,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                ) == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            let mut startup: STARTUPINFOEXW = std::mem::zeroed();
            startup.StartupInfo.cb = std::mem::size_of_val(&startup) as u32;
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = child_in.as_raw_handle();
            startup.StartupInfo.hStdOutput = child_out.as_raw_handle();
            startup.StartupInfo.hStdError = child_err.as_raw_handle();
            startup.lpAttributeList = list;
            let exe_w: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
            let dir_w: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut command: Vec<u16> = format!("\"{}\"", exe.display())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            if CreateProcessW(
                exe_w.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
                std::ptr::null(),
                dir_w.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let process = OwnedHandle::from_raw_handle(info.hProcess);
            drop(OwnedHandle::from_raw_handle(info.hThread));
            Ok(SpawnedPlugin {
                process: Self {
                    job,
                    process,
                    pid: info.dwProcessId,
                },
                input: tokio::fs::File::from_std(std::fs::File::from(parent_in)),
                output: tokio::fs::File::from_std(std::fs::File::from(parent_out)),
                error: tokio::fs::File::from_std(std::fs::File::from(parent_err)),
            })
        }
    }
}
impl Drop for ChildJob {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(
                self.0 as windows_sys::Win32::Foundation::HANDLE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prevents_concurrent_owner_and_releases_on_drop() {
        let name = format!("Local\\m18-test-{}", uuid::Uuid::new_v4());
        let guard = NamedGuard::acquire(&name).unwrap();
        assert!(NamedGuard::acquire(&name).is_err());
        drop(guard);
        assert!(NamedGuard::acquire(&name).is_ok());
    }
    #[tokio::test]
    async fn job_closure_terminates_owned_child() {
        let mut command = tokio::process::Command::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 30",
        ]);
        hidden(&mut command);
        command.kill_on_drop(false);
        let mut child = command.spawn().unwrap();
        let job = ChildJob::attach(child.id().unwrap()).unwrap();
        drop(job);
        let _status = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap();
        // Windows kill-on-close can use exit code 0. Finishing inside the deadline,
        // rather than after the child's 30-second sleep, proves job ownership.
        assert!(child.id().is_none());
    }
}
