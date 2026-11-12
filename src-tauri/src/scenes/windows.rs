use super::{Facts, Needs, ProcessMatcher, RunningProgram, SceneCondition, SceneSettings};
use anyhow::Result;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use windows::{
    core::w,
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as MediaManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Playback,
    },
    Win32::{
        Foundation::*,
        System::{
            Diagnostics::ToolHelp::*, LibraryLoader::GetModuleHandleW, Threading::*, WinRT::*,
        },
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn details(pid: u32, name: String, path: bool) -> RunningProgram {
    let mut p = RunningProgram {
        pid,
        name,
        path: None,
    };
    if path {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let h = Handle(h);
                let mut buf = vec![0u16; 32768];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(
                    h.0,
                    PROCESS_NAME_WIN32,
                    windows::core::PWSTR(buf.as_mut_ptr()),
                    &mut len,
                )
                .is_ok()
                {
                    p.path = Some(String::from_utf16_lossy(&buf[..len as usize]));
                }
            }
        }
    }
    p
}
pub fn processes(paths: bool) -> Result<Vec<RunningProgram>> {
    processes_filtered(paths, None)
}
fn processes_filtered(paths: bool, names: Option<&HashSet<String>>) -> Result<Vec<RunningProgram>> {
    unsafe {
        let h = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        Process32FirstW(h.0, &mut entry)?;
        let mut result = vec![];
        loop {
            let name = String::from_utf16_lossy(
                &entry.szExeFile[..entry
                    .szExeFile
                    .iter()
                    .position(|v| *v == 0)
                    .unwrap_or(entry.szExeFile.len())],
            );
            let query_path =
                paths && names.is_none_or(|names| names.contains(&name.to_lowercase()));
            result.push(details(entry.th32ProcessID, name, query_path));
            if Process32NextW(h.0, &mut entry).is_err() {
                break;
            }
        }
        Ok(result)
    }
}
fn foreground() -> Option<Option<RunningProgram>> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return Some(None);
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut p = details(pid, String::new(), true);
        if let Some(path) = &p.path {
            p.name = std::path::Path::new(path)
                .file_name()?
                .to_string_lossy()
                .into_owned();
        } else {
            return None;
        }
        Some(Some(p))
    }
}
fn media_playing(manager: &MediaManager) -> Option<bool> {
    let sessions = manager.GetSessions().ok()?;
    let mut unknown = false;
    for session in sessions {
        match session.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()) {
            Ok(Playback::Playing) => return Some(true),
            Ok(_) => {}
            Err(_) => unknown = true,
        }
    }
    if unknown {
        None
    } else {
        Some(false)
    }
}
pub fn sample(needs: Needs) -> Facts {
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
    let f = Facts {
        processes: needs
            .processes
            .then(|| processes(needs.paths).ok())
            .flatten(),
        foreground: if needs.foreground { foreground() } else { None },
        media: if needs.media {
            MediaManager::RequestAsync()
                .and_then(|op| op.get())
                .ok()
                .and_then(|m| media_playing(&m))
        } else {
            None
        },
    };
    if initialized {
        unsafe {
            RoUninitialize();
        }
    }
    f
}
const WAKE: u32 = WM_APP + 18;
unsafe extern "system" fn foreground_event(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    let _ = unsafe { PostThreadMessageW(GetCurrentThreadId(), WAKE, WPARAM(0), LPARAM(0)) };
}
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_POWERBROADCAST || message == WM_TIMECHANGE {
        let _ = unsafe { PostThreadMessageW(GetCurrentThreadId(), WAKE, WPARAM(1), LPARAM(0)) };
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
struct MediaEvents {
    manager: MediaManager,
    token: i64,
    sessions: Vec<(Session, i64)>,
}
impl MediaEvents {
    fn new(thread: u32) -> Result<Self> {
        let manager = MediaManager::RequestAsync()?.get()?;
        let token = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
            unsafe {
                let _ = PostThreadMessageW(thread, WAKE, WPARAM(2), LPARAM(0));
            }
            Ok(())
        }))?;
        let mut result = Self {
            manager,
            token,
            sessions: vec![],
        };
        result.rebind(thread)?;
        Ok(result)
    }
    fn rebind(&mut self, thread: u32) -> Result<()> {
        for (session, token) in self.sessions.drain(..) {
            let _ = session.RemovePlaybackInfoChanged(token);
        }
        for session in self.manager.GetSessions()? {
            let token = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                unsafe {
                    let _ = PostThreadMessageW(thread, WAKE, WPARAM(0), LPARAM(0));
                }
                Ok(())
            }))?;
            self.sessions.push((session, token));
        }
        Ok(())
    }
}
impl Drop for MediaEvents {
    fn drop(&mut self) {
        for (session, token) in self.sessions.drain(..) {
            let _ = session.RemovePlaybackInfoChanged(token);
        }
        let _ = self.manager.RemoveSessionsChanged(self.token);
    }
}
pub struct Collector {
    facts: Arc<Mutex<Facts>>,
    updated: Arc<Mutex<Instant>>,
    thread: u32,
    task: std::thread::JoinHandle<()>,
    interval: u32,
    resync: Arc<AtomicU64>,
}
impl Collector {
    pub fn start(
        needs: Needs,
        settings: &SceneSettings,
        signal: tokio::sync::mpsc::Sender<()>,
        core: Arc<crate::core::Core>,
        metrics: Arc<super::Metrics>,
    ) -> Self {
        let interval = settings.process_interval_ms;
        fn visit(c: &SceneCondition, out: &mut HashSet<String>) {
            match c {
                SceneCondition::All { children } | SceneCondition::Any { children } => {
                    for c in children {
                        visit(c, out);
                    }
                }
                SceneCondition::Not { child } => visit(child, out),
                SceneCondition::Process {
                    matcher: ProcessMatcher::Path { value },
                } => {
                    if let Some(name) = std::path::Path::new(value).file_name() {
                        out.insert(name.to_string_lossy().to_lowercase());
                    }
                }
                _ => {}
            }
        }
        let mut names = HashSet::new();
        for s in settings.scenes.iter().filter(|s| s.enabled) {
            visit(&s.condition, &mut names);
        }
        let resync = Arc::new(AtomicU64::new(0));
        let changed_clock = resync.clone();
        let facts = Arc::new(Mutex::new(Facts::default()));
        let updated = Arc::new(Mutex::new(Instant::now()));
        let out = facts.clone();
        let timestamp = updated.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let task = std::thread::spawn(move || unsafe {
            let initialized = RoInitialize(RO_INIT_MULTITHREADED).is_ok();
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let thread = GetCurrentThreadId();
            let _ = tx.send(thread);
            let hook = if needs.foreground {
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(foreground_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            } else {
                HWINEVENTHOOK::default()
            };
            let mut media = if needs.media {
                MediaEvents::new(thread).ok()
            } else {
                None
            };
            let class = w!("DesktopAlchemySceneEvents");
            let instance = GetModuleHandleW(None).ok().map(|v| HINSTANCE(v.0));
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.unwrap_or_default(),
                lpszClassName: class,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                instance,
                None,
            )
            .ok();
            if needs.processes {
                SetTimer(None, 18, interval, None);
            }
            let mut refresh_processes = true;
            loop {
                let before = out.lock().unwrap().clone();
                let mut f = before.clone();
                if needs.processes && refresh_processes {
                    metrics.process_scans.fetch_add(1, Ordering::Relaxed);
                    f.processes = processes_filtered(needs.paths, Some(&names)).ok();
                    *timestamp.lock().unwrap() = Instant::now();
                }
                if needs.foreground {
                    f.foreground = if hook.is_invalid() {
                        None
                    } else {
                        foreground()
                    };
                }
                if needs.media {
                    f.media = media.as_ref().and_then(|m| media_playing(&m.manager));
                }
                let changed = f.processes != before.processes
                    || f.foreground != before.foreground
                    || f.media != before.media;
                let mut published = out.lock().unwrap();
                if changed {
                    core.invalidate_scene_work();
                }
                *published = f;
                drop(published);
                let _ = signal.try_send(());
                if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
                    break;
                }
                metrics.collector_wakes.fetch_add(1, Ordering::Relaxed);
                refresh_processes =
                    msg.message == WM_TIMER || (msg.message == WAKE && msg.wParam.0 == 1);
                if msg.message == WAKE && msg.wParam.0 == 2 {
                    if let Some(m) = &mut media {
                        if m.rebind(thread).is_err() {
                            media = None;
                        }
                    }
                }
                if msg.message == WAKE && msg.wParam.0 == 1 {
                    changed_clock.fetch_add(1, Ordering::SeqCst);
                    core.invalidate_scene_work();
                    if needs.media {
                        media = MediaEvents::new(thread).ok();
                    }
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if needs.processes {
                let _ = KillTimer(None, 18);
            }
            if !hook.is_invalid() {
                let _ = UnhookWinEvent(hook);
            }
            if let Some(hwnd) = hwnd {
                let _ = DestroyWindow(hwnd);
            }
            drop(media);
            if initialized {
                RoUninitialize();
            }
        });
        let thread = rx.recv().unwrap_or(0);
        Self {
            facts,
            updated,
            thread,
            task,
            interval,
            resync,
        }
    }
    pub fn resync(&self) -> u64 {
        self.resync.load(Ordering::SeqCst)
    }
    pub fn facts(&self) -> Facts {
        let mut f = self.facts.lock().unwrap().clone();
        if self.updated.lock().unwrap().elapsed()
            > Duration::from_millis(self.interval as u64 * 2 + 1000)
        {
            f.processes = None;
        }
        f
    }
    pub async fn stop(self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        let _ = tokio::task::spawn_blocking(move || self.task.join()).await;
    }
}
