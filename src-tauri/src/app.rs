#[cfg(feature = "device-m18")]
use crate::devices::m18::sdk;
use crate::{
    core::Core,
    desktop::{self, DesktopState},
    mcp::{McpManager, McpStatus},
    model::*,
    plugins::PluginManager,
    render, storage,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{Emitter, Manager, State};
pub struct AppState {
    pub services: Arc<crate::management::Services>,
    pub devices: Arc<crate::devices::DeviceManager>,
    pub scenes: Arc<crate::scenes::SceneManager>,
    pub core: Arc<Core>,
    pub plugins: Arc<PluginManager>,
    pub mcp: Arc<McpManager>,
    pub resource: PathBuf,
    pub restore_notice: Option<BackupRestoreNotice>,
}
type Reply<T> = Result<T, UiError>;

#[tauri::command]
fn ui_language(s: State<DesktopState>) -> UiLanguageState {
    s.language()
}

#[tauri::command]
fn set_ui_language(
    window: tauri::WebviewWindow,
    s: State<AppState>,
    desktop: State<DesktopState>,
    handle: tauri::AppHandle,
    preference: LanguagePreference,
) -> Reply<UiLanguageState> {
    if window.label() != "main" {
        return Err(UiError::localized(
            "errors.mainWindowOnly",
            "Only the main window can change the UI language",
        ));
    }
    let language = desktop
        .save_language(&s.core.root, preference)
        .map_err(err)?;
    for label in ["main", crate::tray_panel::LABEL] {
        let _ = handle.emit_to(label, "ui-language-changed", &language);
    }
    if let Err(error) = crate::tray_panel::refresh_language(&handle) {
        s.core
            .log("warning", format!("Tray language update failed: {error}"));
    }
    Ok(language)
}

#[tauri::command]
async fn module_request(
    s: State<'_, AppState>,
    device_id: String,
    operation: String,
    payload: serde_json::Value,
) -> Reply<serde_json::Value> {
    s.devices
        .module_request(&device_id, operation, payload)
        .await
        .map_err(err)
}
#[tauri::command]
fn set_capability_enabled(
    s: State<AppState>,
    device_id: String,
    feature: String,
    enabled: bool,
) -> Reply<()> {
    s.devices
        .device_profile(&device_id)
        .map_err(err)?
        .set_capability_enabled(&feature, enabled)
        .map_err(err)
}
#[tauri::command]
fn create_page(s: State<AppState>, profile_id: String, name: String) -> Reply<Page> {
    let core = s.devices.profile(&profile_id).map_err(err)?;
    if !core.supports_pages {
        return Err(alchemy_device_api::UiFailure::new("backend.e001", "此设备不支持页面").into());
    }
    Ok(core
        .adapter
        .get()
        .map(|a| a.new_page(&name))
        .unwrap_or_else(|| alchemy_device_api::default_page(&name, &core.definition())))
}
#[tauri::command]
fn scene_status(s: State<AppState>, profile_id: String) -> Reply<SceneStatus> {
    Ok(s.devices.scene_manager(&profile_id).map_err(err)?.status())
}
#[tauri::command]
fn save_scenes(
    s: State<AppState>,
    profile_id: String,
    settings: SceneSettings,
    expected_revision: Option<String>,
) -> Reply<serde_json::Value> {
    s.devices
        .profile(&profile_id)
        .map_err(err)?
        .save_scenes_checked(settings.clone(), expected_revision.as_deref())
        .map(|_| serde_json::json!({"saved":true,"revision":crate::revisions::revision(&settings)}))
        .map_err(err)
}
#[tauri::command]
fn pause_scenes(s: State<AppState>, profile_id: String, paused: bool) -> Reply<()> {
    s.devices
        .profile(&profile_id)
        .map_err(err)?
        .pause_scenes(paused)
        .map_err(err)
}
#[tauri::command]
fn retry_scene(s: State<AppState>, profile_id: String) -> Reply<()> {
    s.devices.scene_manager(&profile_id).map_err(err)?.retry();
    Ok(())
}
#[tauri::command]
async fn running_programs(s: State<'_, AppState>) -> Reply<Vec<RunningProgram>> {
    s.scenes.programs().await.map_err(err)
}
#[tauri::command]
async fn check_scene_condition(
    s: State<'_, AppState>,
    condition: SceneCondition,
) -> Reply<ConditionTruth> {
    s.scenes.check(condition).await.map_err(err)
}

#[tauri::command]
async fn export_backup(s: State<'_, AppState>, path: String) -> Reply<()> {
    let services = s.services.clone();
    tokio::task::spawn_blocking(move || services.export_backup(&path).map(|_| ()))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
fn backup_restore_notice(s: State<AppState>) -> Option<BackupRestoreNotice> {
    s.restore_notice.clone()
}

#[tauri::command]
async fn inspect_backup(s: State<'_, AppState>, path: String) -> Reply<BackupSummary> {
    let root = s.core.root.clone();
    tokio::task::spawn_blocking(move || crate::backup::inspect(&root, Path::new(&path)))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn import_backup(
    s: State<'_, AppState>,
    handle: tauri::AppHandle,
    path: String,
    fingerprint: String,
) -> Reply<()> {
    let root = s.core.root.clone();
    tokio::task::spawn_blocking(move || {
        crate::backup::stage(&root, Path::new(&path), &fingerprint)
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    handle
        .state::<DesktopState>()
        .restarting
        .store(true, std::sync::atomic::Ordering::SeqCst);
    desktop::shutdown(&handle);
    Ok(())
}

#[tauri::command]
async fn applications_snapshot(s: State<'_, AppState>) -> Reply<serde_json::Value> {
    let core = s.core.clone();
    tokio::task::spawn_blocking(move || {
        let data = core.applications.snapshot()?;
        let mut value = serde_json::to_value(&data)?;
        value["revision"] = serde_json::json!(crate::revisions::revision(&data));
        Ok::<_, anyhow::Error>(value)
    })
    .await
    .map_err(err)?
    .map_err(err)
}
#[tauri::command]
async fn applications_edit(
    s: State<'_, AppState>,
    edit: ApplicationEdit,
    expected_revision: Option<String>,
) -> Reply<serde_json::Value> {
    let core = s.core.clone();
    tokio::task::spawn_blocking(move || {
        let data = core
            .applications
            .edit_checked(edit, expected_revision.as_deref())?;
        let mut value = serde_json::to_value(&data)?;
        value["revision"] = serde_json::json!(crate::revisions::revision(&data));
        Ok::<_, anyhow::Error>(value)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
async fn registered_applications() -> Reply<Vec<RegisteredApplication>> {
    tokio::task::spawn_blocking(crate::application_catalog::ApplicationCatalog::list)
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn registered_application_icon(
    s: State<'_, AppState>,
    target: LaunchTarget,
) -> Reply<String> {
    let root = s.core.root.clone();
    tokio::task::spawn_blocking(move || crate::launch_service::icon(&root, &target))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn save_input_bindings(
    s: State<'_, AppState>,
    profile_id: String,
    bindings: Vec<alchemy_device_api::ControlBinding>,
    expected_revision: Option<String>,
) -> Reply<serde_json::Value> {
    s.services
        .edit_bindings(crate::management::BindingsEdit {
            profile_id: profile_id.clone(),
            expected_revision: expected_revision.unwrap_or_else(|| {
                s.devices
                    .profile(&profile_id)
                    .map(|c| crate::revisions::revision(&c.snapshot().config.bindings))
                    .unwrap_or_default()
            }),
            edit: crate::management::BindingEdit::Replace { bindings },
        })
        .map_err(err)
}

#[tauri::command]
async fn choose_program(window: tauri::WebviewWindow) -> Reply<Option<String>> {
    #[cfg(windows)]
    let owner = window.hwnd().map_err(err)?.0 as usize;
    #[cfg(not(windows))]
    let owner = {
        let _ = window;
        0
    };
    tokio::task::spawn_blocking(move || crate::launcher::choose_program(owner))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn resolve_shortcut(path: String) -> Reply<ShortcutTarget> {
    tokio::task::spawn_blocking(move || crate::launcher::resolve_shortcut(&path))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
fn app_settings(s: State<DesktopState>) -> Reply<AppSettings> {
    s.settings().map_err(err)
}
#[tauri::command]
fn set_app_settings(
    s: State<AppState>,
    desktop: State<DesktopState>,
    settings: AppSettings,
) -> Reply<()> {
    desktop.save(&s.core.root, settings).map_err(err)
}
#[tauri::command]
fn set_model_appearance(
    s: State<AppState>,
    desktop: State<DesktopState>,
    profile_id: String,
    appearance: ModelAppearance,
) -> Reply<()> {
    s.devices.profile(&profile_id).map_err(err)?;
    if let ModelAppearance::Image {
        image: ImageRef::Asset { asset_id },
    } = &appearance
    {
        storage::asset(&s.core.root, asset_id).map_err(err)?;
    }
    desktop
        .save_model_appearance(&s.core.root, profile_id, appearance)
        .map_err(err)
}
#[tauri::command]
fn frontend_ready(s: State<DesktopState>, handle: tauri::AppHandle) {
    s.frontend_ready
        .store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(action) = handle
        .state::<crate::tray_panel::TrayPanelHost>()
        .pending_navigation
        .lock()
        .unwrap()
        .take()
    {
        let _ = handle.emit_to("main", "tray-navigate", action);
    }
}
#[tauri::command]
fn tray_panel_ready(window: tauri::WebviewWindow, handle: tauri::AppHandle) -> Reply<TrayState> {
    if window.label() != crate::tray_panel::LABEL {
        return Err(
            alchemy_device_api::UiFailure::new("backend.e002", "仅托盘面板可调用此接口").into(),
        );
    }
    crate::tray_panel::ready(&handle).map_err(err)
}
#[tauri::command]
fn tray_action(
    window: tauri::WebviewWindow,
    handle: tauri::AppHandle,
    action: TrayAction,
) -> Reply<()> {
    if window.label() != crate::tray_panel::LABEL {
        return Err(
            alchemy_device_api::UiFailure::new("backend.e002", "仅托盘面板可调用此接口").into(),
        );
    }
    crate::tray_panel::action(&handle, action).map_err(err)
}
#[tauri::command]
fn dismiss_tray(window: tauri::WebviewWindow, handle: tauri::AppHandle) -> Reply<()> {
    if window.label() != crate::tray_panel::LABEL {
        return Err(
            alchemy_device_api::UiFailure::new("backend.e002", "仅托盘面板可调用此接口").into(),
        );
    }
    crate::tray_panel::hide(&handle);
    Ok(())
}
#[tauri::command]
fn show_window(handle: tauri::AppHandle) {
    desktop::show(&handle);
}
#[tauri::command]
fn confirm_exit(handle: tauri::AppHandle) {
    desktop::shutdown(&handle);
}
#[tauri::command]
fn library_snapshot(s: State<AppState>) -> Reply<LibraryCatalog> {
    s.core.library_catalog().map_err(err)
}
#[tauri::command]
async fn library_edit(
    s: State<'_, AppState>,
    edit: LibraryEdit,
    expected_revision: Option<String>,
) -> Reply<LibraryCatalog> {
    let core = s.core.clone();
    tokio::task::spawn_blocking(move || {
        core.library
            .edit_checked(edit, expected_revision.as_deref())?;
        core.library_catalog()
    })
    .await
    .map_err(err)?
    .map_err(err)
}
#[tauri::command]
async fn library_import(
    s: State<'_, AppState>,
    paths: Vec<String>,
    folder_id: Option<String>,
) -> Reply<Vec<LibraryImportResult>> {
    let core = s.core.clone();
    tokio::task::spawn_blocking(move || core.library.import(paths, folder_id))
        .await
        .map_err(err)?
        .map_err(err)
}
#[tauri::command]
async fn library_thumbnail(s: State<'_, AppState>, asset_id: String) -> Reply<String> {
    let root = s.core.root.clone();
    tokio::task::spawn_blocking(move || crate::library::thumbnail(&root, &asset_id))
        .await
        .map_err(err)?
        .map_err(err)
}
#[tauri::command]
async fn image_thumbnail(s: State<'_, AppState>, image: ImageRef) -> Reply<String> {
    crate::management::image_preview(s.core.clone(), image)
        .await
        .map_err(err)
}

#[tauri::command]
async fn test_launch_target(target: LaunchTarget) -> Reply<()> {
    tokio::task::spawn_blocking(move || crate::launch_service::execute(&target))
        .await
        .map_err(err)?
        .map_err(err)
}

/// Support/packaging check: validate the installed SDK, identify the device and
/// obtain a firmware reply without opening a webview or requiring a dev runtime.
#[cfg(feature = "device-m18")]
pub fn check_installation() -> anyhow::Result<()> {
    let executable = std::env::current_exe()?;
    let resources = executable.parent().ok_or_else(|| {
        anyhow::anyhow!(alchemy_device_api::UiFailure::new(
            "backend.b002",
            "无法获取安装目录"
        ))
    })?;
    let root = std::env::temp_dir().join(format!("m18-install-check-{}", uuid::Uuid::new_v4()));
    let c = Core::new(root)?;
    let plugins = PluginManager::new(c.clone());
    plugins.register_performance(resources)?;
    crate::plugins::verify_sensor_resources(resources)?;
    crate::plugin_package::inspect(&resources.join("examples/counter.m18plugin"))?;
    let paths = sdk::SdkPaths::resolve(Some(resources))?;
    paths.verified_transport(&c.root)?;
    let devices = crate::devices::DeviceManager::new(c.clone())?;
    let resources = resources.to_path_buf();
    tokio::runtime::Runtime::new()?.block_on(async move {
        devices.start(Some(resources),true);
        let result=tokio::time::timeout(std::time::Duration::from_secs(20),async{
            loop {
                let snapshot=devices.snapshot()?;
                if snapshot.discovered.iter().filter(|device|device.adapter_id==crate::devices::m18::ID).count()>1 {anyhow::bail!(alchemy_device_api::UiFailure::new("backend.b003", "安装自检发现多台 M18，请在客户端明确选择测试目标"))}
                if let Some(device)=snapshot.devices.iter().find(|device|device.adapter_id==crate::devices::m18::ID&&device.state.connection=="ready"&&device.state.receipts.iter().any(|receipt|receipt.command=="firmware"&&receipt.status=="acknowledged")) {return Ok::<_,anyhow::Error>(device.state.clone())}
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }).await;
        devices.shutdown().await;
        let _=c.stop.send(true);
        let stopped=tokio::time::timeout(std::time::Duration::from_secs(5),async{while c.snapshot().sdk_pid.is_some(){tokio::time::sleep(std::time::Duration::from_millis(25)).await;}}).await.is_ok();
        let report=c.export_report()?;
        let s=result.map_err(|_|anyhow::anyhow!("安装自检未能完成：{}",c.snapshot().error.unwrap_or_else(||"未收到设备固件回执".into())))??;
        if !stopped {anyhow::bail!(alchemy_device_api::UiFailure::new("backend.b004", "C SDK 工作线程未在关闭期限内退出"))}
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"ok":true,"devices":s.devices,"sdkPid":s.sdk_pid,"sdkStopped":stopped,"report":report,"note":"已验证安装资源、DeviceManager 路由和固件回执；不等于显示与灯光效果验收"}))?);
        Ok(())
    })
}

#[cfg(not(feature = "device-m18"))]
pub fn check_installation() -> anyhow::Result<()> {
    Ok(())
}

fn err(e: impl Into<anyhow::Error>) -> UiError {
    UiError::from_error(e.into())
}
#[tauri::command]
async fn mcp_status(s: State<'_, AppState>) -> Reply<McpStatus> {
    Ok(s.mcp.status().await)
}
#[tauri::command]
async fn configure_mcp(s: State<'_, AppState>, enabled: bool, port: u16) -> Reply<McpStatus> {
    s.mcp.configure(enabled, port).await.map_err(err)
}
#[tauri::command]
async fn mcp_token(s: State<'_, AppState>) -> Reply<String> {
    s.mcp.token().await.map_err(err)
}
#[tauri::command]
async fn reset_mcp_token(s: State<'_, AppState>) -> Reply<McpStatus> {
    s.mcp.reset_token().await.map_err(err)
}
#[tauri::command]
fn snapshot(s: State<AppState>) -> Reply<WorkspaceSnapshot> {
    s.devices.snapshot().map_err(err)
}
#[tauri::command]
fn device_snapshot(s: State<AppState>, profile_id: String) -> Reply<Snapshot> {
    Ok(s.devices.profile(&profile_id).map_err(err)?.snapshot())
}
#[tauri::command]
fn bind_device(s: State<AppState>, profile_id: String, device_id: String) -> Reply<()> {
    s.devices.bind(&profile_id, &device_id).map_err(err)
}
#[tauri::command]
async fn save_config(
    s: State<'_, AppState>,
    config: AppConfig,
    profile_id: String,
    expected_revision: Option<String>,
) -> Reply<serde_json::Value> {
    s.services
        .save_pages(&profile_id, config.pages, expected_revision.as_deref())
        .await
        .map_err(err)
}

#[tauri::command]
async fn activate_page(s: State<'_, AppState>, page_id: String, device_id: String) -> Reply<()> {
    let core = s.devices.device(&device_id).map_err(err)?;
    core.activate(&page_id).await.map_err(err)
}
#[tauri::command]
async fn show_brightness_test(s: State<'_, AppState>, device_id: String) -> Reply<()> {
    let core = s.devices.device(&device_id).map_err(err)?;
    const ID: &str = "builtin-brightness-test-v1";
    if !core.snapshot().config.pages.iter().any(|p| p.id == ID) {
        let root = core.root.clone();
        let page = tokio::task::spawn_blocking(move || render::brightness_page(&root))
            .await
            .map_err(err)?
            .map_err(err)?;
        let mut config = core.snapshot().config;
        config.pages.push(page);
        core.update_config(config).map_err(err)?;
    }
    core.activate(ID).await.map_err(err)
}
#[tauri::command]
async fn hardware(
    s: State<'_, AppState>,
    command: HardwareCommand,
    allow_persistent: Option<bool>,
    device_id: String,
) -> Reply<String> {
    let core = s.devices.device(&device_id).map_err(err)?;
    if matches!(
        command,
        HardwareCommand::Background {
            persistent: true,
            ..
        } | HardwareCommand::BackgroundData {
            persistent: true,
            ..
        }
    ) && allow_persistent != Some(true)
    {
        return Err(
            alchemy_device_api::UiFailure::new("backend.e003", "写入开机背景需要明确确认").into(),
        );
    };
    let epoch = if command.visual() {
        if core.snapshot().config.scene_settings.enabled {
            core.pause_scenes(true).map_err(err)?;
        }
        Some(core.inner.lock().unwrap().epoch)
    } else {
        None
    };
    let id = core.enqueue(command.clone(), epoch).map_err(err)?;
    {
        let mut i = core.inner.lock().unwrap();
        match command {
            HardwareCommand::ScreenBrightness { value } => {
                i.snapshot.config.device_settings.screen_brightness = value
            }
            _ => {
                i.snapshot
                    .config
                    .device_settings
                    .resolve_led(&command)
                    .map_err(err)?;
            }
        }
        core.persist(&i.snapshot.config).map_err(err)?;
    }
    core.emit();
    Ok(id)
}
#[tauri::command]
fn set_led_settings(
    s: State<AppState>,
    settings: DeviceSettings,
    device_id: String,
    expected_revision: Option<String>,
) -> Reply<()> {
    let core = s.devices.device(&device_id).map_err(err)?;
    if let Some(expected) = expected_revision {
        core.configure_settings_checked(settings, &expected)
            .and_then(|error| error.map_or(Ok(()), |e| Err(anyhow::anyhow!(e))))
            .map_err(err)
    } else {
        core.configure_led_settings(settings).map_err(err)
    }
}
#[tauri::command]
fn retry_connection(s: State<AppState>, device_id: String) -> Reply<()> {
    s.devices.retry(&device_id).map_err(err)
}
#[tauri::command]
fn set_theme(s: State<AppState>, theme: String) -> Reply<()> {
    s.core.set_theme_checked(theme, None).map_err(err)
}

#[tauri::command]
fn import_asset(s: State<AppState>, path: String) -> Reply<String> {
    storage::import(&s.core.root, Path::new(&path)).map_err(err)
}
#[tauri::command]
async fn preview_key(
    s: State<'_, AppState>,
    visual: KeyVisual,
    profile_id: String,
) -> Reply<String> {
    crate::management::key_preview(s.devices.profile(&profile_id).map_err(err)?, visual)
        .await
        .map_err(err)
}

#[tauri::command]
fn preview_asset(s: State<AppState>, asset_id: String) -> Reply<String> {
    storage::data_url(&storage::asset(&s.core.root, &asset_id).map_err(err)?).map_err(err)
}
#[tauri::command]
fn verify_capability(
    s: State<AppState>,
    feature: String,
    status: Verification,
    note: String,
    device_id: String,
) -> Reply<()> {
    let core = s.devices.device(&device_id).map_err(err)?;
    core.mark(&feature, status, &note).map_err(err)
}
#[tauri::command]
fn export_report(s: State<AppState>, device_id: String) -> Reply<String> {
    let core = s.devices.device(&device_id).map_err(err)?;
    core.export_report()
        .map(|p| p.to_string_lossy().into())
        .map_err(err)
}
#[tauri::command]
async fn inspect_plugin_package(
    s: State<'_, AppState>,
    path: String,
) -> Reply<PluginPackageSummary> {
    let manager = s.plugins.clone();
    tokio::task::spawn_blocking(move || manager.inspect_package(Path::new(&path)))
        .await
        .map_err(err)?
        .map_err(err)
}
#[tauri::command]
async fn install_plugin_package(
    s: State<'_, AppState>,
    path: String,
    fingerprint: String,
) -> Reply<String> {
    s.plugins
        .install_package(PathBuf::from(path), fingerprint)
        .await
        .map_err(err)
}
#[tauri::command]
fn widget_samples(s: State<AppState>) -> Vec<WidgetSample> {
    s.core.widget_samples()
}
#[tauri::command]
async fn discover_widget_sources(
    s: State<'_, AppState>,
    id: String,
    lease: String,
) -> Reply<WidgetCatalog> {
    s.plugins.discover_widgets(&id, &lease).await.map_err(err)
}
#[tauri::command]
async fn release_widget_discovery(s: State<'_, AppState>, id: String, lease: String) -> Reply<()> {
    s.plugins.release_discovery(&id, &lease).await;
    Ok(())
}
#[tauri::command]
async fn enable_hardware_sensors(s: State<'_, AppState>) -> Reply<()> {
    s.plugins.enable_hardware_sensors().await.map_err(err)
}
#[tauri::command]
async fn set_plugin_enabled(s: State<'_, AppState>, id: String, enabled: bool) -> Reply<()> {
    s.plugins.set_enabled(&id, enabled).await.map_err(err)
}
#[tauri::command]
async fn remove_plugin(s: State<'_, AppState>, id: String) -> Reply<()> {
    s.plugins.remove(&id).await.map_err(err)
}
#[tauri::command]
async fn inspect_example(s: State<'_, AppState>) -> Reply<PluginPackageSummary> {
    let path = example_package(&s);
    let manager = s.plugins.clone();
    tokio::task::spawn_blocking(move || manager.inspect_package(&path))
        .await
        .map_err(err)?
        .map_err(err)
}
#[tauri::command]
async fn install_example(s: State<'_, AppState>, fingerprint: String) -> Reply<String> {
    s.plugins
        .install_package(example_package(&s), fingerprint)
        .await
        .map_err(err)
}
fn example_package(s: &AppState) -> PathBuf {
    if cfg!(debug_assertions) {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/counter.m18plugin")
    } else {
        s.resource.join("examples/counter.m18plugin")
    }
}
pub fn run() {
    // An isolated diagnostics UI owns no SDK and must not activate the installed app.
    let ui_test_root = if cfg!(all(debug_assertions, feature = "diagnostics")) {
        std::env::var_os("DESKTOP_ALCHEMY_UI_TEST_ROOT").map(PathBuf::from)
    } else {
        None
    };
    let mut builder = tauri::Builder::default();
    if ui_test_root.is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|handle, _, _| {
            desktop::show(handle)
        }));
    }
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let instance_name = if let Some(root) = &ui_test_root {
                use sha2::Digest;
                format!("Local\\DesktopAlchemy.UI-Test.{:x}", sha2::Sha256::digest(root.to_string_lossy().as_bytes()))
            } else { "Local\\DesktopAlchemy.Application".into() };
            let instance = crate::process_lock::NamedGuard::acquire(&instance_name)?;
            let root = ui_test_root.clone().unwrap_or(app.path().app_data_dir()?.join("platform-v1"));
            if !root.is_absolute() { return Err(alchemy_device_api::UiFailure::new("backend.e006", "UI 测试目录须为绝对路径").into()); }
            let resource = app.path().resource_dir()?;
            std::fs::create_dir_all(&root)?;
            crate::plugin_package::recover(&root)?;
            let restore_note = crate::backup::apply_pending(&root)?;
            storage::initialize_workspace(&root)?;
            let core = Core::new(root.clone())?;
            let devices = crate::devices::DeviceManager::new(core.clone())?;
            let scenes = crate::scenes::SceneManager::new(core.clone());
            let mut scene_rx = scenes.changed.subscribe();
            let scene_handle = app.handle().clone();
            let scene_manager = scenes.clone();
            tauri::async_runtime::spawn(async move {
                loop { match scene_rx.recv().await {
                    Ok(status) => { let _ = scene_handle.emit("scene-status", status); },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => { let _ = scene_handle.emit("scene-status", scene_manager.status()); },
                    Err(_) => break,
                } }
            });
            // Per-device scene managers are started by DeviceManager. Global manager serves condition inspection only.
            if let Some(note) = &restore_note { core.log(if note.restored { "info" } else { "error" }, &note.message); }
            app.manage(DesktopState::new(&root, instance));
            app.manage(crate::tray_panel::TrayPanelHost::default());
            let plugins = PluginManager::new(core.clone());
            let plugin_resources = if cfg!(debug_assertions) {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
            } else { resource.clone() };
            if let Err(e) = plugins.register_performance(&plugin_resources) {
                core.log("error", format!("内置性能插件加载失败：{e}"));
            }
            let manager = plugins.clone();
            tauri::async_runtime::spawn(async move { manager.supervise().await; });
            let mut icon_rx = core.icons.changed.subscribe();
            let icon_core = core.clone();
            let icon_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) = icon_rx.recv().await {
                    let _ = icon_handle.emit_to("main", "library-catalog-changed", icon_core.icons.revision());
                    if let Err(error) = icon_core.refresh_icons().await { icon_core.log("warning", format!("插件图标页面刷新失败：{error}")); }
                }
            });
            let mut widget_rx = core.widget_changed.subscribe();
            let widget_core = core.clone();
            let widget_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop { match widget_rx.recv().await {
                    Ok(samples) => { let _ = widget_handle.emit("plugin-widgets", samples); },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let _ = widget_handle.emit("plugin-widgets", widget_core.widget_samples());
                    },
                    Err(_) => break,
                } }
            });
            let mut rx = core.changed.subscribe();
            let tray_devices = devices.clone();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop { match rx.recv().await {
                    Ok(mut s) => {
                        if let Ok(snapshot) = tray_devices.snapshot() {
                            s.connection = if snapshot.devices.iter().any(|d| d.state.connection == "ready") { "ready" } else { "disconnected" }.into();
                        }
                        crate::tray_panel::update(&handle, &s);
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                } }
            });
            let mut rx = core.action_errors.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop { match rx.recv().await {
                    Ok(error) => { let _ = handle.emit("action-error", error); },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                } }
            });
            devices.start(if cfg!(debug_assertions) { None } else { Some(resource.clone()) }, ui_test_root.is_none());
            if ui_test_root.is_some() { core.log("info", "原生 UI 隔离测试：设备连接已禁用"); }
            let device_manager = devices.clone();
            let device_handle = app.handle().clone();
            let mut device_changes = devices.changed.subscribe();
            tauri::async_runtime::spawn(async move {
                while let Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) = device_changes.recv().await {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    while device_changes.try_recv().is_ok() {}
                    if let Ok(snapshot) = device_manager.snapshot() { let _ = device_handle.emit_to("main", "workspace-state", snapshot); }
                }
            });
            let c = core.clone();
            let health_devices = devices.clone();
            tauri::async_runtime::spawn(async move {
                let started = now(); let mut stop = c.stop.subscribe();
                let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
                loop { tokio::select! {
                    _ = stop.changed() => break,
                    _ = ticker.tick() => {
                        use std::io::Write;
                        let s = c.snapshot();
                        let sample = serde_json::json!({"startedAt":started,"sampleAt":now(),"connection":s.connection,"sdkPid":s.sdk_pid,"devices":health_devices.snapshot().ok().map(|workspace| workspace.devices.into_iter().map(|d| serde_json::json!({"deviceId":d.device_id,"profileId":d.profile_id,"connection":d.state.connection,"activePage":d.state.active_page})).collect::<Vec<_>>())});
                        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(c.root.join("session-health.jsonl")) { let _ = writeln!(f, "{sample}"); }
                    }
                } }
            });
            let services = Arc::new(crate::management::Services { core: core.clone(), plugins: plugins.clone(), preferences: app.state::<DesktopState>().preferences.clone(), desktop: Some(Arc::new(crate::desktop::McpDesktopBridge(app.handle().clone()))), restore_notice: restore_note.clone() });
            let mcp = McpManager::with_services(services.clone()); let m = mcp.clone();
            tauri::async_runtime::spawn(async move { m.start_saved().await; });
            app.manage(AppState { services, devices, core: core.clone(), scenes, plugins, mcp, resource, restore_notice: restore_note });
            if let Err(e) = desktop::install_tray(app) { core.log("error", format!("托盘创建失败，关闭窗口将退出应用：{e}")); }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == crate::tray_panel::LABEL {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        crate::tray_panel::hide(window.app_handle());
                    }
                    tauri::WindowEvent::Focused(false) => crate::tray_panel::blur(window),
                    tauri::WindowEvent::Destroyed => crate::tray_panel::fallback(window.app_handle()),
                    _ => {}
                }
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let handle = window.app_handle();
                let s = handle.state::<DesktopState>();
                api.prevent_close();
                if s.quitting.load(std::sync::atomic::Ordering::SeqCst) { return; }
                if s.hide_on_close() {
                    if let Err(e) = window.hide() { handle.state::<AppState>().core.log("error", format!("隐藏窗口失败：{e}")); }
                } else { desktop::request_exit(handle); }
            }
        })
        .invoke_handler({
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![ui_language,set_ui_language,module_request,create_page,set_capability_enabled,discover_widget_sources,release_widget_discovery,enable_hardware_sensors,tray_panel_ready,tray_action,dismiss_tray,scene_status,save_scenes,pause_scenes,retry_scene,running_programs,check_scene_condition,widget_samples,set_plugin_enabled,snapshot,device_snapshot,bind_device,save_config,activate_page,show_brightness_test,hardware,set_led_settings,retry_connection,set_theme,import_asset,preview_key,preview_asset,verify_capability,export_report,inspect_plugin_package,install_plugin_package,remove_plugin,inspect_example,install_example,mcp_status,configure_mcp,mcp_token,reset_mcp_token,app_settings,set_app_settings,set_model_appearance,frontend_ready,show_window,confirm_exit,library_snapshot,library_edit,library_import,library_thumbnail,image_thumbnail,test_launch_target,save_input_bindings,registered_application_icon,choose_program,resolve_shortcut,registered_applications,applications_snapshot,applications_edit,export_backup,inspect_backup,import_backup,backup_restore_notice];
            move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
                if !crate::tray_panel::allows_command(invoke.message.webview().label(), invoke.message.command()) {
                    invoke.resolver.reject(UiError::localized("errors.trayCommandDenied", "托盘面板不允许调用此接口"));
                    true
                } else {
                    handler(invoke)
                }
            }
        })
        .build(tauri::generate_context!()).expect("Desktop Alchemy 启动失败");
    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if *handle.state::<AppState>().core.stop.borrow() {
                return;
            }
            api.prevent_exit();
            desktop::request_exit(handle);
        }
    });
}
