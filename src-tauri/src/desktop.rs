use crate::{
    app::AppState,
    model::{
        AppSettings, CloseBehavior, LanguagePreference, ModelAppearance, UiLanguageState, UiLocale,
    },
    storage,
};
use anyhow::Result;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{Emitter, Manager};

pub struct Preferences {
    settings: Mutex<AppSettings>,
    settings_error: Mutex<Option<String>>,
    system_language: Mutex<UiLocale>,
}
pub struct DesktopState {
    pub preferences: std::sync::Arc<Preferences>,
    pub tray_ready: AtomicBool,
    pub frontend_ready: AtomicBool,
    pub quitting: AtomicBool,
    pub restarting: AtomicBool,
    // Held until the application and its SDK have shut down.
    pub instance: crate::process_lock::NamedGuard,
}
impl std::ops::Deref for DesktopState {
    type Target = Preferences;
    fn deref(&self) -> &Self::Target {
        &self.preferences
    }
}
impl DesktopState {
    pub fn new(root: &Path, instance: crate::process_lock::NamedGuard) -> Self {
        Self {
            preferences: Preferences::load(root),
            tray_ready: AtomicBool::new(false),
            frontend_ready: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            restarting: AtomicBool::new(false),
            instance,
        }
    }
    pub fn hide_on_close(&self) -> bool {
        self.tray_ready.load(Ordering::SeqCst)
            && self.settings.lock().unwrap().close_behavior == CloseBehavior::Tray
    }
}
impl Preferences {
    pub fn replace_checked(
        &self,
        root: &Path,
        settings: AppSettings,
        expected: &str,
    ) -> Result<AppSettings> {
        let mut current = self.settings.lock().unwrap();
        crate::revisions::check(Some(expected), &*current)?;
        storage::validate_app_settings(&settings)?;
        storage::save_json(root, "app-settings.json", &settings)?;
        if settings.language == LanguagePreference::System {
            *self.system_language.lock().unwrap() = crate::i18n::system_locale();
        }
        *current = settings.clone();
        *self.settings_error.lock().unwrap() = None;
        Ok(settings)
    }
    pub fn load(root: &Path) -> std::sync::Arc<Self> {
        let loaded = if root.join("app-settings.json").exists() {
            std::fs::read(root.join("app-settings.json"))
                .map_err(anyhow::Error::from)
                .and_then(|data| {
                    let settings = serde_json::from_slice::<AppSettings>(&data)?;
                    storage::validate_app_settings(&settings)?;
                    Ok(settings)
                })
        } else {
            Ok(AppSettings::default())
        };
        let (settings, error) = match loaded {
            Ok(settings) => (settings, None),
            Err(e) => (
                AppSettings {
                    close_behavior: CloseBehavior::Exit,
                    ..Default::default()
                },
                Some(format!("应用设置损坏，已保留原文件：{e}")),
            ),
        };
        std::sync::Arc::new(Self {
            settings: Mutex::new(settings),
            settings_error: Mutex::new(error),
            system_language: Mutex::new(crate::i18n::system_locale()),
        })
    }
    pub fn settings(&self) -> Result<AppSettings> {
        if let Some(error) = self.settings_error.lock().unwrap().as_ref() {
            anyhow::bail!(crate::model::UiFailure::new(
                "errors.settingsCorrupt",
                error.clone()
            ))
        }
        Ok(self.settings.lock().unwrap().clone())
    }
    pub fn save(&self, root: &Path, settings: AppSettings) -> Result<()> {
        // This existing command edits close behavior. Model preferences have a
        // separate field-scoped command so late writes cannot erase each other.
        self.edit(root, |next| next.close_behavior = settings.close_behavior)?;
        Ok(())
    }
    pub fn save_model_appearance(
        &self,
        root: &Path,
        profile_id: String,
        appearance: ModelAppearance,
    ) -> Result<()> {
        self.edit(root, |next| {
            if matches!(appearance, ModelAppearance::Default) {
                next.model_appearances.remove(&profile_id);
            } else {
                next.model_appearances.insert(profile_id, appearance);
            }
        })?;
        Ok(())
    }
    pub fn language(&self) -> UiLanguageState {
        let settings = self.settings.lock().unwrap();
        crate::i18n::resolve(settings.language, *self.system_language.lock().unwrap())
    }
    pub fn save_language(
        &self,
        root: &Path,
        preference: LanguagePreference,
    ) -> Result<UiLanguageState> {
        let system = if preference == LanguagePreference::System {
            crate::i18n::system_locale()
        } else {
            *self.system_language.lock().unwrap()
        };
        let mut current = self.settings.lock().unwrap();
        let mut next = current.clone();
        next.language = preference;
        storage::validate_app_settings(&next)?;
        storage::save_json(root, "app-settings.json", &next)?;
        *self.system_language.lock().unwrap() = system;
        *current = next;
        *self.settings_error.lock().unwrap() = None;
        Ok(crate::i18n::resolve(preference, system))
    }
    fn edit(&self, root: &Path, edit: impl FnOnce(&mut AppSettings)) -> Result<()> {
        let mut current = self.settings.lock().unwrap();
        let mut next = current.clone();
        edit(&mut next);
        storage::validate_app_settings(&next)?;
        storage::save_json(root, "app-settings.json", &next)?;
        *current = next;
        *self.settings_error.lock().unwrap() = None;
        Ok(())
    }
}

pub struct McpDesktopBridge(pub tauri::AppHandle);
impl crate::management::DesktopBridge for McpDesktopBridge {
    fn resource_changed(&self, resource: &str) {
        let event = if resource == "library" {
            "library-catalog-changed"
        } else {
            "applications-changed"
        };
        let _ = self.0.emit_to("main", event, 0u64);
    }
    fn preferences_changed(&self, settings: &AppSettings) {
        let language = self.0.state::<DesktopState>().language();
        for label in ["main", crate::tray_panel::LABEL] {
            let _ = self.0.emit_to(label, "ui-language-changed", &language);
        }
        let _ = crate::tray_panel::refresh_language(&self.0);
        let _ = self.0.emit_to("main", "app-settings-changed", settings);
    }
    fn request_restart(&self) {
        self.0
            .state::<DesktopState>()
            .restarting
            .store(true, Ordering::SeqCst);
        shutdown(&self.0);
    }
}
pub fn show(handle: &tauri::AppHandle) {
    crate::tray_panel::hide(handle);
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
pub fn request_exit(handle: &tauri::AppHandle) {
    let desktop = handle.state::<DesktopState>();
    if desktop.quitting.load(Ordering::SeqCst) {
        return;
    }
    // Let the live editor decide whether its unsaved draft needs confirmation.
    if desktop.frontend_ready.load(Ordering::SeqCst)
        && handle.emit_to("main", "exit-requested", ()).is_ok()
    {
        return;
    }
    shutdown(handle);
}
pub fn shutdown(handle: &tauri::AppHandle) {
    if handle
        .state::<DesktopState>()
        .quitting
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let s = handle.state::<AppState>();
    let core = s.core.clone();
    let plugins = s.plugins.clone();
    let mcp = s.mcp.clone();
    let scenes = s.scenes.clone();
    let devices = s.devices.clone();
    let h = handle.clone();
    tauri::async_runtime::spawn(async move {
        scenes.shutdown().await;
        devices.stop_automation().await;
        if let Err(e) = mcp.shutdown().await {
            core.log("warning", format!("MCP 停止时恢复显示失败：{e}"));
        }
        plugins.stop_all().await;
        devices.shutdown().await;
        if let Err(e) = core.reset_led_for_shutdown().await {
            core.log("warning", format!("退出时恢复默认 LED 灯效失败：{e}"));
        }
        let _ = core.export_report();
        let _ = core.stop.send(true);
        if tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while core.snapshot().sdk_pid.is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .is_err()
        {
            core.log("warning", "设备 SDK 关闭超过等待期限");
        }
        if h.state::<DesktopState>().restarting.load(Ordering::SeqCst) {
            h.restart();
        } else {
            h.exit(0);
        }
    });
}
pub fn install_tray(app: &tauri::App) -> Result<()> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let menu = crate::tray_panel::fallback_menu(app.handle())?;
    let icon = app
        .default_window_icon()
        .ok_or_else(|| {
            anyhow::anyhow!(alchemy_device_api::UiFailure::new(
                "backend.b050",
                "应用图标不存在"
            ))
        })?
        .clone();
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Desktop Alchemy")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show(app),
            "exit" => request_exit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Down,
                ..
            } => crate::tray_panel::right_down(tray.app_handle()),
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => show(tray.app_handle()),
            TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Up,
                rect,
                position,
                ..
            } => {
                if let Err(error) = crate::tray_panel::toggle(tray.app_handle(), rect, position) {
                    tray.app_handle().state::<AppState>().core.log(
                        "error",
                        format!("托盘面板打开失败，已恢复系统菜单：{error}"),
                    );
                    crate::tray_panel::fallback(tray.app_handle());
                }
            }
            _ => {}
        })
        .build(app)?;
    app.state::<DesktopState>()
        .tray_ready
        .store(true, Ordering::SeqCst);
    if let Err(error) = crate::tray_panel::install_window(app) {
        app.state::<AppState>().core.log(
            "warning",
            format!("托盘面板创建失败，已保留系统菜单：{error}"),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(root: &Path) -> DesktopState {
        DesktopState::new(
            root,
            crate::process_lock::NamedGuard::acquire(&format!(
                "Local\\m18-desktop-test-{}",
                uuid::Uuid::new_v4()
            ))
            .unwrap(),
        )
    }
    #[test]
    fn defaults_fallback_persistence_and_atomic_failure() {
        let root = std::env::temp_dir().join(format!("m18-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        // Existing installations and backups do not contain a language field.
        std::fs::write(
            root.join("app-settings.json"),
            r#"{"close_behavior":"tray"}"#,
        )
        .unwrap();
        let s = state(&root);
        assert_eq!(s.language().preference, LanguagePreference::System);
        s.save_language(&root, LanguagePreference::English).unwrap();
        assert_eq!(state(&root).language().locale, UiLocale::English);
        assert_eq!(s.settings().unwrap().close_behavior, CloseBehavior::Tray);
        assert!(!s.hide_on_close());
        assert_eq!(s.settings().unwrap().language, LanguagePreference::English);
        s.tray_ready.store(true, Ordering::SeqCst);
        assert!(s.hide_on_close());
        s.save_model_appearance(
            &root,
            "device-a".into(),
            ModelAppearance::Text {
                text: "My deck".into(),
            },
        )
        .unwrap();
        s.save(
            &root,
            AppSettings {
                close_behavior: CloseBehavior::Exit,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!s.hide_on_close());
        assert_eq!(
            state(&root).settings().unwrap().close_behavior,
            CloseBehavior::Exit
        );
        assert!(
            matches!(state(&root).settings().unwrap().model_appearances.get("device-a"), Some(ModelAppearance::Text { text }) if text == "My deck")
        );
        assert!(s
            .save_model_appearance(
                &root,
                "device-a".into(),
                ModelAppearance::Text { text: "\n".into() }
            )
            .is_err());
        std::fs::create_dir(root.join("app-settings.json.tmp")).unwrap();
        assert!(s.save_language(&root, LanguagePreference::Chinese).is_err());
        assert_eq!(s.language().locale, UiLocale::English);
        assert!(s.save(&root, AppSettings::default()).is_err());
        assert!(s
            .save_model_appearance(&root, "device-a".into(), ModelAppearance::Default)
            .is_err());
        assert!(
            matches!(s.settings().unwrap().model_appearances.get("device-a"), Some(ModelAppearance::Text { text }) if text == "My deck")
        );
        assert_eq!(s.settings().unwrap().close_behavior, CloseBehavior::Exit);
        std::fs::write(root.join("app-settings.json"), "broken").unwrap();
        let s = state(&root);
        assert!(s.settings().is_err());
        assert!(!s.hide_on_close());
        assert_eq!(
            std::fs::read_to_string(root.join("app-settings.json")).unwrap(),
            "broken"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
