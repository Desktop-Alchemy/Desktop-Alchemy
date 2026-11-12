//! Application operations shared by desktop IPC and the MCP management tools.
//! Transport, credentials and raw device handles do not belong to this layer.
use crate::{core::Core, model::*, revisions, storage};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

pub fn versioned(value: impl Serialize) -> Value {
    json!({"revision": revisions::revision(&value), "data": value})
}

pub trait DesktopBridge: Send + Sync {
    fn preferences_changed(&self, settings: &AppSettings);
    fn resource_changed(&self, resource: &str);
    fn request_restart(&self);
}
#[derive(Clone)]
pub struct Services {
    pub core: Arc<Core>,
    pub plugins: Arc<crate::plugins::PluginManager>,
    pub preferences: Arc<crate::desktop::Preferences>,
    pub desktop: Option<Arc<dyn DesktopBridge>>,
    pub restore_notice: Option<BackupRestoreNotice>,
}
impl Services {
    pub fn resource_changed(&self, resource: &str) {
        self.core.emit();
        if let Some(desktop) = &self.desktop {
            desktop.resource_changed(resource);
        }
    }
    pub fn headless(core: Arc<Core>) -> Arc<Self> {
        Arc::new(Self {
            plugins: crate::plugins::PluginManager::new(core.clone()),
            preferences: crate::desktop::Preferences::load(&core.root),
            core,
            desktop: None,
            restore_notice: None,
        })
    }
    pub fn profile(&self, id: &str) -> Result<Arc<Core>> {
        if let Some(manager) = self.core.device_manager() {
            return manager.profile(id);
        }
        if self.core.profile_id.as_deref() == Some(id) {
            return Ok(self.core.clone());
        }
        Err(alchemy_device_api::ApiError::new(
            alchemy_device_api::ErrorCode::NotFound,
            "Profile not found",
        )
        .into())
    }
    pub fn workspace(&self) -> Result<Value> {
        // Deliberately project configuration, not internal snapshots/paths/locks.
        let profiles = storage::load_workspace(&self.core.root)?
            .profiles
            .into_iter()
            .map(|p| {
                json!({"profile_id":p.id,"name":p.name,"adapter_id":p.adapter_id,
                "device_id":p.device_id,"config_error":p.config_error})
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"api_version":1,"profiles":profiles,"management":true,
            "temporary_control_restored_on_disable":true,"persistent_changes_survive_disable":true,
            "unsupported":["raw_sdk","raw_hid","boot_image_write","hardware_image_readback"]}),
        )
    }
    pub fn profile_snapshot(&self, id: &str) -> Result<Value> {
        let core = self.profile(id)?;
        let s = core.snapshot();
        Ok(
            json!({"profile_id":id,"definition":core.definition(),"saved":s.config,
            "revisions":s.revisions,"runtime":{"device_id":s.selected_device,
            "connection":s.connection,"active_page":s.active_page},
            "scene_status":core.device_manager().and_then(|m| m.scene_manager(id).ok()).map(|m|m.status())}),
        )
    }
    pub async fn save_pages(
        &self,
        id: &str,
        pages: Vec<Page>,
        expected: Option<&str>,
    ) -> Result<Value> {
        let core = self.profile(id)?;
        let revision = revisions::revision(&pages);
        let refresh = core.update_pages_checked(pages, expected)?;
        let display_error = if let Some(page) = refresh {
            core.activate_from(&page, crate::core::ActivationSource::Refresh)
                .await
                .err()
                .map(|e| e.to_string())
        } else {
            None
        };
        Ok(
            json!({"saved":true,"revision":revision,"display_error":display_error,"verification":"not_observed"}),
        )
    }
    pub async fn edit_pages(&self, request: PagesEdit) -> Result<Value> {
        let core = self.profile(&request.profile_id)?;
        let snapshot = core.snapshot();
        revisions::check(Some(&request.expected_revision), &snapshot.config.pages)?;
        let mut pages = snapshot.config.pages;
        match request.edit {
            PageEdit::Create { name } => {
                let page = core
                    .adapter
                    .get()
                    .context("Device module unavailable")?
                    .new_page(&name);
                pages.push(page);
            }
            PageEdit::Duplicate { page_id, name } => {
                let mut page = pages
                    .iter()
                    .find(|p| p.id == page_id)
                    .context("Page not found")?
                    .clone();
                page.id = uuid::Uuid::new_v4().to_string();
                page.name = name;
                pages.push(page);
            }
            PageEdit::Rename { page_id, name } => page_mut(&mut pages, &page_id)?.name = name,
            PageEdit::Delete { page_id } => {
                page_mut(&mut pages, &page_id)?;
                pages.retain(|p| p.id != page_id);
                for page in &mut pages {
                    for binding in &mut page.bindings {
                        if matches!(&binding.action, KeyAction::Page{page_id:id} if id==&page_id) {
                            binding.action = KeyAction::None;
                        }
                    }
                }
            }
            PageEdit::Reorder { page_ids } => {
                anyhow::ensure!(
                    page_ids.len() == pages.len(),
                    "Provide every page exactly once"
                );
                let mut sorted = Vec::new();
                for id in page_ids {
                    let index = pages
                        .iter()
                        .position(|p| p.id == id)
                        .context("Duplicate or unknown page")?;
                    sorted.push(pages.remove(index));
                }
                pages = sorted;
            }
            PageEdit::Update { page } => {
                let id = page.id.clone();
                *page_mut(&mut pages, &id)? = page;
            }
            PageEdit::SetKeys {
                page_id,
                keys,
                bindings,
            } => {
                let page = page_mut(&mut pages, &page_id)?;
                for key in keys {
                    let slot = page
                        .keys
                        .iter_mut()
                        .find(|k| k.key == key.key)
                        .context("Display key not found")?;
                    *slot = key;
                }
                if let Some(bindings) = bindings {
                    page.bindings = bindings;
                }
            }
            PageEdit::ApplyApplication {
                page_id,
                control_id,
                application_id,
            } => {
                let application = self
                    .core
                    .applications
                    .backup_snapshot()?
                    .items
                    .into_iter()
                    .find(|a| a.id == application_id)
                    .context("Application preset not found")?;
                apply_application(
                    &mut page_mut(&mut pages, &page_id)?.bindings,
                    None,
                    &control_id,
                    &application,
                )?;
                let page = page_mut(&mut pages, &page_id)?;
                if let Some(visual) = page
                    .keys
                    .iter_mut()
                    .find(|v| v.key.to_string() == control_id)
                {
                    apply_application_visual(visual, &application);
                }
            }
            PageEdit::Replace { pages: replacement } => pages = replacement,
        }
        self.save_pages(&request.profile_id, pages, Some(&request.expected_revision))
            .await
    }
    pub fn edit_bindings(&self, request: BindingsEdit) -> Result<Value> {
        let core = self.profile(&request.profile_id)?;
        let mut bindings = core.snapshot().config.bindings;
        match request.edit {
            BindingEdit::Replace { bindings: next } => bindings = next,
            BindingEdit::ApplyApplication {
                control_id,
                application_id,
            } => {
                let application = self
                    .core
                    .applications
                    .backup_snapshot()?
                    .items
                    .into_iter()
                    .find(|a| a.id == application_id)
                    .context("Application preset not found")?;
                apply_application(&mut bindings, None, &control_id, &application)?;
            }
        }
        let revision = revisions::revision(&bindings);
        core.save_control_bindings_checked(bindings, Some(&request.expected_revision))?;
        Ok(json!({"saved":true,"revision":revision}))
    }
    pub async fn device_settings(&self, request: DeviceTarget) -> Result<Value> {
        let core = crate::devices::resolve_device(&self.core, &request.device_id, false)?;
        #[cfg(feature = "device-ms-audio-dock")]
        if core.adapter_id == crate::devices::ms_audio_dock::ID {
            let state = self
                .core
                .device_manager()
                .context("Device manager unavailable")?
                .module_request(&request.device_id, "read".into(), Value::Null)
                .await?;
            return Ok(
                json!({"source":"hardware_readback","device_id":request.device_id,"data":state}),
            );
        }
        let s = core.snapshot();
        Ok(
            json!({"source":"saved_configuration","revision":revisions::revision(&s.config.device_settings),
            "data":s.config.device_settings,"hardware_readback_supported":false,
            "runtime":{"connection":s.connection,"firmware":s.devices.first().map(|d|&d.firmware),"gif_status":s.gif_status}}),
        )
    }
    pub async fn update_device_settings(&self, request: DeviceSettingsEdit) -> Result<Value> {
        let core = crate::devices::resolve_device(&self.core, &request.device_id, true)?;
        match request.edit {
            SettingsEdit::M18 {
                expected_revision,
                settings,
            } => {
                anyhow::ensure!(
                    core.adapter_id == "mirabox.m18",
                    "This settings operation requires an M18 adapter"
                );
                anyhow::ensure!(
                    settings.screen_brightness <= 100,
                    "Screen brightness must be 0..100"
                );
                let brightness = settings.screen_brightness;
                let led_error = core.configure_settings_checked(settings, &expected_revision)?;
                // Configuration is already durable. A queue error is a distinct outcome.
                let outcome = core.enqueue(
                    HardwareCommand::ScreenBrightness { value: brightness },
                    None,
                );
                Ok(
                    json!({"saved":true,"revision":core.snapshot().revisions.get("settings"),
                    "receipt_id":outcome.as_ref().ok(),"display_error":outcome.err().map(|e|e.to_string()).or(led_error),"verification":"not_observed"}),
                )
            }
            #[cfg(feature = "device-ms-audio-dock")]
            SettingsEdit::AudioDock { update } => {
                anyhow::ensure!(
                    core.adapter_id == crate::devices::ms_audio_dock::ID,
                    "This operation requires an Audio Dock"
                );
                let state = self
                    .core
                    .device_manager()
                    .context("Device manager unavailable")?
                    .module_request(
                        &request.device_id,
                        "update".into(),
                        serde_json::to_value(update)?,
                    )
                    .await?;
                Ok(
                    json!({"source":"hardware_readback","verification":"readback_confirmed","data":state}),
                )
            }
        }
    }
    pub fn update_preferences(&self, request: PreferencesEdit) -> Result<Value> {
        for (id, appearance) in &request.settings.model_appearances {
            self.profile(id)?;
            if let ModelAppearance::Image {
                image: ImageRef::Asset { asset_id },
            } = appearance
            {
                storage::asset(&self.core.root, asset_id)?;
            }
        }
        let saved = self.preferences.replace_checked(
            &self.core.root,
            request.settings,
            &request.expected_revision,
        )?;
        if let Some(desktop) = &self.desktop {
            desktop.preferences_changed(&saved);
        }
        Ok(versioned(saved))
    }
    pub fn export_backup(&self, path: &str) -> Result<Value> {
        let data = crate::backup::BackupData {
            config: storage::load_workspace(&self.core.root)?,
            library: self.core.library.snapshot()?,
            applications: self.core.applications.backup_snapshot()?,
            settings: self.preferences.settings()?,
        };
        crate::backup::export(&self.core.root, Path::new(path), &data)?;
        Ok(json!({"exported":true,"includes_unsaved_drafts":false}))
    }
}
fn page_mut<'a>(pages: &'a mut [Page], id: &str) -> Result<&'a mut Page> {
    pages
        .iter_mut()
        .find(|p| p.id == id)
        .context("Page not found")
}
pub async fn image_preview(core: Arc<Core>, image: ImageRef) -> Result<String> {
    blocking(move || {
        use base64::Engine;
        match image {
            ImageRef::Asset { asset_id } => crate::library::thumbnail(&core.root, &asset_id),
            ImageRef::PluginIcon { plugin_id, icon_id } => {
                let bytes = core
                    .icons
                    .resolve(&PluginIconRef { plugin_id, icon_id })?
                    .context("Plugin icon unavailable")?;
                let image =
                    crate::render::resize_image(image::load_from_memory(&bytes)?, 160, 160, false);
                let mut png = std::io::Cursor::new(Vec::new());
                image.write_to(&mut png, image::ImageFormat::Png)?;
                Ok(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(png.into_inner())
                ))
            }
        }
    })
    .await
}
pub async fn key_preview(core: Arc<Core>, visual: KeyVisual) -> Result<String> {
    anyhow::ensure!(
        core.layout.display_keys.contains(&visual.key),
        "Control has no display"
    );
    anyhow::ensure!(
        visual.text.chars().count() <= 80,
        "Key text exceeds 80 characters"
    );
    let widget_data = core.widget_data(&visual);
    blocking(move || {
        use base64::Engine;
        let png = if let (Some(binding), Some(data)) = (&visual.widget, widget_data) {
            crate::render::widget_sized(
                binding,
                &data,
                core.layout.key_width,
                core.layout.key_height,
            )?
        } else {
            core.key_png(&visual)?
        };
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
    })
    .await
}
pub fn apply_application(
    bindings: &mut Vec<alchemy_device_api::ControlBinding>,
    trigger: Option<alchemy_device_api::InputTrigger>,
    control_id: &str,
    application: &ApplicationEntry,
) -> Result<()> {
    let trigger = trigger.unwrap_or(alchemy_device_api::InputTrigger::Press);
    if let Some(binding) = bindings.iter_mut().find(|b| {
        b.control_id == control_id
            && revisions::revision(&b.trigger) == revisions::revision(&trigger)
    }) {
        binding.action = KeyAction::Launch {
            target: application.target.clone(),
        };
    } else {
        bindings.push(alchemy_device_api::ControlBinding {
            control_id: control_id.into(),
            enabled: false,
            trigger,
            action: KeyAction::Launch {
                target: application.target.clone(),
            },
        });
    }
    Ok(())
}
pub fn apply_application_visual(visual: &mut KeyVisual, application: &ApplicationEntry) {
    let default_content = visual.has_default_content();
    if visual.text.is_empty() || visual.text == format!("{:02}", visual.key) {
        visual.text = application.name.chars().take(80).collect();
    }
    if visual.plugin_icon.is_none()
        && (visual.asset_id.is_none() || visual.asset_id == visual.default_icon_asset_id)
    {
        visual.asset_id = application.icon_asset_id.clone();
        visual.default_icon_asset_id = application.icon_asset_id.clone();
        if default_content && application.icon_asset_id.is_some() {
            visual.content_mode = Some(KeyContentMode::Pattern);
            visual.show_text = false;
        }
    }
}

#[cfg(test)]
mod content_tests {
    use super::*;

    #[test]
    fn application_icons_default_to_pattern_but_preserve_custom_display_settings() {
        let application = ApplicationEntry {
            id: "app".into(),
            name: "Application".into(),
            target: LaunchTarget::Uri {
                uri: "https://example.com".into(),
            },
            icon_asset_id: Some(format!("{}.png", "a".repeat(64))),
        };
        let mut visual = crate::devices::default_page("default").keys.remove(0);
        apply_application_visual(&mut visual, &application);
        assert_eq!(visual.content_mode, Some(KeyContentMode::Pattern));
        assert!(!visual.show_text);
        assert_eq!(visual.text, application.name);
        assert_eq!(visual.asset_id, application.icon_asset_id);
        for mode in [
            None,
            Some(KeyContentMode::None),
            Some(KeyContentMode::Text),
            Some(KeyContentMode::Pattern),
        ] {
            let mut custom = visual.clone();
            custom.content_mode = mode;
            custom.text = "Custom title".into();
            custom.text_position = TextPosition::Bottom;
            custom.show_text = true;
            apply_application_visual(&mut custom, &application);
            assert_eq!(custom.content_mode, mode);
            assert_eq!(custom.text, "Custom title");
            assert!(custom.show_text);
            assert_eq!(custom.text_position, TextPosition::Bottom);
        }
    }
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileTarget {
    pub profile_id: String,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceTarget {
    pub device_id: String,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Search {
    pub query: Option<String>,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryMutation {
    pub expected_revision: String,
    pub edit: LibraryEdit,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryUpload {
    pub expected_revision: String,
    pub folder_id: Option<String>,
    pub items: Vec<UploadItem>,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadItem {
    pub name: String,
    pub path: Option<String>,
    pub data: Option<String>,
    pub replace_item_id: Option<String>,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplicationMutation {
    pub expected_revision: String,
    pub edit: ApplicationEdit,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shortcut {
    pub path: String,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub image: ImageRef,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyPreview {
    pub profile_id: String,
    pub visual: KeyVisual,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PagesEdit {
    pub profile_id: String,
    pub expected_revision: String,
    pub edit: PageEdit,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PageEdit {
    Create {
        name: String,
    },
    Duplicate {
        page_id: String,
        name: String,
    },
    Rename {
        page_id: String,
        name: String,
    },
    Delete {
        page_id: String,
    },
    Reorder {
        page_ids: Vec<String>,
    },
    Update {
        page: Page,
    },
    SetKeys {
        page_id: String,
        keys: Vec<KeyVisual>,
        bindings: Option<Vec<alchemy_device_api::ControlBinding>>,
    },
    ApplyApplication {
        page_id: String,
        control_id: String,
        application_id: String,
    },
    Replace {
        pages: Vec<Page>,
    },
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BindingsEdit {
    pub profile_id: String,
    pub expected_revision: String,
    pub edit: BindingEdit,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingEdit {
    Replace {
        bindings: Vec<alchemy_device_api::ControlBinding>,
    },
    ApplyApplication {
        control_id: String,
        application_id: String,
    },
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageActivation {
    pub device_id: String,
    pub page_id: String,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenesEdit {
    pub profile_id: String,
    pub expected_revision: String,
    pub settings: SceneSettings,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConditionCheck {
    pub profile_id: String,
    pub condition: SceneCondition,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceSettingsEdit {
    pub device_id: String,
    pub edit: SettingsEdit,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingsEdit {
    M18 {
        expected_revision: String,
        settings: DeviceSettings,
    },
    #[cfg(feature = "device-ms-audio-dock")]
    AudioDock {
        update: crate::devices::ms_audio_dock::types::AudioDockSettingsUpdate,
    },
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceManagement {
    pub edit: DeviceEdit,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceEdit {
    Retry {
        device_id: String,
    },
    Bind {
        profile_id: String,
        device_id: String,
    },
    Capability {
        device_id: String,
        feature: String,
        enabled: bool,
    },
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeEdit {
    pub expected_revision: String,
    pub theme: String,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreferencesEdit {
    pub expected_revision: String,
    pub settings: AppSettings,
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WidgetRequest {
    pub plugin_id: String,
    pub lease: String,
    pub release: bool,
}
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRequest {
    pub request_id: String,
    pub operation: Operation,
}
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Launch { target: LaunchTarget },
    PluginInspect { path: String },
    PluginInstall { path: String, fingerprint: String },
    PluginEnable { id: String, enabled: bool },
    PluginStart { id: String },
    PluginStop { id: String },
    PluginRemove { id: String },
    EnableHardwareSensors,
    BackupExport { path: String },
    BackupInspect { path: String },
    BackupRestore { path: String, fingerprint: String },
}
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationTarget {
    pub operation_id: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgetOperations {
    pub operation_ids: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct OperationState {
    pub operation_id: String,
    pub request_id: String,
    pub fingerprint: String,
    pub status: String,
    pub result: Option<Value>,
}
pub struct Operations {
    root: std::path::PathBuf,
    states: Mutex<BTreeMap<String, OperationState>>,
    load_error: Option<String>,
    slots: Arc<tokio::sync::Semaphore>,
    restart: Mutex<Option<Arc<dyn DesktopBridge>>>,
}
impl Operations {
    pub fn load(root: &Path) -> Arc<Self> {
        let path = root.join("mcp-operations.json");
        let states = if path.exists() {
            std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<BTreeMap<String, OperationState>>(&b).ok())
        } else {
            Some(BTreeMap::new())
        };
        let load_error=states.is_none().then(||"Operation history is damaged or unreadable; the original file was preserved and new jobs are disabled".to_owned());
        let mut states = states.unwrap_or_default();
        for state in states.values_mut() {
            if state.status == "running" {
                state.status = "interrupted".into();
            }
        }
        Arc::new(Self {
            root: root.into(),
            states: Mutex::new(states),
            load_error,
            slots: Arc::new(tokio::sync::Semaphore::new(8)),
            restart: Mutex::new(None),
        })
    }
    /// Called by the HTTP body after delivery, never while holding a tool lease.
    pub fn restart_pending(&self) -> bool {
        self.restart.lock().unwrap().is_some()
    }
    pub fn response_finished(&self) {
        let desktop = self.restart.lock().unwrap().take();
        if let Some(desktop) = desktop {
            desktop.request_restart();
        }
    }
    pub fn get(&self, id: &str) -> Result<Value> {
        if let Some(error) = &self.load_error {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ConfigUnavailable,
                error,
            )
            .into());
        }
        let states = self.states.lock().unwrap();
        let state = states
            .values()
            .find(|s| s.operation_id == id)
            .context("Operation not found")?;
        Ok(serde_json::to_value(state)?)
    }
    pub fn forget(&self, ids: &[String]) -> Result<Value> {
        anyhow::ensure!(
            self.load_error.is_none(),
            "Operation history is unavailable"
        );
        let mut states = self.states.lock().unwrap();
        let mut next = states.clone();
        for id in ids {
            let key = next
                .iter()
                .find(|(_, state)| &state.operation_id == id)
                .map(|(key, _)| key.clone())
                .context("Operation not found")?;
            anyhow::ensure!(
                next[&key].status != "running",
                "An active operation cannot be forgotten"
            );
            next.remove(&key);
        }
        storage::save_json(&self.root, "mcp-operations.json", &next)?;
        *states = next;
        Ok(json!({"forgotten":ids.len()}))
    }
    pub fn submit(
        self: &Arc<Self>,
        services: Arc<Services>,
        request: JobRequest,
        access: tokio::sync::OwnedRwLockReadGuard<bool>,
    ) -> Result<Value> {
        if let Some(error) = &self.load_error {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ConfigUnavailable,
                error,
            )
            .into());
        }
        anyhow::ensure!(
            !request.request_id.is_empty() && request.request_id.len() <= 120,
            "request_id must contain 1..120 bytes"
        );
        let fingerprint = revisions::revision(&request.operation);
        let mut states = self.states.lock().unwrap();
        if let Some(existing) = states.get(&request.request_id) {
            anyhow::ensure!(
                existing.fingerprint == fingerprint,
                "request_id already describes another operation"
            );
            return Ok(serde_json::to_value(existing)?);
        }
        // Keep the ledger bounded without silently allowing old IDs to execute again.
        if states.len() >= 1024 {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ResourceExhausted,
                "Operation ledger is full",
            )
            .into());
        }
        let permit = self.slots.clone().try_acquire_owned().map_err(|_| {
            alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ResourceExhausted,
                "Too many active operations",
            )
        })?;
        let state = OperationState {
            operation_id: uuid::Uuid::new_v4().to_string(),
            request_id: request.request_id.clone(),
            fingerprint,
            status: "running".into(),
            result: None,
        };
        states.insert(request.request_id.clone(), state.clone());
        if let Err(e) = storage::save_json(&self.root, "mcp-operations.json", &*states) {
            states.remove(&request.request_id);
            return Err(e);
        }
        drop(states);
        let operations = self.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let restore = matches!(request.operation, Operation::BackupRestore { .. });
            let result = run_operation(&services, request.operation).await;
            let success = result.is_ok();
            {
                let mut states = operations.states.lock().unwrap();
                let state = states.get_mut(&request.request_id).unwrap();
                state.status = if success { "completed" } else { "failed" }.into();
                state.result = Some(match result {
                    Ok(value) => value,
                    Err(e) => json!({"error":alchemy_device_api::ApiError::from_error(&e)}),
                });
                if let Err(e) =
                    storage::save_json(&operations.root, "mcp-operations.json", &*states)
                {
                    let state = states.get_mut(&request.request_id).unwrap();
                    state.status = "result_persistence_failed".into();
                    state.result =
                        Some(json!({"error":e.to_string(),"operation_may_have_completed":success}));
                }
            }
            drop(access); // Never hold a tool lease while asking MCP itself to stop.
            if restore && success {
                *operations.restart.lock().unwrap() = services.desktop.clone();
            }
        });
        Ok(serde_json::to_value(state)?)
    }
}
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await?
}
async fn run_operation(services: &Arc<Services>, operation: Operation) -> Result<Value> {
    let s = services.clone();
    match operation {
        Operation::Launch { target } => {
            blocking(move || crate::launch_service::execute(&target)).await?;
            Ok(json!({"started":true}))
        }
        Operation::PluginInspect { path } => {
            blocking(move || {
                Ok(serde_json::to_value(
                    s.plugins.inspect_package(Path::new(&path))?,
                )?)
            })
            .await
        }
        Operation::PluginInstall { path, fingerprint } => {
            let id = s.plugins.install_package(path.into(), fingerprint).await?;
            let enabled = !s.core.snapshot().config.disabled_plugins.contains(&id);
            Ok(json!({"plugin_id":id,"enabled":enabled}))
        }
        Operation::PluginEnable { id, enabled } => {
            s.plugins.set_enabled(&id, enabled).await?;
            Ok(json!({"saved":true}))
        }
        Operation::PluginStart { id } => {
            s.plugins.start(&id).await?;
            Ok(json!({"started":true}))
        }
        Operation::PluginStop { id } => {
            s.plugins.stop(&id).await?;
            Ok(json!({"stopped":true}))
        }
        Operation::PluginRemove { id } => {
            s.plugins.remove(&id).await?;
            Ok(json!({"removed":true}))
        }
        Operation::EnableHardwareSensors => {
            s.plugins.enable_hardware_sensors().await?;
            Ok(json!({"enabled":true}))
        }
        Operation::BackupExport { path } => blocking(move || s.export_backup(&path)).await,
        Operation::BackupInspect { path } => {
            blocking(move || {
                Ok(serde_json::to_value(crate::backup::inspect(
                    &s.core.root,
                    Path::new(&path),
                )?)?)
            })
            .await
        }
        Operation::BackupRestore { path, fingerprint } => {
            // Headless tests can exercise staging without an actual application restart.
            blocking(move || crate::backup::stage(&s.core.root, Path::new(&path), &fingerprint))
                .await?;
            Ok(json!({"staged":true,"restart_requested":services.desktop.is_some()}))
        }
    }
}

/// Validate nested request objects without tightening saved-file compatibility.
pub struct Strict<T>(pub T);
impl<T: JsonSchema> JsonSchema for Strict<T> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("Strict{}", T::schema_name()).into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        T::json_schema(generator)
    }
}
impl<'de, T: serde::de::DeserializeOwned + Serialize> Deserialize<'de> for Strict<T> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        alchemy_device_api::strict(value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}
