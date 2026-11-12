//! A presentation-only window. Device ownership and editor drafts stay in their existing hosts.
use crate::{
    app::AppState,
    desktop,
    model::{Snapshot, TrayAction, TrayState},
};
use anyhow::{anyhow, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

pub const LABEL: &str = "tray-panel";
const WIDTH: f64 = 276.0;
const HEIGHT: f64 = 274.0;

#[derive(Default)]
pub struct TrayPanelHost {
    ready: AtomicBool,
    anchor: Mutex<Option<Bounds>>,
    dismissed_on_icon: Mutex<Option<Instant>>,
    right_down_was_visible: AtomicBool,
    last_state: Mutex<Option<TrayState>>,
    pub pending_navigation: Mutex<Option<TrayAction>>,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
impl Bounds {
    fn contains(self, point: PhysicalPosition<f64>) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }
}

// All inputs and outputs are physical pixels. Only panel dimensions/gaps start as DIP.
fn position(
    anchor: Bounds,
    work: Bounds,
    scale: f64,
) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let width = (WIDTH * scale).round().min(work.width).max(1.0);
    let height = (HEIGHT * scale).round().min(work.height).max(1.0);
    let gap = 4.0 * scale;
    let above = anchor.y - height - gap;
    let below = anchor.y + anchor.height + gap;
    let y = if above >= work.y { above } else { below };
    let x = anchor.x + anchor.width - width;
    (
        PhysicalPosition::new(
            x.clamp(work.x, work.x + work.width - width).round() as i32,
            y.clamp(work.y, work.y + work.height - height).round() as i32,
        ),
        PhysicalSize::new(width as u32, height as u32),
    )
}

pub fn fallback_menu(handle: &tauri::AppHandle) -> Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem};
    let chinese = handle.state::<desktop::DesktopState>().language().locale
        == crate::model::UiLocale::Chinese;
    let show = MenuItem::with_id(
        handle,
        "show",
        if chinese {
            "显示窗口"
        } else {
            "Show window"
        },
        true,
        None::<&str>,
    )?;
    let exit = MenuItem::with_id(
        handle,
        "exit",
        if chinese { "退出" } else { "Quit" },
        true,
        None::<&str>,
    )?;
    Ok(Menu::with_items(handle, &[&show, &exit])?)
}

pub fn refresh_language(handle: &tauri::AppHandle) -> Result<()> {
    let chinese = handle.state::<desktop::DesktopState>().language().locale
        == crate::model::UiLocale::Chinese;
    if let Some(window) = handle.get_webview_window(LABEL) {
        window.set_title(if chinese {
            "Desktop Alchemy · 托盘"
        } else {
            "Desktop Alchemy · Tray"
        })?;
    }
    if !handle.state::<TrayPanelHost>().ready.load(Ordering::SeqCst) {
        if let Some(tray) = handle.tray_by_id("main-tray") {
            tray.set_menu(Some(fallback_menu(handle)?))?;
        }
    }
    Ok(())
}

pub fn fallback(handle: &tauri::AppHandle) {
    if handle
        .state::<desktop::DesktopState>()
        .quitting
        .load(Ordering::SeqCst)
    {
        return;
    }
    handle
        .state::<TrayPanelHost>()
        .ready
        .store(false, Ordering::SeqCst);
    hide(handle);
    if let Some(tray) = handle.tray_by_id("main-tray") {
        let result = fallback_menu(handle).and_then(|menu| {
            #[cfg(all(debug_assertions, feature = "diagnostics"))]
            let labels: Vec<String> = menu
                .items()?
                .iter()
                .filter_map(|item| {
                    if let tauri::menu::MenuItemKind::MenuItem(item) = item {
                        item.text().ok()
                    } else {
                        None
                    }
                })
                .collect();
            tray.set_menu(Some(menu))?;
            #[cfg(all(debug_assertions, feature = "diagnostics"))]
            if std::env::var_os("DESKTOP_ALCHEMY_UI_TEST_ROOT").is_some() {
                let _ = handle.emit_to("main", "diagnostics-tray-menu", labels);
            }
            Ok(())
        });
        if let Err(error) = result {
            handle
                .state::<AppState>()
                .core
                .log("error", format!("托盘备用菜单创建失败：{error}"));
            // Never hide the only recoverable window if both tray presentations failed.
            handle
                .state::<desktop::DesktopState>()
                .tray_ready
                .store(false, Ordering::SeqCst);
            desktop::show(handle);
        }
    }
}

pub fn install_window(app: &tauri::App) -> Result<()> {
    tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("tray.html".into()))
        .title(
            if app.state::<desktop::DesktopState>().language().locale
                == crate::model::UiLocale::Chinese
            {
                "Desktop Alchemy · 托盘"
            } else {
                "Desktop Alchemy · Tray"
            },
        )
        .inner_size(WIDTH, HEIGHT)
        .visible(false)
        .focused(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .on_page_load(|window, event| {
            if event.event() == tauri::webview::PageLoadEvent::Started {
                fallback(window.app_handle());
            }
        })
        .build()?;
    // Only the existing SDK-free, isolated debug mode exposes this test event.
    // It opens the real native window through the same geometry/focus path as a tray click.
    #[cfg(all(debug_assertions, feature = "diagnostics"))]
    if std::env::var_os("DESKTOP_ALCHEMY_UI_TEST_ROOT").is_some() {
        use tauri::Listener;
        let fallback_handle = app.handle().clone();
        app.listen("diagnostics-tray-fallback", move |_| {
            let handle = fallback_handle.clone();
            let _ = fallback_handle.run_on_main_thread(move || fallback(&handle));
        });
        let handle = app.handle().clone();
        app.listen("diagnostics-tray-toggle", move |_| {
            let h = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let result = (|| -> Result<()> {
                    let tray = h.tray_by_id("main-tray").ok_or_else(|| {
                        anyhow!(alchemy_device_api::UiFailure::new(
                            "backend.b247",
                            "托盘不存在"
                        ))
                    })?;
                    let rect = if let Some(rect) = tray.rect()? {
                        rect
                    } else {
                        // Windows may not give hidden overflow icons a rectangle until opened.
                        let main = h.get_webview_window("main").ok_or_else(|| {
                            anyhow!(alchemy_device_api::UiFailure::new(
                                "backend.b248",
                                "主窗口不存在"
                            ))
                        })?;
                        let monitor = main.primary_monitor()?.ok_or_else(|| {
                            anyhow!(alchemy_device_api::UiFailure::new(
                                "backend.b249",
                                "显示器不存在"
                            ))
                        })?;
                        let work = monitor.work_area();
                        tauri::Rect {
                            position: PhysicalPosition::new(
                                work.position.x + work.size.width as i32 - 40,
                                work.position.y + work.size.height as i32,
                            )
                            .into(),
                            size: PhysicalSize::new(32, 32).into(),
                        }
                    };
                    let point = rect.position.to_physical::<f64>(1.0);
                    toggle(&h, rect, point)
                })();
                if let Err(error) = result {
                    h.state::<AppState>()
                        .core
                        .log("error", format!("托盘隔离检查失败：{error}"));
                }
            });
        });
    }
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if !handle.state::<TrayPanelHost>().ready.load(Ordering::SeqCst) {
            handle
                .state::<AppState>()
                .core
                .log("warning", "托盘面板未能加载，已保留系统菜单");
            fallback(&handle);
        }
    });
    Ok(())
}

pub fn ready(handle: &tauri::AppHandle) -> Result<TrayState> {
    let tray = handle.tray_by_id("main-tray").ok_or_else(|| {
        anyhow!(alchemy_device_api::UiFailure::new(
            "backend.b250",
            "托盘尚未创建"
        ))
    })?;
    tray.set_menu(None::<tauri::menu::Menu<tauri::Wry>>)?;
    handle
        .state::<TrayPanelHost>()
        .ready
        .store(true, Ordering::SeqCst);
    Ok(TrayState::from(&handle.state::<AppState>().core.snapshot()))
}

pub fn hide(handle: &tauri::AppHandle) {
    if let Some(window) = handle.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

pub fn blur(window: &tauri::Window) {
    let handle = window.app_handle();
    let host = handle.state::<TrayPanelHost>();
    if let (Some(anchor), Ok(cursor)) = (*host.anchor.lock().unwrap(), window.cursor_position()) {
        if anchor.contains(cursor) {
            *host.dismissed_on_icon.lock().unwrap() = Some(Instant::now());
        }
    }
    hide(handle);
}

pub fn right_down(handle: &tauri::AppHandle) {
    let host = handle.state::<TrayPanelHost>();
    let visible = handle
        .get_webview_window(LABEL)
        .is_some_and(|w| w.is_visible().unwrap_or(false));
    let just_dismissed = host
        .dismissed_on_icon
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|time| time.elapsed() < Duration::from_millis(500));
    host.right_down_was_visible
        .store(visible || just_dismissed, Ordering::SeqCst);
}

pub fn toggle(
    handle: &tauri::AppHandle,
    rect: tauri::Rect,
    cursor: PhysicalPosition<f64>,
) -> Result<()> {
    let host = handle.state::<TrayPanelHost>();
    if !host.ready.load(Ordering::SeqCst) {
        return Ok(());
    }
    let window = handle.get_webview_window(LABEL).ok_or_else(|| {
        anyhow!(alchemy_device_api::UiFailure::new(
            "backend.b251",
            "托盘面板不存在"
        ))
    })?;
    // Clicking the icon may blur/hide the panel before Windows delivers button-up.
    let just_dismissed = host
        .dismissed_on_icon
        .lock()
        .unwrap()
        .take()
        .is_some_and(|time| time.elapsed() < Duration::from_millis(500));
    let was_visible = host.right_down_was_visible.swap(false, Ordering::SeqCst);
    if window.is_visible()? || just_dismissed || was_visible {
        hide(handle);
        return Ok(());
    }
    let monitor = window
        .monitor_from_point(cursor.x, cursor.y)?
        .or(window.primary_monitor()?)
        .ok_or_else(|| {
            anyhow!(alchemy_device_api::UiFailure::new(
                "backend.b252",
                "无法定位托盘显示器"
            ))
        })?;
    let scale = monitor.scale_factor();
    let p = rect.position.to_physical::<f64>(scale);
    let s = rect.size.to_physical::<f64>(scale);
    let anchor = Bounds {
        x: p.x,
        y: p.y,
        width: s.width,
        height: s.height,
    };
    let work = monitor.work_area();
    let (point, size) = position(
        anchor,
        Bounds {
            x: work.position.x as f64,
            y: work.position.y as f64,
            width: work.size.width as f64,
            height: work.size.height as f64,
        },
        scale,
    );
    *host.anchor.lock().unwrap() = Some(anchor);
    window.set_size(size)?;
    window.set_position(point)?;
    let state = TrayState::from(&handle.state::<AppState>().core.snapshot());
    *host.last_state.lock().unwrap() = Some(state.clone());
    window.emit("tray-opened", state)?;
    window.show()?;
    window.set_focus()?;
    Ok(())
}

pub fn update(handle: &tauri::AppHandle, snapshot: &Snapshot) {
    let host = handle.state::<TrayPanelHost>();
    if !host.ready.load(Ordering::SeqCst) {
        return;
    }
    if let Some(window) = handle.get_webview_window(LABEL) {
        if !window.is_visible().unwrap_or(false) {
            return;
        }
        let state = TrayState::from(snapshot);
        let mut previous = host.last_state.lock().unwrap();
        if previous.as_ref() != Some(&state) && window.emit("tray-state", &state).is_ok() {
            *previous = Some(state);
        }
    }
}

pub fn action(handle: &tauri::AppHandle, action: TrayAction) -> Result<()> {
    hide(handle);
    match action {
        TrayAction::Exit => desktop::request_exit(handle),
        TrayAction::Show => desktop::show(handle),
        action => {
            desktop::show(handle);
            if handle
                .state::<desktop::DesktopState>()
                .frontend_ready
                .load(Ordering::SeqCst)
            {
                handle.emit_to("main", "tray-navigate", action)?;
            } else {
                *handle
                    .state::<TrayPanelHost>()
                    .pending_navigation
                    .lock()
                    .unwrap() = Some(action);
            }
        }
    }
    Ok(())
}

pub fn allows_command(window: &str, command: &str) -> bool {
    window != LABEL
        || matches!(
            command,
            "tray_panel_ready" | "tray_action" | "dismiss_tray" | "ui_language"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_respects_work_area_dpi_and_negative_coordinates() {
        for scale in [1.0, 1.5, 2.0] {
            for (x, y) in [(0.0, 0.0), (-1920.0, -1080.0), (1920.0, 0.0)] {
                let work = Bounds {
                    x,
                    y,
                    width: 1920.0,
                    height: 1040.0,
                };
                for (ax, ay) in [
                    (x, y),
                    (x + 1880.0, y),
                    (x, y + 1040.0),
                    (x + 1880.0, y + 1040.0),
                    (x + 1200.0, y + 600.0),
                ] {
                    let (p, s) = position(
                        Bounds {
                            x: ax,
                            y: ay,
                            width: 32.0,
                            height: 32.0,
                        },
                        work,
                        scale,
                    );
                    assert!(p.x as f64 >= x && p.y as f64 >= y);
                    assert!((p.x as f64 + s.width as f64) <= x + work.width);
                    assert!((p.y as f64 + s.height as f64) <= y + work.height);
                    assert_eq!(s.width, (WIDTH * scale).round() as u32);
                    if ay == y {
                        assert_eq!(p.y, (y + 32.0 + 4.0 * scale).round() as i32);
                    }
                    if ay == y + 1040.0 {
                        assert_eq!(p.y, (ay - HEIGHT * scale - 4.0 * scale).round() as i32);
                    }
                }
            }
        }
        let (p, s) = position(
            Bounds {
                x: 9999.0,
                y: -100.0,
                width: 32.0,
                height: 32.0,
            },
            Bounds {
                x: -100.0,
                y: 0.0,
                width: 200.0,
                height: 180.0,
            },
            1.5,
        );
        assert_eq!(p, PhysicalPosition::new(-100, 0));
        assert_eq!(s, PhysicalSize::new(200, 180));
    }
    #[test]
    fn tray_cannot_invoke_editor_device_or_secret_commands() {
        for command in [
            "snapshot",
            "hardware",
            "save_config",
            "confirm_exit",
            "mcp_token",
            "frontend_ready",
            "import_backup",
            "set_ui_language",
        ] {
            assert!(!allows_command(LABEL, command));
            assert!(allows_command("main", command));
        }
        for command in [
            "tray_panel_ready",
            "tray_action",
            "dismiss_tray",
            "ui_language",
        ] {
            assert!(allows_command(LABEL, command));
        }
    }
}
