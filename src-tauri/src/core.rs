use crate::{model::*, render, storage};
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Instant,
};
use tokio::sync::{broadcast, watch, Notify};

#[derive(Clone, Copy)]
pub enum ActivationSource {
    Manual,
    External,
    Refresh,
    Reconnect,
    Scene {
        generation: u64,
        source_epoch: u64,
        valid_until: Instant,
    },
}

#[derive(Clone)]
pub struct Work {
    pub widget_frame: Option<crate::widgets::WidgetFrame>,
    pub image_frame: Option<Arc<Vec<u8>>>,
    pub icon_revision: Option<u64>,
    pub source: Option<String>,
    pub source_epoch: u64,
    /// An already-rendered page, sent contiguously in one scheduler turn.
    pub page_batch: Option<u64>,
    pub receipt_id: Option<String>,
    pub device_id: String,
    pub device_path: String,
    pub connection_epoch: u64,
    pub epoch: Option<u64>,
    pub command: HardwareCommand,
    pub led_command: Option<HardwareCommand>,
}
type ImageFrames = BTreeMap<u8, Arc<Vec<u8>>>;

#[derive(Default)]
struct PageImages {
    frames: ImageFrames,
    revision: Option<u64>,
}
pub struct Inner {
    pub module_state: serde_json::Value,
    pub analog_armed: BTreeMap<String, bool>,
    pub scene_receipts: std::collections::BTreeSet<String>,
    pub scene_repaint: bool,
    pub scene_frame_error: Option<String>,
    pub scene_generation: u64,
    pub scenes_stopping: bool,
    pub widgets: crate::widgets::WidgetState,
    pub snapshot: Snapshot,
    pub paths: BTreeMap<String, String>,
    pub connection_epoch: u64,
    pub queue: VecDeque<Work>,
    pub epoch: u64,
    pub overrides: BTreeMap<String, (String, HardwareCommand)>,
    pub icon_overrides: BTreeMap<String, (String, KeyVisual)>,
    pub led: DeviceSettings,
    pub led_frame: Option<Vec<[u8; 3]>>,
    pub led_effect_started_at: Instant,
    pub led_shutdown: bool,
    pub source_epochs: BTreeMap<String, u64>,
}
pub struct WorkspaceServices {
    pub root: PathBuf,
    pub library: Arc<crate::library::LibraryStore>,
    pub icons: Arc<crate::plugin_icons::PluginIconStore>,
    pub applications: Arc<crate::applications::ApplicationStore>,
}
impl std::ops::Deref for Core {
    type Target = WorkspaceServices;
    fn deref(&self) -> &Self::Target {
        &self.services
    }
}
pub struct Core {
    pub services: Arc<WorkspaceServices>,
    pub profile_error: Option<String>,
    pub adapter: OnceLock<Arc<dyn crate::devices::DeviceAdapter>>,
    pub profile_id: Option<String>,
    pub adapter_id: String,
    pub layout: DeviceLayout,
    pub supports_pages: bool,
    pub device_definition: DeviceDefinition,
    pub action_slots: Arc<tokio::sync::Semaphore>,
    pub manager: OnceLock<Weak<crate::devices::DeviceManager>>,
    pub widget_changed: broadcast::Sender<Vec<WidgetSample>>,
    pub action_errors: broadcast::Sender<UiError>,
    pub inner: Mutex<Inner>,
    pub changed: broadcast::Sender<Snapshot>,
    pub events: broadcast::Sender<DeviceEvent>,
    pub wake: Notify,
    pub restart: Notify,
    pub stop: watch::Sender<bool>,
}
impl Core {
    pub fn set_theme_checked(&self, theme: String, expected: Option<&str>) -> Result<()> {
        anyhow::ensure!(
            ["system", "light", "dark"].contains(&theme.as_str()),
            "Invalid theme"
        );
        let mut inner = self.inner.lock().unwrap();
        crate::revisions::check(expected, &inner.snapshot.config.theme)?;
        let mut next = inner.snapshot.config.clone();
        next.theme = theme;
        self.persist(&next)?;
        inner.snapshot.config = next;
        drop(inner);
        self.emit();
        Ok(())
    }
    #[cfg(all(test, feature = "device-m18"))]
    pub(crate) fn new_test_device(root: PathBuf) -> Result<Arc<Self>> {
        storage::initialize_workspace(&root)?;
        let config = crate::devices::default_config();
        let existing = storage::load_workspace(&root)?
            .profiles
            .into_iter()
            .find(|p| p.adapter_id == crate::devices::default_adapter_id());
        let profile = match existing {
            Some(profile) => profile,
            None => {
                let profile = DeviceProfile::new(
                    crate::devices::default_adapter_id().into(),
                    "test".into(),
                    None,
                    config,
                );
                storage::edit_workspace(&root, |w| {
                    w.profiles.push(profile.clone());
                    Ok(())
                })?;
                profile
            }
        };
        let core = Self::create(
            root,
            Some(profile.id),
            profile.adapter_id,
            profile.config,
            None,
        )?;
        let _ = core.adapter.set(Arc::new(crate::devices::m18::Adapter));
        Ok(core)
    }
    fn clear_queued_led_commands(inner: &mut Inner) {
        let stale: Vec<String> = inner
            .queue
            .iter()
            .filter(|work| work.command.merge_key() == Some("led_state"))
            .filter_map(|work| work.receipt_id.clone())
            .collect();
        inner
            .queue
            .retain(|work| work.command.merge_key() != Some("led_state"));
        for receipt in &mut inner.snapshot.receipts {
            if stale.contains(&receipt.id) && receipt.status == "queued" {
                receipt.status = "superseded".into();
                receipt.detail = "已由新的 LED 设置取代".into();
            }
        }
    }

    pub fn new(root: PathBuf) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        crate::plugin_package::recover(&root)?;
        let config = storage::load(&root)?;
        Self::create(
            root,
            None,
            crate::devices::default_adapter_id().into(),
            config,
            None,
        )
    }
    pub fn for_profile(
        host: &Arc<Core>,
        profile: &DeviceProfile,
        definition: DeviceDefinition,
    ) -> Result<Arc<Self>> {
        let mut core = Self::create(
            host.root.clone(),
            Some(profile.id.clone()),
            profile.adapter_id.clone(),
            profile.config.clone(),
            Some(host),
        )?;
        Arc::get_mut(&mut core).unwrap().supports_pages = definition.supports_pages;
        Arc::get_mut(&mut core).unwrap().layout = definition.layout.clone();
        Arc::get_mut(&mut core).unwrap().device_definition = definition;
        Ok(core)
    }
    fn create(
        root: PathBuf,
        profile_id: Option<String>,
        adapter_id: String,
        config: AppConfig,
        host: Option<&Arc<Core>>,
    ) -> Result<Arc<Self>> {
        let (changed, _) = broadcast::channel(32);
        let (events, _) = broadcast::channel(128);
        let (stop, _) = watch::channel(false);
        let (action_errors, _) = broadcast::channel(16);
        let (widget_changed, _) = broadcast::channel(16);
        let services = host.map(|h| h.services.clone()).unwrap_or_else(|| {
            Arc::new(WorkspaceServices {
                library: Arc::new(crate::library::LibraryStore::new(&root, &config)),
                applications: Arc::new(crate::applications::ApplicationStore::new(&root, &config)),
                icons: Arc::new(crate::plugin_icons::PluginIconStore::new(&root)),
                root,
            })
        });
        Ok(Arc::new(Self {
            profile_error: None,
            adapter: OnceLock::new(),
            profile_id,
            supports_pages: crate::devices::definition(&adapter_id).supports_pages,
            device_definition: crate::devices::definition(&adapter_id),
            action_slots: Arc::new(tokio::sync::Semaphore::new(8)),
            layout: crate::devices::adapter_layout(&adapter_id),
            adapter_id,
            manager: OnceLock::new(),
            widget_changed,
            services,
            action_errors,
            inner: Mutex::new(Inner {
                module_state: serde_json::Value::Null,
                analog_armed: Default::default(),
                scene_receipts: Default::default(),
                scene_repaint: false,
                scene_frame_error: None,
                scene_generation: 0,
                scenes_stopping: false,
                widgets: crate::widgets::WidgetState::default(),
                led: config.device_settings.clone(),
                snapshot: Snapshot {
                    revisions: Default::default(),
                    ui_message: None,
                    input_values: Default::default(),
                    active_page: config.active_page.clone(),
                    connection: "disconnected".into(),
                    error: None,
                    devices: vec![],
                    selected_device: None,
                    config,
                    pressed: vec![],
                    last_key_press: None,
                    logs: vec![],
                    receipts: vec![],
                    plugins: vec![],
                    sdk_pid: None,
                    gif_status: None,
                },
                paths: BTreeMap::new(),
                connection_epoch: 0,
                queue: VecDeque::new(),
                epoch: 0,
                overrides: BTreeMap::new(),
                icon_overrides: BTreeMap::new(),
                led_frame: None,
                led_effect_started_at: Instant::now(),
                led_shutdown: false,
                source_epochs: BTreeMap::new(),
            }),
            changed,
            events,
            wake: Notify::new(),
            restart: Notify::new(),
            stop,
        }))
    }
    pub fn persist(&self, config: &AppConfig) -> Result<()> {
        if let Some(error) = &self.profile_error {
            bail!("配置不可用：{error}")
        }
        if self.profile_id.is_some() {
            storage::validate_device_config(config, &self.definition())?;
            if let Some(adapter) = self.adapter.get() {
                adapter.validate(config)?;
                let workspace = storage::load_workspace(&self.root)?;
                let profile = workspace
                    .profiles
                    .iter()
                    .find(|p| Some(&p.id) == self.profile_id.as_ref())
                    .context(alchemy_device_api::UiFailure::new(
                        "backend.b041",
                        "未知设备配置",
                    ))?;
                if profile.config_version != 1 || profile.schema_version != adapter.config_version()
                {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b042",
                        "配置版本不支持"
                    ))
                }
                adapter.validate_module(&profile.module_data)?;
                for id in adapter.asset_references(&profile.module_data)? {
                    storage::asset(&self.root, &id)?;
                }
            }
            self.validate_config_assets(config)?;
        }
        if let Some(id) = &self.profile_id {
            storage::save_profile(&self.root, id, config)
        } else {
            storage::save(&self.root, config)
        }
    }
    pub fn device_manager(&self) -> Option<Arc<crate::devices::DeviceManager>> {
        self.manager.get().and_then(Weak::upgrade)
    }
    pub fn display_size(&self, key: u8) -> (u16, u16) {
        self.device_definition
            .displays
            .iter()
            .find(|d| d.key == key)
            .map(|d| (d.width, d.height))
            .unwrap_or((self.layout.key_width, self.layout.key_height))
    }
    pub fn key_png(&self, visual: &KeyVisual) -> Result<Vec<u8>> {
        let image = visual
            .plugin_icon
            .as_ref()
            .filter(|_| visual.renders_image())
            .map(|icon| self.icons.resolve(icon))
            .transpose()?
            .flatten();
        render::key_png_sized(
            &self.root,
            visual,
            image.as_deref().map(Vec::as_slice),
            self.display_size(visual.key).0,
            self.display_size(visual.key).1,
        )
    }
    pub fn library_catalog(&self) -> Result<LibraryCatalog> {
        Ok(self.icons.catalog(self.library.snapshot()?))
    }
    pub async fn refresh_icons(self: &Arc<Self>) -> Result<()> {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                for runtime in manager.runtimes() {
                    Box::pin(runtime.core.refresh_icons()).await?;
                }
            }
        }
        let (page, mut overrides, visuals, epoch) = {
            let i = self.inner.lock().unwrap();
            let page = i
                .snapshot
                .config
                .pages
                .iter()
                .find(|p| p.id == i.snapshot.active_page);
            if page.is_none_or(|p| !p.keys.iter().any(|k| k.plugin_icon.is_some()))
                && i.icon_overrides.is_empty()
            {
                return Ok(());
            }
            (
                i.snapshot.active_page.clone(),
                i.overrides.values().cloned().collect::<Vec<_>>(),
                i.icon_overrides.clone(),
                i.epoch + 1,
            )
        };
        self.activate_from(&page, ActivationSource::Refresh).await?;
        if self.inner.lock().unwrap().epoch != epoch {
            return Ok(());
        }
        overrides.sort_by_key(|(_, c)| matches!(c, HardwareCommand::LedBrightness { .. }));
        for (owner, command) in overrides {
            if let Some((_, visual)) = match &command {
                HardwareCommand::KeyData { key, .. } => visuals.get(&format!("key:{key}")),
                _ => None,
            } {
                let _ = self.plugin_visual(&owner, visual.clone(), epoch);
            } else {
                let _ = self.plugin_command(&owner, command, Some(epoch));
            }
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.inner.lock().unwrap().snapshot.clone();
        snapshot.revisions = Self::config_revisions(&snapshot.config);
        if snapshot.error.is_some() && snapshot.ui_message.is_none() {
            snapshot.ui_message = Some(UiMessage::new("errors.deviceFailed"));
        }
        snapshot
    }
    pub fn config_revisions(config: &AppConfig) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("pages".into(), crate::revisions::revision(&config.pages)),
            (
                "bindings".into(),
                crate::revisions::revision(&config.bindings),
            ),
            (
                "scenes".into(),
                crate::revisions::revision(&config.scene_settings),
            ),
            (
                "settings".into(),
                crate::revisions::revision(&config.device_settings),
            ),
        ])
    }
    pub fn scene_generation(&self) -> u64 {
        self.inner.lock().unwrap().scene_generation
    }
    pub fn scene_source_epoch(&self) -> u64 {
        self.inner
            .lock()
            .unwrap()
            .source_epochs
            .get("scene-automation")
            .copied()
            .unwrap_or(0)
    }
    pub fn invalidate_scene_work(&self) {
        self.cancel_owner("scene-automation");
    }
    pub fn scene_frame_state(&self) -> (bool, Option<String>) {
        let i = self.inner.lock().unwrap();
        (i.scene_repaint, i.scene_frame_error.clone())
    }
    pub fn retry_scene_frame(&self) {
        let mut i = self.inner.lock().unwrap();
        if i.scene_frame_error.take().is_some() {
            i.scene_repaint = true;
        }
    }
    pub fn stop_scenes(&self) {
        {
            let mut i = self.inner.lock().unwrap();
            i.scenes_stopping = true;
            i.scene_generation += 1;
        }
        self.invalidate_scene_work();
    }
    pub fn save_scenes(&self, settings: SceneSettings) -> Result<()> {
        self.save_scenes_checked(settings, None)
    }
    pub fn save_scenes_checked(
        &self,
        settings: SceneSettings,
        expected: Option<&str>,
    ) -> Result<()> {
        if !self.supports_pages {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b043",
                "此设备不支持页面自动化"
            ))
        }
        let mut i = self.inner.lock().unwrap();
        crate::revisions::check(expected, &i.snapshot.config.scene_settings)?;
        let mut next = i.snapshot.config.clone();
        next.scene_settings = settings;
        crate::scenes::validate(&next.scene_settings, &next.pages)?;
        self.persist(&next)?;
        i.snapshot.config = next;
        i.scene_generation += 1;
        drop(i);
        self.invalidate_scene_work();
        self.emit();
        Ok(())
    }
    pub fn pause_scenes(&self, paused: bool) -> Result<()> {
        let mut i = self.inner.lock().unwrap();
        if i.snapshot.config.scene_settings.paused == paused {
            return Ok(());
        }
        let mut next = i.snapshot.config.clone();
        next.scene_settings.paused = paused;
        self.persist(&next)?;
        i.snapshot.config = next;
        i.scene_generation += 1;
        drop(i);
        self.invalidate_scene_work();
        self.emit();
        Ok(())
    }
    pub fn emit(&self) {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                manager.sync_plugins();
                let _ = manager.changed.send(());
            }
        }
        let _ = self.changed.send(self.snapshot());
    }
    pub fn log(&self, level: &str, message: impl Into<String>) {
        let message: String = message.into().chars().take(1800).collect();
        let entry = LogEntry {
            at: now(),
            level: level.into(),
            message,
        };
        {
            let mut i = self.inner.lock().unwrap();
            i.snapshot.logs.push(entry.clone());
            if i.snapshot.logs.len() > 150 {
                i.snapshot.logs.remove(0);
            }
        }
        let file = self.root.join("runtime.log");
        if std::fs::metadata(&file).is_ok_and(|m| m.len() > 2_000_000) {
            let _ = std::fs::rename(&file, self.root.join("runtime.previous.log"));
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(file)
        {
            let _ =
                f.write_all(format!("{} {} {}\n", entry.at, entry.level, entry.message).as_bytes());
        }
        self.emit();
    }
    pub fn connection(&self, status: &str, error: Option<String>) {
        if self.profile_id.is_none() && status == "error" {
            if let Some(manager) = self.device_manager() {
                for runtime in manager.runtimes_for(&self.adapter_id) {
                    runtime.core.connection(status, error.clone());
                }
            }
        }
        let mut i = self.inner.lock().unwrap();
        i.snapshot.connection = status.into();
        i.snapshot.error = error;
        drop(i);
        self.emit();
    }
    pub fn event(&self, e: DeviceEvent) {
        use std::io::Write;
        let file = self.root.join("events.jsonl");
        if std::fs::metadata(&file).is_ok_and(|m| m.len() > 2_000_000) {
            let _ = std::fs::rename(&file, self.root.join("events.previous.jsonl"));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(file)
        {
            if let Ok(line) = serde_json::to_string(&e) {
                let _ = f.write_all(format!("{line}\n").as_bytes());
            }
        }
        if let Some(key) = e.key {
            match e.kind.as_str() {
                "button.down" => self.log("input", format!("按键 {key} 已按下")),
                "button.up" => self.log("input", format!("按键 {key} 已松开")),
                _ => {}
            }
        }
        let _ = self.events.send(e);
        self.emit();
    }
    pub fn receipt(&self, id: &str, status: &str, detail: impl Into<String>) {
        let mut i = self.inner.lock().unwrap();
        let detail = detail.into();
        if matches!(status, "acknowledged" | "failed" | "cancelled") && i.scene_receipts.remove(id)
        {
            if status == "cancelled" {
                i.scene_repaint = true;
            }
            if status == "failed" {
                i.scene_frame_error = Some(detail.clone());
            }
        }
        let mut updated = None;
        if let Some(r) = i.snapshot.receipts.iter_mut().find(|r| r.id == id) {
            r.status = status.into();
            r.at = now();
            r.detail = detail;
            updated = Some(r.clone());
        }
        drop(i);
        if let Some(r) = updated {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.root.join("commands.jsonl"))
            {
                if let Ok(line) = serde_json::to_string(&r) {
                    let _ = f.write_all(format!("{line}\n").as_bytes());
                }
            }
        }
        self.emit();
    }
    pub fn clear_connection(&self) {
        self.clear_connection_checked(None);
    }
    pub fn disconnect_session(&self, connection: &alchemy_device_api::Connection) {
        self.clear_connection_checked(Some(connection));
    }
    fn clear_connection_checked(&self, expected: Option<&alchemy_device_api::Connection>) {
        let mut i = self.inner.lock().unwrap();
        if let Some(token) = expected {
            if i.connection_epoch != token.epoch
                || i.paths.get(&token.device_id) != Some(&token.path)
            {
                return;
            }
            i.snapshot.connection = "disconnected".into();
            i.snapshot.error = None;
        }
        let ids: i32 = i.snapshot.devices.len() as i32;
        let devices = i.snapshot.devices.clone();
        i.connection_epoch += 1;
        i.epoch += 1;
        i.snapshot.sdk_pid = None;
        i.paths.clear();
        i.widgets.clear_frames();
        i.led = i.snapshot.config.device_settings.clone();
        i.led_frame = None;
        i.led_effect_started_at = Instant::now();
        i.queue.clear();
        i.scene_receipts.clear();
        i.scene_repaint = false;
        i.scene_frame_error = None;
        i.snapshot.devices.clear();
        i.snapshot.selected_device = None;
        i.snapshot.pressed.clear();
        i.snapshot.input_values.clear();
        i.analog_armed.clear();
        i.module_state = serde_json::Value::Null;
        i.snapshot.last_key_press = None;
        for r in &mut i.snapshot.receipts {
            if r.status == "queued" {
                r.status = "cancelled".into();
                r.detail = "设备已断开，命令未发送".into();
            }
        }
        drop(i);
        if ids > 0 {
            for d in devices {
                self.event(DeviceEvent {
                    input: None,
                    at: now(),
                    kind: "device.disconnected".into(),
                    device_id: d.id,
                    key: None,
                    pressed: None,
                    page_id: None,
                });
            }
        }
        self.emit();
    }
    pub fn validate_command(&self, c: &HardwareCommand) -> Result<()> {
        self.validate_command_inner(c).map_err(|e| {
            alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::InvalidArgument,
                e.to_string(),
            )
            .into()
        })
    }
    fn validate_command_inner(&self, c: &HardwareCommand) -> Result<()> {
        if self.layout.display_keys.is_empty()
            && self.layout.background_width == 0
            && self.layout.led_count == 0
        {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b044",
                "此设备不支持硬件写入"
            ))
        }
        match c {
            HardwareCommand::ScreenBrightness { value } if *value > 100 => {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b045",
                    "屏幕亮度范围 0–100"
                ))
            }
            HardwareCommand::KeyImage { key, .. }
            | HardwareCommand::KeyData { key, .. }
            | HardwareCommand::ClearKey { key }
            | HardwareCommand::KeyGif { key, .. }
            | HardwareCommand::ClearGif { key }
                if !self.layout.display_keys.contains(key) =>
            {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b046",
                    "目标按键不支持图像"
                ))
            }
            HardwareCommand::BulkImage { keys, .. }
                if keys.is_empty()
                    || keys.len() > self.layout.display_keys.len()
                    || keys.iter().any(|k| !self.layout.display_keys.contains(k)) =>
            {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b047",
                    "批量图像按键不符合设备布局"
                ))
            }
            HardwareCommand::LedColors { colors }
                if colors.len() != self.layout.led_count as usize =>
            {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b048",
                    "逐灯颜色数量与设备不一致"
                ))
            }
            _ => {}
        }
        match c {
            HardwareCommand::LedColor { color } => {
                storage::color(color)?;
            }
            HardwareCommand::KeyImage { asset_id, .. }
            | HardwareCommand::BulkImage { asset_id, .. }
            | HardwareCommand::Background { asset_id, .. } => {
                storage::asset(&self.root, asset_id)?;
            }
            HardwareCommand::KeyGif { asset_id, .. } => {
                let p = storage::asset(&self.root, asset_id)?;
                if p.extension().is_none_or(|e| e != "gif") {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b049",
                        "动图须为 GIF"
                    ))
                }
            }
            HardwareCommand::KeyData { data, .. }
            | HardwareCommand::BackgroundData { data, .. } => {
                storage::validate_image(&storage::decode_data(data)?)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub fn enqueue(&self, c: HardwareCommand, epoch: Option<u64>) -> Result<String> {
        self.enqueue_owned(c, epoch, None)
    }
    fn enqueue_owned(
        &self,
        c: HardwareCommand,
        epoch: Option<u64>,
        source: Option<&str>,
    ) -> Result<String> {
        self.validate_command(&c)?;
        let mut i = self.inner.lock().unwrap();
        let receipt = Self::enqueue_locked(&mut i, c, epoch, source, None)?;
        drop(i);
        self.wake.notify_one();
        self.emit();
        Ok(receipt)
    }
    fn enqueue_locked(
        i: &mut Inner,
        c: HardwareCommand,
        epoch: Option<u64>,
        source: Option<&str>,
        page_batch: Option<u64>,
    ) -> Result<String> {
        let id = i.snapshot.selected_device.clone().ok_or_else(|| {
            alchemy_device_api::ApiError::localized(
                alchemy_device_api::ErrorCode::Offline,
                "backend.e011",
                "设备尚未连接",
            )
        })?;
        let d = i
            .snapshot
            .devices
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| {
                alchemy_device_api::ApiError::localized(
                    alchemy_device_api::ErrorCode::Offline,
                    "backend.e012",
                    "设备已断开",
                )
            })?;
        if i.snapshot
            .config
            .disabled_capabilities
            .iter()
            .any(|id| id == c.capability())
            || !d
                .capabilities
                .get(c.capability())
                .is_some_and(|c| c.available)
        {
            return Err(alchemy_device_api::ApiError::localized(
                alchemy_device_api::ErrorCode::Unsupported,
                "backend.e013",
                "当前设备不支持此功能",
            )
            .into());
        }
        if let Some(e) = epoch {
            if e != i.epoch {
                return Err(alchemy_device_api::ApiError::localized(
                    alchemy_device_api::ErrorCode::StaleOperation,
                    "backend.e014",
                    "页面已切换，旧操作已取消",
                )
                .into());
            }
        }
        if i.queue.len() >= 4096 {
            return Err(alchemy_device_api::ApiError::localized(
                alchemy_device_api::ErrorCode::ResourceExhausted,
                "backend.e015",
                "设备命令队列已满，请稍后重试",
            )
            .into());
        }
        let is_led_command = matches!(
            &c,
            HardwareCommand::LedBrightness { .. }
                | HardwareCommand::LedColor { .. }
                | HardwareCommand::LedColors { .. }
                | HardwareCommand::ResetLed
        );
        if is_led_command {
            i.led_frame = None;
        }
        let elapsed = i.led_effect_started_at.elapsed();
        let led_command = i.led.resolve_led_at(&c, elapsed)?;
        if let Some(k) = c.merge_key() {
            let stale: Vec<String> = i
                .queue
                .iter()
                .filter(|w| w.command.merge_key() == Some(k))
                .filter_map(|w| w.receipt_id.clone())
                .collect();
            i.queue.retain(|w| w.command.merge_key() != Some(k));
            for r in &mut i.snapshot.receipts {
                if stale.contains(&r.id) {
                    r.status = "superseded".into();
                    r.detail = "已合并为最新值".into();
                }
            }
        }
        let receipt_id = uuid::Uuid::new_v4().to_string();
        i.snapshot.receipts.push(CommandReceipt {
            device_id: id.clone(),
            id: receipt_id.clone(),
            command: serde_json::to_value(&c)?["type"]
                .as_str()
                .unwrap_or("command")
                .into(),
            status: "queued".into(),
            at: now(),
            detail: "等待发送".into(),
        });
        if i.snapshot.receipts.len() > 60 {
            i.snapshot.receipts.remove(0);
        }
        let source_epoch = source
            .and_then(|s| i.source_epochs.get(s))
            .copied()
            .unwrap_or(0);
        i.queue.push_back(Work {
            widget_frame: None,
            image_frame: None,
            icon_revision: None,
            source: source.map(str::to_owned),
            source_epoch,
            page_batch,
            receipt_id: Some(receipt_id.clone()),
            connection_epoch: i.connection_epoch,
            device_path: i.paths.get(&id).cloned().unwrap_or_default(),
            device_id: id,
            epoch,
            command: c,
            led_command,
        });
        Ok(receipt_id)
    }
    #[cfg(test)]
    fn enqueue_page_frame(
        &self,
        commands: Vec<HardwareCommand>,
        epoch: u64,
        settings: DeviceSettings,
    ) -> Result<bool> {
        self.enqueue_page_frame_owned(commands, epoch, settings, None, None, PageImages::default())
    }
    fn enqueue_page_frame_owned(
        &self,
        commands: Vec<HardwareCommand>,
        epoch: u64,
        settings: DeviceSettings,
        scene_guard: Option<(u64, u64, Instant)>,
        page_id: Option<&str>,
        images: PageImages,
    ) -> Result<bool> {
        for command in &commands {
            if !matches!(command, HardwareCommand::KeyData { key, .. } if images.frames.contains_key(key))
            {
                self.validate_command(command)?;
            }
        }
        if images
            .revision
            .is_some_and(|revision| revision != self.icons.revision())
        {
            return Ok(false);
        }
        let mut prepared_bytes = images.frames.values().map(|v| v.len() as u64).sum::<u64>();
        for command in &commands {
            match command {
                HardwareCommand::KeyImage { asset_id, .. }
                | HardwareCommand::KeyGif { asset_id, .. }
                | HardwareCommand::Background { asset_id, .. } => {
                    prepared_bytes = prepared_bytes.saturating_add(
                        std::fs::metadata(storage::asset(&self.root, asset_id)?)?.len(),
                    );
                }
                HardwareCommand::KeyData { data, .. }
                | HardwareCommand::BackgroundData { data, .. } => {
                    prepared_bytes = prepared_bytes.saturating_add(data.len() as u64)
                }
                _ => {}
            }
        }
        if prepared_bytes
            > self
                .adapter
                .get()
                .map(|a| {
                    a.batch_limits()
                        .bytes
                        .min(alchemy_device_api::MAX_PAGE_BYTES)
                })
                .unwrap_or(alchemy_device_api::MAX_PAGE_BYTES) as u64
        {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ResourceExhausted,
                "整页准备数据超过设备预算",
            )
            .into());
        }
        let mut i = self.inner.lock().unwrap();
        if i.epoch != epoch {
            return Ok(false);
        }
        if let Some((generation, source_epoch, valid_until)) = scene_guard {
            if Instant::now() >= valid_until
                || i.scenes_stopping
                || i.scene_generation != generation
                || !i.snapshot.config.scene_settings.enabled
                || i.snapshot.config.scene_settings.paused
                || i.source_epochs
                    .get("scene-automation")
                    .copied()
                    .unwrap_or(0)
                    != source_epoch
            {
                return Ok(false);
            }
        }
        let device = i
            .snapshot
            .selected_device
            .as_ref()
            .and_then(|id| i.snapshot.devices.iter().find(|device| &device.id == id));
        let Some(device) = device else {
            return Ok(true);
        };
        let commands: Vec<_> = commands
            .into_iter()
            .filter(|command| {
                !i.snapshot
                    .config
                    .disabled_capabilities
                    .iter()
                    .any(|id| id == command.capability())
                    && device
                        .capabilities
                        .get(command.capability())
                        .is_some_and(|cap| cap.available)
            })
            .collect();
        if commands.len()
            > self
                .adapter
                .get()
                .map(|a| {
                    a.batch_limits()
                        .operations
                        .min(alchemy_device_api::MAX_PAGE_OPERATIONS)
                })
                .unwrap_or(alchemy_device_api::MAX_PAGE_OPERATIONS)
            || i.queue.len() + commands.len() > 4096
        {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ResourceExhausted,
                "设备命令或整页预算已满，更新未入队",
            )
            .into());
        }
        i.led = settings;
        i.led_frame = None;
        // No consumer can observe a partially queued page or interleave a plugin write.
        i.scene_receipts.clear();
        i.scene_repaint = false;
        i.scene_frame_error = None;
        for command in commands {
            let receipt = Self::enqueue_locked(
                &mut i,
                command,
                Some(epoch),
                scene_guard.map(|_| "scene-automation"),
                Some(epoch),
            )?;
            if let Some(work) = i.queue.back_mut() {
                work.icon_revision = images.revision;
                if let HardwareCommand::KeyData { key, .. } = &work.command {
                    work.image_frame = images.frames.get(key).cloned();
                }
            }
            if scene_guard.is_some() {
                i.scene_receipts.insert(receipt);
            }
        }
        if let Some(id) = page_id {
            i.snapshot.active_page = id.into();
        }
        i.widgets.page_ready = true;
        drop(i);
        self.wake.notify_one();
        self.emit();
        Ok(true)
    }
    /// Dequeue a complete admitted page. External writes keep normal pacing and
    /// cannot be pulled into a page merely because they have the same epoch.
    pub fn pop_batch(&self) -> Vec<Work> {
        let Some(first) = self.pop() else {
            return vec![];
        };
        let batch = first.page_batch;
        let mut works = vec![first];
        if let Some(epoch) = batch {
            let mut i = self.inner.lock().unwrap();
            while i.epoch == epoch
                && works.len() < alchemy_device_api::MAX_PAGE_OPERATIONS
                && i.queue.front().is_some_and(|work| work.page_batch == batch)
            {
                works.push(i.queue.pop_front().unwrap());
            }
        }
        works
    }
    /// Recheck after image preparation, before anything enters the SDK queue.
    pub fn work_is_current(&self, work: &Work) -> bool {
        let i = self.inner.lock().unwrap();
        work.icon_revision
            .is_none_or(|revision| revision == self.icons.revision())
            && work.epoch.is_none_or(|epoch| epoch == i.epoch)
            && crate::widgets::frame_is_current(&i, work)
            && work.connection_epoch == i.connection_epoch
            && i.paths.get(&work.device_id) == Some(&work.device_path)
            && work.source.as_ref().is_none_or(|source| {
                i.source_epochs.get(source).copied().unwrap_or(0) == work.source_epoch
            })
    }
    pub fn pop(&self) -> Option<Work> {
        let mut i = self.inner.lock().unwrap();
        while let Some(w) = i.queue.pop_front() {
            if w.epoch.is_none_or(|e| e == i.epoch) && i.paths.contains_key(&w.device_id) {
                return Some(w);
            }
            if let Some(r) = i
                .snapshot
                .receipts
                .iter_mut()
                .find(|r| Some(r.id.as_str()) == w.receipt_id.as_deref())
            {
                r.status = "cancelled".into();
                r.detail = "旧页面命令已取消".into();
            }
        }
        while let Some((_, work)) = i.widgets.pending.pop_first() {
            if crate::widgets::frame_is_current(&i, &work) && work.epoch == Some(i.epoch) {
                return Some(work);
            }
        }
        let individual_leds_available = i
            .snapshot
            .selected_device
            .as_ref()
            .and_then(|id| i.snapshot.devices.iter().find(|device| &device.id == id))
            .and_then(|device| device.capabilities.get("individual_leds"))
            .is_some_and(|capability| capability.available);
        if i.led_shutdown
            || !i.led.led_mode.is_animated()
            || i.overrides.contains_key("led_color")
            || i.overrides.contains_key("led_brightness")
            || !individual_leds_available
        {
            i.led_frame = None;
            return None;
        }
        let colors = i.led_frame.take()?;
        let device_id = i.snapshot.selected_device.clone()?;
        if !i.paths.contains_key(&device_id) {
            return None;
        }
        let command = HardwareCommand::LedColors { colors };
        Some(Work {
            widget_frame: None,
            image_frame: None,
            icon_revision: None,
            source: None,
            source_epoch: 0,
            page_batch: None,
            receipt_id: None,
            connection_epoch: i.connection_epoch,
            device_path: i.paths.get(&device_id).cloned().unwrap_or_default(),
            device_id,
            epoch: None,
            command: command.clone(),
            led_command: Some(command),
        })
    }

    /// Render into one replaceable slot. Animation frames have no public receipt and
    /// cannot accumulate behind normal hardware commands.
    pub fn tick_led_effect(&self) {
        let mut i = self.inner.lock().unwrap();
        let individual_leds_available = i
            .snapshot
            .selected_device
            .as_ref()
            .and_then(|id| i.snapshot.devices.iter().find(|d| &d.id == id))
            .and_then(|device| device.capabilities.get("individual_leds"))
            .is_some_and(|capability| capability.available);
        if i.led_shutdown
            || !i.led.led_mode.is_animated()
            || i.overrides.contains_key("led_color")
            || i.overrides.contains_key("led_brightness")
            || !individual_leds_available
            || i.snapshot
                .selected_device
                .as_ref()
                .is_none_or(|id| !i.paths.contains_key(id))
        {
            i.led_frame = None;
            return;
        }
        let elapsed = i.led_effect_started_at.elapsed();
        i.led_frame =
            crate::led::render_frame_count(&i.led, elapsed, self.layout.led_count as usize).ok();
    }

    pub fn led_effect_active(&self) -> bool {
        let i = self.inner.lock().unwrap();
        i.led.led_mode.is_animated()
            && !i.led_shutdown
            && !i.overrides.contains_key("led_color")
            && !i.overrides.contains_key("led_brightness")
            && i.snapshot
                .selected_device
                .as_ref()
                .and_then(|id| i.snapshot.devices.iter().find(|device| &device.id == id))
                .and_then(|device| device.capabilities.get("individual_leds"))
                .is_some_and(|capability| capability.available)
    }

    pub fn configure_led_settings(&self, settings: DeviceSettings) -> Result<()> {
        self.apply_led_settings(settings, true, None)?
            .map_or(Ok(()), |e| Err(anyhow::anyhow!(e)))
    }

    pub fn configure_settings_checked(
        &self,
        settings: DeviceSettings,
        expected: &str,
    ) -> Result<Option<String>> {
        self.apply_led_settings(settings, true, Some(expected))
    }
    /// Apply a temporary LED profile for the hardware diagnostic without changing
    /// the user's saved configuration.
    pub fn preview_led_settings(&self, settings: DeviceSettings) -> Result<()> {
        self.apply_led_settings(settings, false, None)?
            .map_or(Ok(()), |e| Err(anyhow::anyhow!(e)))
    }

    fn apply_led_settings(
        &self,
        settings: DeviceSettings,
        persist: bool,
        expected: Option<&str>,
    ) -> Result<Option<String>> {
        storage::validate_led_settings_count(&settings, self.layout.led_count as usize)?;
        let has_device = {
            let mut i = self.inner.lock().unwrap();
            crate::revisions::check(expected, &i.snapshot.config.device_settings)?;
            let feature = if settings.led_mode.requires_individual_leds() {
                "individual_leds"
            } else {
                "led_color"
            };
            if let Some(id) = &i.snapshot.selected_device {
                if i.paths.contains_key(id)
                    && !i
                        .snapshot
                        .devices
                        .iter()
                        .find(|d| &d.id == id)
                        .and_then(|d| d.capabilities.get(feature))
                        .is_some_and(|cap| cap.available)
                {
                    bail!("当前设备不支持所选 LED 模式")
                }
            }
            if persist {
                let mut config = i.snapshot.config.clone();
                config.device_settings = settings.clone();
                self.persist(&config)?;
            }
            let previous = i.led.clone();
            let now = Instant::now();
            if previous.led_mode != settings.led_mode {
                i.led_effect_started_at = now;
            } else if previous.led_mode.is_animated() && previous.led_speed != settings.led_speed {
                let old_period = crate::led::cycle_period(previous.led_speed).as_secs_f64();
                let new_period = crate::led::cycle_period(settings.led_speed).as_secs_f64();
                let phase = (i.led_effect_started_at.elapsed().as_secs_f64() / old_period).fract();
                let elapsed = std::time::Duration::from_secs_f64(phase * new_period);
                i.led_effect_started_at = now.checked_sub(elapsed).unwrap_or(now);
            }
            Self::clear_queued_led_commands(&mut i);
            if persist {
                let mut config = i.snapshot.config.clone();
                config.device_settings = settings.clone();
                i.snapshot.config = config;
            }
            i.led = settings.clone();
            i.led_frame = None;
            i.led_shutdown = false;
            i.snapshot
                .selected_device
                .as_ref()
                .is_some_and(|id| i.paths.contains_key(id))
        };
        self.emit();
        if has_device {
            if let Some(command) = settings.saved_led_command() {
                return Ok(self.enqueue(command, None).err().map(|e| e.to_string()));
            }
        }
        Ok(None)
    }

    pub async fn reset_led_for_shutdown(&self) -> Result<()> {
        if self.layout.led_count == 0 {
            return Ok(());
        }
        let connected = {
            let mut i = self.inner.lock().unwrap();
            i.led_shutdown = true;
            i.led_frame = None;
            i.snapshot
                .selected_device
                .as_ref()
                .is_some_and(|id| i.paths.contains_key(id))
        };
        if !connected {
            return Ok(());
        }
        let receipt_id = self.enqueue(HardwareCommand::ResetLed, None)?;
        let acknowledged = tokio::time::timeout(std::time::Duration::from_millis(1500), async {
            loop {
                let receipt = self
                    .snapshot()
                    .receipts
                    .into_iter()
                    .find(|receipt| receipt.id == receipt_id);
                match receipt.map(|receipt| receipt.status) {
                    Some(status) if status == "acknowledged" => return Ok(()),
                    Some(status) if status == "failed" || status == "cancelled" => {
                        bail!("设备未确认恢复默认 LED 灯效")
                    }
                    _ => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
                }
            }
        })
        .await;
        match acknowledged {
            Ok(result) => result,
            Err(_) => bail!("恢复默认 LED 灯效等待超时"),
        }
    }
    pub fn update_config(&self, mut config: AppConfig) -> Result<()> {
        self.validate_config_assets(&config)?;
        let mut i = self.inner.lock().unwrap();
        config.verification = i.snapshot.config.verification.clone();
        config.managed_plugin_ids = i.snapshot.config.managed_plugin_ids.clone();
        config.disabled_plugins = i.snapshot.config.disabled_plugins.clone();
        config.scene_settings = i.snapshot.config.scene_settings.clone();
        self.persist(&config)?;
        i.snapshot.config = config;
        drop(i);
        self.emit();
        Ok(())
    }
    /// Save editor pages against the current configuration, without overwriting
    /// device settings or a page selected by a physical key during autosave.
    pub fn update_pages(&self, pages: Vec<Page>) -> Result<Option<String>> {
        self.update_pages_checked(pages, None)
    }
    pub fn update_pages_checked(
        &self,
        pages: Vec<Page>,
        expected: Option<&str>,
    ) -> Result<Option<String>> {
        if !self.supports_pages {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::Unsupported,
                "此设备不支持页面",
            )
            .into());
        }
        let mut i = self.inner.lock().unwrap();
        crate::revisions::check(expected, &i.snapshot.config.pages)?;
        let current = &i.snapshot.config;
        let mut next = current.clone();
        next.pages = pages;
        if !next.pages.iter().any(|p| p.id == next.active_page) {
            next.active_page = next.pages.first().context("至少保留一个页面")?.id.clone();
        }
        for scene in &mut next.scene_settings.scenes {
            if !next.pages.iter().any(|p| p.id == scene.page_id) {
                scene.enabled = false;
            }
        }
        if let SceneFallback::Page { page_id } = &next.scene_settings.fallback {
            if !next.pages.iter().any(|p| p.id == *page_id) {
                next.scene_settings.fallback = SceneFallback::Keep;
            }
        }
        let runtime_page = if next.pages.iter().any(|p| p.id == i.snapshot.active_page) {
            i.snapshot.active_page.clone()
        } else {
            next.pages[0].id.clone()
        };
        self.validate_config_assets(&next)?;
        let active_changed =
            serde_json::to_value(
                current
                    .pages
                    .iter()
                    .find(|p| p.id == i.snapshot.active_page),
            )? != serde_json::to_value(next.pages.iter().find(|p| p.id == runtime_page))?;
        self.persist(&next)?;
        let refresh = active_changed.then(|| runtime_page.clone());
        if active_changed {
            i.widgets.clear_frames();
        }
        i.snapshot.active_page = runtime_page;
        i.snapshot.config = next;
        i.scene_generation += 1;
        drop(i);
        self.invalidate_scene_work();
        self.emit();
        Ok(refresh)
    }
    pub(crate) fn validate_config_assets(&self, config: &AppConfig) -> Result<()> {
        for page in &config.pages {
            if let Some(id) = &page.background_asset {
                storage::asset(&self.root, id)?;
            }
            for key in &page.keys {
                for id in key.asset_id.iter().chain(key.default_icon_asset_id.iter()) {
                    storage::asset(&self.root, id)?;
                }
            }
        }
        Ok(())
    }
    pub fn set_capability_enabled(&self, feature: &str, enabled: bool) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if !inner
            .snapshot
            .devices
            .iter()
            .any(|d| d.capabilities.contains_key(feature))
        {
            bail!("未知能力")
        }
        let mut config = inner.snapshot.config.clone();
        config.disabled_capabilities.retain(|id| id != feature);
        if !enabled {
            config.disabled_capabilities.push(feature.into());
        }
        self.persist(&config)?;
        inner.snapshot.config = config;
        // Changing a capability cancels every pending/preparing job for this session.
        inner.epoch += 1;
        inner.widgets.clear_frames();
        inner.queue.clear();
        for receipt in &mut inner.snapshot.receipts {
            if receipt.status == "queued" {
                receipt.status = "cancelled".into();
                receipt.detail = "能力设置已改变".into();
            }
        }
        drop(inner);
        self.emit();
        Ok(())
    }
    pub fn mark(&self, feature: &str, status: Verification, note: &str) -> Result<()> {
        if note.trim().is_empty() {
            bail!("请填写实际观察结果")
        };
        let mut i = self.inner.lock().unwrap();
        let id = i.snapshot.selected_device.clone().context("设备未连接")?;
        let mut devices = i.snapshot.devices.clone();
        let d = devices.iter_mut().find(|d| d.id == id).ok_or_else(|| {
            alchemy_device_api::ApiError::new(alchemy_device_api::ErrorCode::Offline, "设备已断开")
        })?;
        let cap = d.capabilities.get_mut(feature).context("未知能力")?;
        cap.verification = status;
        cap.reason = note.into();
        let key = format!("{}|{}", d.id, d.firmware);
        let mut config = i.snapshot.config.clone();
        config.verification.insert(key, d.capabilities.clone());
        self.persist(&config)?;
        i.snapshot.config = config;
        i.snapshot.devices = devices;
        drop(i);
        self.emit();
        Ok(())
    }
    pub async fn activate(self: &Arc<Self>, id: &str) -> Result<()> {
        self.activate_from(id, ActivationSource::Manual).await
    }
    pub async fn activate_from(self: &Arc<Self>, id: &str, source: ActivationSource) -> Result<()> {
        if !self.supports_pages {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::Unsupported,
                "此设备不支持页面",
            )
            .into());
        }
        let (page, epoch, guard) = {
            let mut i = self.inner.lock().unwrap();
            let p = i
                .snapshot
                .config
                .pages
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .ok_or_else(|| {
                    alchemy_device_api::ApiError::new(
                        alchemy_device_api::ErrorCode::NotFound,
                        "页面不存在",
                    )
                })?;
            let guard = if let ActivationSource::Scene {
                generation,
                source_epoch,
                valid_until,
            } = source
            {
                if Instant::now() >= valid_until
                    || source_epoch
                        != i.source_epochs
                            .get("scene-automation")
                            .copied()
                            .unwrap_or(0)
                    || i.scenes_stopping
                    || generation != i.scene_generation
                    || !i.snapshot.config.scene_settings.enabled
                    || i.snapshot.config.scene_settings.paused
                {
                    bail!(crate::scenes::ActivationCancelled);
                }
                Some((generation, source_epoch, valid_until))
            } else {
                None
            };
            if matches!(
                source,
                ActivationSource::Manual | ActivationSource::External
            ) {
                let mut next = i.snapshot.config.clone();
                next.active_page = id.into();
                if next.scene_settings.enabled {
                    next.scene_settings.paused = true;
                }
                self.persist(&next)?;
                i.snapshot.config = next;
                i.scene_generation += 1;
                *i.source_epochs
                    .entry("scene-automation".into())
                    .or_default() += 1;
            }
            i.scene_receipts.clear();
            i.scene_repaint = false;
            i.scene_frame_error = None;
            i.epoch += 1;
            i.widgets.clear_frames();
            i.overrides.clear();
            i.icon_overrides.clear();
            i.led_frame = None;
            Self::clear_queued_led_commands(&mut i);
            let e = i.epoch;
            let stale: Vec<_> = i
                .queue
                .iter()
                .filter(|w| w.command.visual())
                .filter_map(|w| w.receipt_id.clone())
                .collect();
            i.queue.retain(|w| !w.command.visual());
            for r in &mut i.snapshot.receipts {
                if stale.contains(&r.id) {
                    r.status = "cancelled".into();
                    r.detail = "页面已更新".into();
                }
            }
            if guard.is_none() || i.snapshot.selected_device.is_none() {
                i.snapshot.active_page = id.into();
            }
            (p, e, guard)
        };
        let event = DeviceEvent {
            input: None,
            at: now(),
            kind: "page.changed".into(),
            device_id: self.snapshot().selected_device.unwrap_or_default(),
            key: None,
            pressed: None,
            page_id: Some(id.into()),
        };
        if self.snapshot().selected_device.is_none() {
            self.event(event);
            return Ok(());
        }
        let root = self.root.clone();
        let core = self.clone();
        let revision = page
            .keys
            .iter()
            .any(|k| k.plugin_icon.is_some())
            .then(|| self.icons.revision());
        let (mut rendered, frames) =
            tokio::task::spawn_blocking(move || -> Result<(Vec<HardwareCommand>, ImageFrames)> {
                let mut frames = BTreeMap::new();
                let mut cmds = vec![HardwareCommand::StopGif];
                if core.layout.background_width > 0 && core.layout.background_height > 0 {
                    let bg = if let Some(id) = page.background_asset {
                        id
                    } else {
                        storage::import(
                            &root,
                            &render::blank_background_sized(
                                &root,
                                core.layout.background_width,
                                core.layout.background_height,
                            )?,
                        )?
                    };
                    cmds.push(HardwareCommand::Background {
                        asset_id: bg,
                        persistent: false,
                    });
                }
                let mut animated = false;
                for k in page
                    .keys
                    .iter()
                    .filter(|k| core.layout.display_keys.contains(&k.key))
                {
                    if let Some(id) = k.animated_asset() {
                        cmds.push(HardwareCommand::KeyGif {
                            key: k.key,
                            asset_id: id.to_owned(),
                        });
                        animated = true;
                        continue;
                    }
                    cmds.push(HardwareCommand::ClearGif { key: k.key });
                    if k.plugin_icon.is_some() {
                        frames.insert(k.key, Arc::new(core.key_png(k)?));
                        cmds.push(HardwareCommand::KeyData {
                            key: k.key,
                            data: String::new(),
                        });
                    } else {
                        let image = render::key_sized(
                            &root,
                            k,
                            core.display_size(k.key).0,
                            core.display_size(k.key).1,
                        )?;
                        cmds.push(HardwareCommand::KeyImage {
                            key: k.key,
                            asset_id: storage::import(&root, &image)?,
                        });
                    }
                }
                cmds.push(HardwareCommand::Refresh);
                if animated {
                    cmds.push(HardwareCommand::StartGif)
                }
                Ok((cmds, frames))
            })
            .await??;
        let settings = self.snapshot().config.device_settings;
        rendered.push(HardwareCommand::ScreenBrightness {
            value: settings.screen_brightness,
        });
        if let Some(command) = settings.saved_led_command() {
            rendered.push(command);
        }
        if !self.enqueue_page_frame_owned(
            rendered,
            epoch,
            settings,
            guard,
            Some(id),
            PageImages { frames, revision },
        )? {
            bail!(crate::scenes::ActivationCancelled);
        }
        self.event(event);
        Ok(())
    }
    pub async fn execute_action(self: &Arc<Self>, action: KeyAction) -> Result<()> {
        if *self.stop.borrow() {
            bail!("应用正在退出")
        }
        if let Some(target) = crate::launch_service::action_target(&action) {
            tokio::task::spawn_blocking(move || crate::launch_service::execute(&target)).await??;
            self.log("info", "Windows 已接受启动请求");
            return Ok(());
        }
        if !self.supports_pages && !matches!(action, KeyAction::None) {
            bail!("此设备不支持页面导航")
        }
        let s = self.snapshot();
        let index = s
            .config
            .pages
            .iter()
            .position(|p| p.id == s.active_page)
            .unwrap_or(0);
        let target = match action {
            KeyAction::None => return Ok(()),
            KeyAction::Home => s.config.pages[0].id.clone(),
            KeyAction::Next => s.config.pages[(index + 1) % s.config.pages.len()]
                .id
                .clone(),
            KeyAction::Previous => s.config.pages
                [(index + s.config.pages.len() - 1) % s.config.pages.len()]
            .id
            .clone(),
            KeyAction::Page { page_id } => page_id,
            KeyAction::Launch { .. } => {
                unreachable!("launch actions returned above")
            }
        };
        self.activate(&target).await
    }
    /// Keep reference-bearing temporary visuals live without persisting pixels.
    pub fn plugin_visual(&self, owner: &str, visual: KeyVisual, epoch: u64) -> Result<String> {
        if !self.layout.display_keys.contains(&visual.key) {
            bail!("目标按键不支持显示");
        }
        let revision = self.icons.revision();
        let png = Arc::new(self.key_png(&visual)?);
        let key = visual.key;
        let command = HardwareCommand::KeyData {
            key,
            data: String::new(),
        };
        let mut i = self.inner.lock().unwrap();
        if revision != self.icons.revision() {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::StaleOperation,
                "插件图标已更新，请重试",
            )
            .into());
        }
        let receipt =
            Self::enqueue_locked(&mut i, command.clone(), Some(epoch), Some(owner), None)?;
        let work = i.queue.back_mut().expect("just queued visual");
        work.image_frame = Some(png);
        work.icon_revision = Some(revision);
        let slot = format!("key:{key}");
        i.overrides.insert(slot.clone(), (owner.into(), command));
        i.icon_overrides.insert(slot, (owner.into(), visual));
        drop(i);
        self.wake.notify_one();
        self.emit();
        Ok(receipt)
    }
    pub fn plugin_command(
        &self,
        owner: &str,
        command: HardwareCommand,
        epoch: Option<u64>,
    ) -> Result<String> {
        let receipt = self.enqueue_owned(command.clone(), epoch, Some(owner))?;
        let slots: Vec<String> = match &command {
            HardwareCommand::KeyImage { key, .. }
            | HardwareCommand::KeyData { key, .. }
            | HardwareCommand::KeyGif { key, .. }
            | HardwareCommand::ClearKey { key }
            | HardwareCommand::ClearGif { key } => vec![format!("key:{key}")],
            HardwareCommand::Background { .. } | HardwareCommand::BackgroundData { .. } => {
                vec!["background".into()]
            }
            HardwareCommand::ScreenBrightness { .. } => vec!["screen_brightness".into()],
            HardwareCommand::LedBrightness { .. } => vec!["led_brightness".into()],
            HardwareCommand::LedColor { .. }
            | HardwareCommand::LedColors { .. }
            | HardwareCommand::ResetLed => vec!["led_color".into()],
            HardwareCommand::BulkImage { keys, .. } => {
                keys.iter().map(|k| format!("key:{k}")).collect()
            }
            HardwareCommand::ClearAll => self
                .layout
                .display_keys
                .iter()
                .map(|k| format!("key:{k}"))
                .collect(),
            HardwareCommand::StartGif | HardwareCommand::StopGif => vec!["gif_loop".into()],
            _ => vec![],
        };
        let mut i = self.inner.lock().unwrap();
        if epoch.is_none_or(|e| e == i.epoch) {
            for slot in slots {
                let per_slot = match &command {
                    HardwareCommand::BulkImage { asset_id, .. } => HardwareCommand::KeyImage {
                        key: slot[4..].parse().expect("internal key slot"),
                        asset_id: asset_id.clone(),
                    },
                    HardwareCommand::ClearAll => HardwareCommand::ClearKey {
                        key: slot[4..].parse().expect("internal key slot"),
                    },
                    _ => command.clone(),
                };
                i.icon_overrides.remove(&slot);
                i.overrides.insert(slot, (owner.into(), per_slot));
            }
        }
        Ok(receipt)
    }
    pub async fn restore_plugin(self: &Arc<Self>, owner: &str) -> Result<()> {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                Box::pin(manager.restore_owner(owner)).await?;
            }
        }
        self.cancel_owner(owner);
        let (mut remaining, visuals) = {
            let mut i = self.inner.lock().unwrap();
            let bound = i
                .snapshot
                .config
                .pages
                .iter()
                .filter(|p| p.id == i.snapshot.active_page)
                .flat_map(|p| &p.keys)
                .any(|k| k.widget.as_ref().is_some_and(|w| w.plugin_id == owner));
            if !bound && !i.overrides.values().any(|(id, _)| id == owner) {
                return Ok(());
            }
            i.overrides.retain(|_, (id, _)| id != owner);
            i.icon_overrides.retain(|_, (id, _)| id != owner);
            (
                i.overrides.values().cloned().collect::<Vec<_>>(),
                i.icon_overrides.clone(),
            )
        };
        let original = self.snapshot().active_page;
        let epoch = self.inner.lock().unwrap().epoch + 1;
        self.activate_from(&original, ActivationSource::Refresh)
            .await?;
        if self.inner.lock().unwrap().epoch != epoch {
            return Ok(());
        }
        // Establish a remaining plugin's static colour before applying its level,
        // particularly when the saved baseline uses the firmware default effect.
        remaining.sort_by_key(|(_, c)| matches!(c, HardwareCommand::LedBrightness { .. }));
        for (id, command) in remaining {
            if let Some((_, visual)) = match &command {
                HardwareCommand::KeyData { key, .. } => visuals.get(&format!("key:{key}")),
                _ => None,
            } {
                let _ = self.plugin_visual(&id, visual.clone(), epoch);
            } else {
                let _ = self.plugin_command(&id, command, Some(epoch));
            }
        }
        Ok(())
    }
    /// Invalidate queued or preparing work; a command already accepted by the SDK cannot be recalled.
    pub fn cancel_owner(&self, owner: &str) {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                for runtime in manager.runtimes() {
                    runtime.core.cancel_owner(owner);
                }
            }
        }
        let ids = {
            let mut i = self.inner.lock().unwrap();
            *i.source_epochs.entry(owner.into()).or_default() += 1;
            i.widgets.cancel(owner);
            let ids: Vec<String> = i
                .queue
                .iter()
                .filter(|w| w.source.as_deref() == Some(owner))
                .filter_map(|w| w.receipt_id.clone())
                .collect();
            i.queue.retain(|w| w.source.as_deref() != Some(owner));
            ids
        };
        for id in ids {
            self.receipt(&id, "cancelled", "控制来源已停止，命令未发送");
        }
    }
    pub fn export_report(&self) -> Result<PathBuf> {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                let file = self.root.join("acceptance-report.json");
                std::fs::write(
                    &file,
                    serde_json::to_vec_pretty(
                        &serde_json::json!({"generatedAt":now(),"workspace":manager.snapshot()?,"note":"各设备状态独立；SDK 回执不表示实机绘制完成。"}),
                    )?,
                )?;
                return Ok(file);
            }
        }
        let file = if let Some(profile_id) = &self.profile_id {
            use sha2::Digest;
            self.root.join(format!(
                "acceptance-report-{:x}.json",
                sha2::Sha256::digest(profile_id.as_bytes())
            ))
        } else {
            self.root.join("acceptance-report.json")
        };
        let s = self.snapshot();
        std::fs::write(
            &file,
            serde_json::to_vec_pretty(
                &serde_json::json!({"generatedAt":now(),"devices":s.devices,"verification":s.config.verification,"connection":s.connection,"receipts":s.receipts,"logs":s.logs,"inputHistory":std::fs::read_to_string(self.root.join("events.jsonl")).unwrap_or_default().lines().filter_map(|s|serde_json::from_str::<serde_json::Value>(s).ok()).collect::<Vec<_>>(),"note":"sent 表示已提交官方 C SDK 队列；acknowledged 表示 C SDK 接受调用。两者均不等于屏幕、灯光实机效果通过。Windows 由客户端进程唯一持有官方 C SDK，读取线程退出后释放设备句柄。"}),
            )?,
        )?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn automatic(c: &Core) -> ActivationSource {
        ActivationSource::Scene {
            generation: c.scene_generation(),
            source_epoch: c.scene_source_epoch(),
            valid_until: Instant::now() + std::time::Duration::from_secs(60),
        }
    }
    #[tokio::test]
    async fn scene_runtime_page_never_saves_and_manual_navigation_persists_pause() {
        let c = Core::new_test_device(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()))
            .unwrap();
        let mut pages = c.snapshot().config.pages;
        let base = pages[0].id.clone();
        let other = crate::devices::default_page("自动页面");
        pages.push(other.clone());
        c.update_pages(pages).unwrap();
        c.save_scenes(SceneSettings {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
        let before = std::fs::read(c.root.join("config.json")).unwrap();
        c.activate_from(&other.id, automatic(&c)).await.unwrap();
        assert_eq!(c.snapshot().active_page, other.id);
        assert_eq!(c.snapshot().config.active_page, base);
        assert_eq!(before, std::fs::read(c.root.join("config.json")).unwrap());
        c.activate_from(&other.id, ActivationSource::Reconnect)
            .await
            .unwrap();
        assert!(!c.snapshot().config.scene_settings.paused);
        c.execute_action(KeyAction::Home).await.unwrap();
        assert_eq!(c.snapshot().active_page, base);
        assert!(c.snapshot().config.scene_settings.paused);
        let restored = Core::new_test_device(c.root.clone()).unwrap();
        assert!(restored.snapshot().config.scene_settings.paused);
        assert!(c.activate_from(&other.id, automatic(&c)).await.is_err());
    }
    #[tokio::test]
    async fn scene_source_cancel_covers_dequeued_batches_and_does_not_pause_on_refresh() {
        let c = connected();
        c.save_scenes(SceneSettings {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
        let home = c.snapshot().active_page;
        c.activate_from(&home, automatic(&c)).await.unwrap();
        let batch = c.pop_batch();
        assert!(!batch.is_empty());
        assert!(batch
            .iter()
            .all(|w| w.source.as_deref() == Some("scene-automation")));
        c.pause_scenes(true).unwrap();
        assert!(batch.iter().all(|w| !c.work_is_current(w)));
        c.pause_scenes(false).unwrap();
        c.activate_from(&home, ActivationSource::Refresh)
            .await
            .unwrap();
        assert!(!c.snapshot().config.scene_settings.paused);
        let old = automatic(&c);
        c.invalidate_scene_work();
        assert!(c.activate_from(&home, old).await.is_err());
        c.activate_from(&home, automatic(&c)).await.unwrap();
        let batch = c.pop_batch();
        c.stop_scenes();
        assert!(batch.iter().all(|w| !c.work_is_current(w)));
    }
    #[test]
    fn scene_save_failure_and_page_deletion_are_atomic_and_keep_editor_boundaries() {
        let c = connected();
        let mut pages = c.snapshot().config.pages;
        let other = crate::devices::default_page("目标");
        pages.push(other.clone());
        c.update_pages(pages.clone()).unwrap();
        let settings = SceneSettings {
            enabled: true,
            fallback: SceneFallback::Page {
                page_id: other.id.clone(),
            },
            scenes: vec![Scene {
                id: "rule".into(),
                name: "场景".into(),
                enabled: true,
                page_id: other.id.clone(),
                trigger: SceneTrigger::Continuous,
                condition: SceneCondition::All { children: vec![] },
                enter_delay_ms: 0,
                exit_delay_ms: 0,
            }],
            ..Default::default()
        };
        c.save_scenes(settings.clone()).unwrap();
        let tmp = c.root.join("config.json.tmp");
        std::fs::create_dir(&tmp).unwrap();
        assert!(c.pause_scenes(true).is_err());
        assert!(!c.snapshot().config.scene_settings.paused);
        assert!(c.save_scenes(SceneSettings::default()).is_err());
        assert_eq!(c.snapshot().config.scene_settings, settings);
        std::fs::remove_dir(tmp).unwrap();
        c.update_pages(vec![pages[0].clone()]).unwrap();
        let s = c.snapshot().config.scene_settings;
        assert!(!s.scenes[0].enabled);
        assert_eq!(s.scenes[0].page_id, other.id);
        assert_eq!(s.fallback, SceneFallback::Keep);
        assert_eq!(storage::test_config(&c.root).unwrap().scene_settings, s);
    }
    #[tokio::test]
    async fn plugin_icon_frames_are_ephemeral_and_catalog_changes_cancel_prepared_batches() {
        use sha2::{Digest, Sha256};
        let c = connected();
        let dir = c.root.join("icon-source");
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 128]))
            .save(dir.join("icon.png"))
            .unwrap();
        let manifest: PluginManifest = serde_json::from_value(serde_json::json!({"id":"example.icons","name":"Icons","version":"1","api_version":1,"permissions":[],"icons":[{"id":"alpha","name":"Alpha","path":"icon.png"}]})).unwrap();
        c.icons
            .mount(&manifest.id, c.icons.prepare(&manifest, &dir).unwrap());
        let reference = PluginIconRef {
            plugin_id: manifest.id.clone(),
            icon_id: "alpha".into(),
        };
        let mut pages = c.snapshot().config.pages;
        pages[0].keys[0].plugin_icon = Some(reference);
        pages[0].keys[0].text.clear();
        pages[0].keys[0].background = "#0000FF".into();
        let id = pages[0].id.clone();
        c.update_pages(pages).unwrap();
        c.activate_from(&id, ActivationSource::Refresh)
            .await
            .unwrap();
        let batch = c.pop_batch();
        assert!(!batch.is_empty());
        let work = batch
            .iter()
            .find(|w| matches!(w.command, HardwareCommand::KeyData { key: 1, .. }))
            .unwrap();
        let png = work.image_frame.as_ref().unwrap();
        let pixel = image::load_from_memory(png)
            .unwrap()
            .to_rgb8()
            .get_pixel(0, 0)
            .0;
        assert!((i16::from(pixel[0]) - 128).abs() <= 1 && (i16::from(pixel[2]) - 127).abs() <= 1);
        assert!(!c
            .root
            .join("assets")
            .join(format!("{:x}.png", Sha256::digest(png.as_slice())))
            .exists());
        let original = std::fs::read(dir.join("icon.png")).unwrap();
        assert!(!c
            .root
            .join("assets")
            .join(format!("{:x}.png", Sha256::digest(&original)))
            .exists());
        assert!(batch.iter().all(|w| c.work_is_current(w)));
        let mut temporary = c.snapshot().config.pages[0].keys[0].clone();
        temporary.key = 2;
        temporary.background = "#00FF00".into();
        let epoch = c.inner.lock().unwrap().epoch;
        c.plugin_visual("another.plugin", temporary, epoch).unwrap();
        let queued_temporary = c.pop().unwrap();
        c.icons.unmount(&manifest.id);
        assert!(batch.iter().all(|w| !c.work_is_current(w)));
        assert!(!c.work_is_current(&queued_temporary));
        c.refresh_icons().await.unwrap();
        let temporary_png = {
            let i = c.inner.lock().unwrap();
            assert_eq!(i.icon_overrides.len(), 1);
            i.queue
                .iter()
                .find(|w| matches!(w.command, HardwareCommand::KeyData { key: 2, .. }))
                .unwrap()
                .image_frame
                .clone()
                .unwrap()
        };
        assert_eq!(
            image::load_from_memory(&temporary_png)
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [0, 255, 0]
        );
        let fallback = c.pop_batch();
        let png = fallback
            .iter()
            .find_map(|w| {
                matches!(w.command, HardwareCommand::KeyData { key: 1, .. })
                    .then_some(w.image_frame.as_ref())
                    .flatten()
            })
            .unwrap();
        assert_eq!(
            image::load_from_memory(png)
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [0, 0, 255]
        );
        let mut invalid = c.snapshot().config.pages;
        invalid[0].keys[0].asset_id = Some("a".repeat(64) + ".png");
        assert!(c.update_pages(invalid).is_err());
        std::fs::remove_dir_all(&c.root).unwrap();
    }
    fn connected() -> Arc<Core> {
        let c = Core::new_test_device(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()))
            .unwrap();
        {
            let mut i = c.inner.lock().unwrap();
            i.paths.insert("m18".into(), "path".into());
            i.snapshot.selected_device = Some("m18".into());
            i.snapshot.devices.push(DeviceDescriptor {
                adapter_id: crate::devices::default_adapter_id().into(),
                layout: crate::devices::default_layout().into(),
                id: "m18".into(),
                model: "M18".into(),
                serial: "a".into(),
                firmware: "v3".into(),
                vendor_id: 0x6603,
                product_id: 0x1012,
                capabilities: crate::devices::default_capabilities("v3"),
            });
        }
        c
    }
    #[test]
    fn autosave_preserves_current_page_settings_and_temporary_control() {
        let c = connected();
        let mut pages = c.snapshot().config.pages;
        let other = crate::devices::default_page("其他页面");
        pages.push(other.clone());
        assert!(c.update_pages(pages.clone()).unwrap().is_none());
        c.plugin_command("mcp", HardwareCommand::ScreenBrightness { value: 30 }, None)
            .unwrap();
        {
            let mut i = c.inner.lock().unwrap();
            i.snapshot.config.active_page = other.id.clone();
            i.snapshot.active_page = other.id.clone();
            i.snapshot.config.theme = "dark".into();
            i.snapshot.config.device_settings.screen_brightness = 45;
            i.snapshot
                .config
                .managed_plugin_ids
                .push("example.fixture".into());
        }
        pages[0].keys[0].text = "已修改".into();
        assert!(c.update_pages(pages.clone()).unwrap().is_none());
        let saved = storage::test_config(&c.root).unwrap();
        assert_eq!(saved.active_page, other.id);
        assert_eq!(saved.theme, "dark");
        assert_eq!(saved.device_settings.screen_brightness, 45);
        assert_eq!(saved.managed_plugin_ids, vec!["example.fixture"]);
        assert_eq!(saved.pages[0].keys[0].text, "已修改");
        assert!(!c.inner.lock().unwrap().overrides.is_empty());
        pages[1].keys[0].text = "当前页更改".into();
        assert_eq!(c.update_pages(pages.clone()).unwrap(), Some(other.id));
        pages.pop();
        assert_eq!(
            c.update_pages(pages.clone()).unwrap(),
            Some(pages[0].id.clone())
        );
    }
    #[test]
    fn autosave_does_not_publish_invalid_pages_or_failed_writes() {
        let c = connected();
        let original = serde_json::to_value(c.snapshot().config).unwrap();
        assert!(c.update_pages(vec![]).is_err());
        let mut pages = c.snapshot().config.pages;
        pages[0].keys[0].asset_id = Some("missing.png".into());
        assert!(c.update_pages(pages.clone()).is_err());
        pages[0].keys[0].asset_id = None;
        pages[0].name = "写入失败".into();
        let config_path = c.root.join("config.json");
        std::fs::remove_file(&config_path).unwrap();
        std::fs::create_dir(&config_path).unwrap();
        assert!(c.update_pages(pages).is_err());
        assert_eq!(serde_json::to_value(c.snapshot().config).unwrap(), original);
    }
    #[test]
    fn merges_latest_brightness() {
        let c = connected();
        c.enqueue(HardwareCommand::ScreenBrightness { value: 10 }, None)
            .unwrap();
        c.enqueue(HardwareCommand::ScreenBrightness { value: 40 }, None)
            .unwrap();
        assert_eq!(c.inner.lock().unwrap().queue.len(), 1);
        assert!(matches!(
            c.pop().unwrap().command,
            HardwareCommand::ScreenBrightness { value: 40 }
        ));
    }
    #[test]
    fn stopping_owner_invalidates_work_already_preparing_and_allows_new_work() {
        let c = connected();
        c.plugin_command("mcp", HardwareCommand::Refresh, None)
            .unwrap();
        let preparing = c.pop().unwrap();
        assert!(c.work_is_current(&preparing));
        c.cancel_owner("mcp");
        assert!(!c.work_is_current(&preparing));
        c.plugin_command("mcp", HardwareCommand::Refresh, None)
            .unwrap();
        assert!(c.work_is_current(&c.pop().unwrap()));
    }
    #[test]
    fn merged_led_commands_keep_the_latest_complete_output() {
        let c = connected();
        c.enqueue(
            HardwareCommand::LedColor {
                color: "#804020".into(),
            },
            None,
        )
        .unwrap();
        c.enqueue(HardwareCommand::LedBrightness { value: 128 }, None)
            .unwrap();
        assert_eq!(c.inner.lock().unwrap().queue.len(), 1);
        let work = c.pop().unwrap();
        assert!(matches!(
            work.command,
            HardwareCommand::LedBrightness { value: 128 }
        ));
        assert!(
            matches!(work.led_command, Some(HardwareCommand::LedColor { ref color }) if color == "#402010")
        );
        c.enqueue(HardwareCommand::ResetLed, None).unwrap();
        assert!(c
            .enqueue(HardwareCommand::LedBrightness { value: 100 }, None)
            .is_err());
        assert!(matches!(
            c.pop().unwrap().led_command,
            Some(HardwareCommand::ResetLed)
        ));
    }
    #[tokio::test]
    async fn default_effect_survives_page_activation() {
        let c = connected();
        {
            let mut i = c.inner.lock().unwrap();
            i.snapshot.config.device_settings.led_mode = LedMode::DeviceDefault;
        }
        c.activate(&c.snapshot().config.active_page).await.unwrap();
        let i = c.inner.lock().unwrap();
        assert!(i
            .queue
            .iter()
            .any(|w| matches!(w.led_command, Some(HardwareCommand::ResetLed))));
        assert!(!i.queue.iter().any(|w| matches!(
            w.command,
            HardwareCommand::LedBrightness { .. } | HardwareCommand::LedColor { .. }
        )));
    }
    #[tokio::test]
    async fn plugin_restore_applies_static_colour_before_level_over_default_effect() {
        let c = connected();
        c.inner
            .lock()
            .unwrap()
            .snapshot
            .config
            .device_settings
            .led_mode = LedMode::DeviceDefault;
        c.activate(&c.snapshot().config.active_page).await.unwrap();
        c.plugin_command(
            "colour",
            HardwareCommand::LedColor {
                color: "#FFFFFF".into(),
            },
            Some(1),
        )
        .unwrap();
        c.plugin_command(
            "level",
            HardwareCommand::LedBrightness { value: 40 },
            Some(1),
        )
        .unwrap();
        c.plugin_command(
            "screen",
            HardwareCommand::ScreenBrightness { value: 20 },
            Some(1),
        )
        .unwrap();
        c.restore_plugin("screen").await.unwrap();
        let i = c.inner.lock().unwrap();
        let led = i.queue.iter().find(|w| w.led_command.is_some()).unwrap();
        assert!(
            matches!(&led.led_command, Some(HardwareCommand::LedColor { color }) if color == "#282828")
        );
        assert!(i.overrides.values().any(|(id, _)| id == "level"));
        assert!(matches!(
            i.snapshot.config.device_settings.led_mode,
            LedMode::DeviceDefault
        ));
    }

    #[test]
    fn animation_frames_coalesce_in_one_slot_without_receipts() {
        let c = connected();
        let mut settings = c.snapshot().config.device_settings;
        settings.led_mode = LedMode::Rainbow;
        settings.led_speed = 7;
        c.configure_led_settings(settings).unwrap();
        assert_eq!(
            c.snapshot().config.device_settings.led_mode,
            LedMode::Rainbow
        );
        assert_eq!(c.snapshot().config.device_settings.led_speed, 7);
        let persisted = storage::test_config(&c.root).unwrap().device_settings;
        assert_eq!(persisted.led_mode, LedMode::Rainbow);
        assert_eq!(persisted.led_speed, 7);
        {
            let mut i = c.inner.lock().unwrap();
            i.led_frame = Some(vec![[1, 2, 3]; 24]);
            i.led_frame = Some(vec![[4, 5, 6]; 24]);
        }
        let work = c.pop().unwrap();
        assert!(work.receipt_id.is_none());
        assert!(matches!(
            &work.led_command,
            Some(HardwareCommand::LedColors { colors }) if *colors == vec![[4, 5, 6]; 24]
        ));
        assert!(c.pop().is_none());
        assert!(c
            .snapshot()
            .receipts
            .iter()
            .all(|receipt| receipt.command != "led_frame"));
    }

    #[test]
    fn disconnect_stops_frames_and_reconnect_resumes_saved_effect() {
        let c = connected();
        let mut settings = c.snapshot().config.device_settings;
        settings.led_mode = LedMode::Wave;
        c.configure_led_settings(settings).unwrap();
        c.tick_led_effect();
        assert!(c.inner.lock().unwrap().led_frame.is_some());
        let descriptor = c.snapshot().devices[0].clone();
        c.clear_connection();
        c.tick_led_effect();
        assert!(c.inner.lock().unwrap().led_frame.is_none());
        {
            let mut i = c.inner.lock().unwrap();
            i.paths.insert(descriptor.id.clone(), "path".into());
            i.snapshot.selected_device = Some(descriptor.id.clone());
            i.snapshot.devices.push(descriptor);
            i.snapshot.connection = "ready".into();
            i.led = i.snapshot.config.device_settings.clone();
            i.led_effect_started_at = Instant::now();
        }
        c.tick_led_effect();
        assert!(c.inner.lock().unwrap().led_frame.is_some());
    }

    #[test]
    fn preview_led_settings_do_not_modify_saved_configuration() {
        let c = connected();
        let saved = c.snapshot().config.device_settings;
        let mut preview = saved.clone();
        preview.led_mode = LedMode::Comet;
        preview.led_speed = 10;
        c.preview_led_settings(preview).unwrap();
        assert_eq!(c.snapshot().config.device_settings.led_mode, saved.led_mode);
        assert_eq!(
            c.snapshot().config.device_settings.led_speed,
            saved.led_speed
        );
        assert_eq!(
            storage::test_config(&c.root)
                .unwrap()
                .device_settings
                .led_mode,
            saved.led_mode
        );
        assert_eq!(c.inner.lock().unwrap().led.led_mode, LedMode::Comet);
    }

    #[tokio::test]
    async fn plugin_led_override_pauses_animation_and_restore_resumes_it() {
        let c = connected();
        let mut settings = c.snapshot().config.device_settings;
        settings.led_mode = LedMode::Comet;
        c.configure_led_settings(settings).unwrap();
        c.plugin_command(
            "plugin-level",
            HardwareCommand::LedBrightness { value: 80 },
            None,
        )
        .unwrap();
        c.tick_led_effect();
        assert!(c.inner.lock().unwrap().led_frame.is_none());
        c.restore_plugin("plugin-level").await.unwrap();
        c.tick_led_effect();
        assert!(c.inner.lock().unwrap().led_frame.is_some());
        c.plugin_command(
            "plugin-color",
            HardwareCommand::LedColor {
                color: "#00FF00".into(),
            },
            None,
        )
        .unwrap();
        c.tick_led_effect();
        assert!(c.inner.lock().unwrap().led_frame.is_none());
        c.restore_plugin("plugin-color").await.unwrap();
        c.tick_led_effect();
        let i = c.inner.lock().unwrap();
        assert!(i.led.led_mode.is_animated());
        assert!(i.led_frame.is_some());
        assert!(!i.overrides.contains_key("led_color"));
        assert!(!i
            .queue
            .iter()
            .any(|work| work.command.merge_key() == Some("led_state")));
    }

    #[tokio::test]
    async fn clean_shutdown_sends_default_effect_and_waits_for_acknowledgement() {
        let c = connected();
        let mut settings = c.snapshot().config.device_settings;
        settings.led_mode = LedMode::Breathing;
        c.configure_led_settings(settings).unwrap();
        let core = c.clone();
        let shutdown = tokio::spawn(async move { core.reset_led_for_shutdown().await });
        tokio::task::yield_now().await;
        let work = c.pop().unwrap();
        assert!(matches!(work.command, HardwareCommand::ResetLed));
        let receipt_id = work.receipt_id.unwrap();
        c.tick_led_effect();
        assert!(c.inner.lock().unwrap().led_frame.is_none());
        c.receipt(&receipt_id, "acknowledged", "test acknowledgement");
        shutdown.await.unwrap().unwrap();
        assert!(c.inner.lock().unwrap().led_shutdown);
        assert!(matches!(
            c.snapshot().config.device_settings.led_mode,
            LedMode::Breathing
        ));
    }

    #[test]
    fn rejects_stale_page_jobs() {
        let c = connected();
        assert!(c.enqueue(HardwareCommand::Refresh, Some(1)).is_err());
        c.enqueue(HardwareCommand::Refresh, Some(0)).unwrap();
        c.inner.lock().unwrap().epoch = 1;
        assert!(c.pop().is_none());
    }

    #[test]
    fn disconnect_cancels_queued_commands_and_expires_paths() {
        let c = connected();
        let id = c.enqueue(HardwareCommand::Refresh, Some(0)).unwrap();
        c.clear_connection();
        assert!(c.pop().is_none());
        assert!(c.inner.lock().unwrap().paths.is_empty());
        assert!(c.snapshot().pressed.is_empty());
        assert_eq!(
            c.snapshot()
                .receipts
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .status,
            "cancelled"
        );
        assert!(c.enqueue(HardwareCommand::Refresh, None).is_err());
    }
    #[tokio::test]
    async fn stopping_one_plugin_restores_other_plugin_overrides() {
        let c = connected();
        c.plugin_command(
            "a",
            HardwareCommand::LedColor {
                color: "#FFFFFF".into(),
            },
            Some(0),
        )
        .unwrap();
        c.plugin_command(
            "b",
            HardwareCommand::ScreenBrightness { value: 40 },
            Some(0),
        )
        .unwrap();
        c.restore_plugin("a").await.unwrap();
        let i = c.inner.lock().unwrap();
        assert!(i.overrides.values().all(|(owner, _)| owner != "a"));
        assert!(i.overrides.values().any(|(owner, _)| owner == "b"));
        assert_eq!(i.snapshot.config.device_settings.screen_brightness, 70);
    }

    #[tokio::test]
    async fn page_notification_follows_baseline_queue() {
        let c = connected();
        let mut events = c.events.subscribe();
        let observer = c.clone();
        let task = tokio::spawn(async move {
            loop {
                if events.recv().await.unwrap().kind == "page.changed" {
                    let epoch = observer.inner.lock().unwrap().epoch;
                    observer
                        .plugin_command(
                            "listener",
                            HardwareCommand::LedColor {
                                color: "#123456".into(),
                            },
                            Some(epoch),
                        )
                        .unwrap();
                    break;
                }
            }
        });
        let id = c.snapshot().config.active_page;
        c.activate(&id).await.unwrap();
        task.await.unwrap();
        assert!(
            matches!(&c.inner.lock().unwrap().queue.back().unwrap().command,HardwareCommand::LedColor{color} if color=="#123456")
        );
    }
    #[test]
    fn bulk_overrides_are_stored_per_key() {
        let c = connected();
        let p = c.root.join("pixel.png");
        image::RgbImage::from_pixel(1, 1, image::Rgb([1, 2, 3]))
            .save(&p)
            .unwrap();
        let id = storage::import(&c.root, &p).unwrap();
        c.plugin_command(
            "bulk",
            HardwareCommand::BulkImage {
                keys: vec![1, 2],
                asset_id: id,
            },
            Some(0),
        )
        .unwrap();
        let i = c.inner.lock().unwrap();
        assert!(matches!(
            &i.overrides["key:2"].1,
            HardwareCommand::KeyImage { key: 2, .. }
        ));
    }

    #[tokio::test]
    async fn prepared_page_is_one_batch_and_does_not_absorb_plugin_updates() {
        let c = connected();
        c.activate(&c.snapshot().config.active_page).await.unwrap();
        let asset = c
            .inner
            .lock()
            .unwrap()
            .queue
            .iter()
            .find_map(|work| match &work.command {
                HardwareCommand::KeyImage { asset_id, .. } => Some(asset_id.clone()),
                _ => None,
            })
            .unwrap();
        let epoch = c.inner.lock().unwrap().epoch;
        c.plugin_command(
            "plugin",
            HardwareCommand::KeyImage {
                key: 1,
                asset_id: asset,
            },
            Some(epoch),
        )
        .unwrap();
        let page = c.pop_batch();
        assert!(page.len() > 30);
        assert_eq!(
            page.iter()
                .filter(|w| matches!(w.command, HardwareCommand::KeyImage { .. }))
                .count(),
            15
        );
        assert!(page
            .iter()
            .all(|w| w.page_batch == Some(epoch) && w.source.is_none()));
        assert!(matches!(
            page.first().unwrap().command,
            HardwareCommand::StopGif
        ));
        assert_eq!(
            page.iter()
                .filter(|w| matches!(w.command, HardwareCommand::Refresh))
                .count(),
            1
        );
        let plugin = c.pop_batch();
        assert_eq!(plugin.len(), 1);
        assert_eq!(plugin[0].source.as_deref(), Some("plugin"));
        assert_eq!(plugin[0].page_batch, None);
    }

    #[tokio::test]
    async fn retained_gif_only_plays_in_pattern_mode() {
        let c = connected();
        let mut gif = Vec::new();
        image::codecs::gif::GifEncoder::new(&mut gif)
            .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                64,
                64,
                image::Rgba([255, 0, 0, 255]),
            )))
            .unwrap();
        let asset = storage::put_asset(&c.root, &gif).unwrap();
        for mode in [
            KeyContentMode::Pattern,
            KeyContentMode::Text,
            KeyContentMode::None,
            KeyContentMode::Pattern,
        ] {
            let mut pages = c.snapshot().config.pages;
            pages[0].keys[0].asset_id = Some(asset.clone());
            pages[0].keys[0].content_mode = Some(mode);
            c.update_pages(pages).unwrap();
            c.activate(&c.snapshot().config.active_page).await.unwrap();
            let batch = c.pop_batch();
            assert_eq!(
                batch
                    .iter()
                    .any(|w| matches!(w.command, HardwareCommand::KeyGif { key: 1, .. })),
                mode == KeyContentMode::Pattern
            );
            assert_eq!(
                batch
                    .iter()
                    .any(|w| matches!(w.command, HardwareCommand::StartGif)),
                mode == KeyContentMode::Pattern
            );
            assert_eq!(
                batch
                    .iter()
                    .filter(|w| matches!(w.command, HardwareCommand::Refresh))
                    .count(),
                1
            );
            assert_eq!(
                c.snapshot().config.pages[0].keys[0].asset_id.as_deref(),
                Some(asset.as_str())
            );
        }
    }

    #[tokio::test]
    async fn full_queue_rejects_the_entire_page_without_partial_updates() {
        let c = connected();
        let settings = c.snapshot().config.device_settings;
        let too_many = vec![HardwareCommand::Refresh; alchemy_device_api::MAX_PAGE_OPERATIONS + 1];
        assert!(c.enqueue_page_frame(too_many, 0, settings.clone()).is_err());
        assert!(c.inner.lock().unwrap().queue.is_empty());
        let megabyte = Arc::new(vec![0; 1024 * 1024]);
        let images = PageImages {
            frames: (1..=65).map(|key| (key, megabyte.clone())).collect(),
            revision: None,
        };
        assert!(c
            .enqueue_page_frame_owned(
                vec![HardwareCommand::Refresh],
                0,
                settings,
                None,
                None,
                images
            )
            .is_err());
        assert!(c.inner.lock().unwrap().queue.is_empty());
        for _ in 0..4096 {
            c.enqueue(HardwareCommand::Firmware, None).unwrap();
        }
        assert!(c.activate(&c.snapshot().config.active_page).await.is_err());
        let i = c.inner.lock().unwrap();
        assert_eq!(i.queue.len(), 4096);
        assert!(i
            .queue
            .iter()
            .all(|w| matches!(w.command, HardwareCommand::Firmware)));
    }

    #[test]
    fn completed_render_of_an_old_page_does_not_change_the_new_queue_or_led_state() {
        let c = connected();
        let settings = c.snapshot().config.device_settings;
        c.inner.lock().unwrap().epoch = 1;
        assert!(!c
            .enqueue_page_frame(vec![HardwareCommand::Firmware], 0, settings)
            .unwrap());
        assert!(c.inner.lock().unwrap().queue.is_empty());
    }
}
