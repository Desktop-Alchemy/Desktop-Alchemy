//! Device registry and independent per-device runtimes. No transport handles live here.
use crate::{core::Core, model::*, storage};
use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;

#[cfg(feature = "device-ms-audio-dock")]
pub use alchemy_audio_dock as ms_audio_dock;
pub use alchemy_device_api::{AdapterFuture, DeviceAdapter, DeviceSession};
#[cfg(feature = "device-m18")]
pub use alchemy_m18 as m18;
#[cfg(feature = "device-m18")]
#[path = "m18/diagnostic.rs"]
pub mod diagnostic;
mod host;
pub fn default_adapter_id() -> &'static str {
    #[cfg(feature = "device-m18")]
    {
        m18::ID
    }
    #[cfg(not(feature = "device-m18"))]
    {
        ""
    }
}
pub fn default_layout() -> DeviceLayout {
    adapter_layout(default_adapter_id())
}
pub fn default_config() -> AppConfig {
    built_in_adapters()
        .get(default_adapter_id())
        .map(|adapter| adapter.default_config())
        .unwrap_or_else(AppConfig::empty)
}
pub fn built_in_adapters() -> BTreeMap<String, Arc<dyn DeviceAdapter>> {
    #[allow(unused_mut)]
    let mut adapters: BTreeMap<String, Arc<dyn DeviceAdapter>> = BTreeMap::new();
    #[cfg(feature = "device-m18")]
    adapters.insert(m18::ID.into(), Arc::new(m18::Adapter));
    #[cfg(feature = "device-ms-audio-dock")]
    adapters.insert(
        ms_audio_dock::ID.into(),
        Arc::new(ms_audio_dock::Adapter::default()),
    );
    #[cfg(feature = "device-simulator")]
    adapters.insert(
        alchemy_device_simulator::ID.into(),
        Arc::new(alchemy_device_simulator::Adapter),
    );
    adapters
}
pub fn definition(id: &str) -> DeviceDefinition {
    built_in_adapters()
        .get(id)
        .map(|adapter| adapter.definition().normalized())
        .unwrap_or_else(|| DeviceDefinition {
            inputs: vec![],
            displays: vec![],
            adapter_id: id.into(),
            name: id.into(),
            layout: DeviceLayout::default(),
            supports_pages: false,
            input_names: Default::default(),
        })
}
pub fn adapter_layout(id: &str) -> DeviceLayout {
    definition(id).layout
}
pub fn default_page(name: &str) -> Page {
    #[cfg(feature = "device-m18")]
    {
        m18::new_page(name)
    }
    #[cfg(not(feature = "device-m18"))]
    {
        Page {
            bindings: vec![],
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            keys: vec![],
            background_asset: None,
        }
    }
}
pub fn default_capabilities(firmware: &str) -> DeviceCapabilities {
    #[cfg(feature = "device-m18")]
    {
        m18::capabilities(firmware)
    }
    #[cfg(not(feature = "device-m18"))]
    {
        let _ = firmware;
        Default::default()
    }
}

pub struct Runtime {
    pub core: Arc<Core>,
    pub scenes: Arc<crate::scenes::SceneManager>,
}
pub struct DeviceManager {
    pub host: Arc<Core>,
    adapters: BTreeMap<String, Arc<dyn DeviceAdapter>>,
    runtimes: Mutex<BTreeMap<String, Arc<Runtime>>>,
    discovered: Mutex<BTreeMap<String, DeviceDescriptor>>,
    retries: Mutex<Vec<String>>,
    discovery_gate: Mutex<()>,
    collectors: Arc<crate::scenes::SharedCollectors>,
    started: std::sync::atomic::AtomicBool,
    cancel: tokio_util::sync::CancellationToken,
    tasks: Mutex<Vec<(String, tokio::task::JoinHandle<()>)>>,
    resource: Mutex<Option<PathBuf>>,
    adapter_cancels: Mutex<BTreeMap<String, tokio_util::sync::CancellationToken>>,
    #[cfg(windows)]
    raw: Mutex<Option<crate::raw_input::RawInputService>>,
    pub changed: broadcast::Sender<()>,
}
impl DeviceManager {
    pub fn adapter_host(self: &Arc<Self>, adapter_id: &str) -> alchemy_device_api::AdapterHost {
        alchemy_device_api::AdapterHost {
            api: Arc::new(host::Bridge::new(Arc::downgrade(self), adapter_id.into())),
            root: self.host.root.clone(),
            resource: self.resource.lock().unwrap().clone(),
            cancel: self
                .adapter_cancels
                .lock()
                .unwrap()
                .entry(adapter_id.into())
                .or_insert_with(|| self.cancel.child_token())
                .clone(),
        }
    }
    pub async fn module_request(
        self: &Arc<Self>,
        device_id: &str,
        operation: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let core = self.device(device_id)?;
        let adapter = self
            .adapters
            .get(&core.adapter_id)
            .context(alchemy_device_api::UiFailure::new("backend.b288", "设备模块未启用"))?
            .clone();
        if !adapter.operations().contains(&operation.as_str()) {
            return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::Unsupported, "backend.e016", "模块操作不支持")
            .into());
        }
        let connection = host::connection(&core).context(alchemy_device_api::UiFailure::new("backend.b290", "设备已离线"))?;
        let context = self.adapter_host(&core.adapter_id);
        adapter
            .request(context, connection, operation, payload)
            .await
    }
    pub fn adapter_definition(&self, id: &str) -> Result<DeviceDefinition> {
        Ok(self
            .adapters
            .get(id)
            .context(alchemy_device_api::UiFailure::new("backend.b288", "设备模块未启用"))?
            .definition())
    }
    pub fn new(host: Arc<Core>) -> Result<Arc<Self>> {
        let adapters = built_in_adapters();
        Self::with_adapters(host, adapters)
    }
    pub fn with_adapters(
        host: Arc<Core>,
        adapters: BTreeMap<String, Arc<dyn DeviceAdapter>>,
    ) -> Result<Arc<Self>> {
        for (id, adapter) in &adapters {
            if id != adapter.id() {
                bail!(alchemy_device_api::UiFailure::new("backend.b291", "适配器注册标识不一致"))
            };
            storage::validate_definition(&adapter.definition().normalized())?;
        }
        storage::initialize_workspace(&host.root)?;
        let mut workspace = storage::load_workspace(&host.root)?;
        let original_workspace = workspace.clone();
        let mut migrated = false;
        for profile in &mut workspace.profiles {
            if let Some(adapter) = adapters.get(&profile.adapter_id) {
                if profile.config_error.is_none()
                    && profile.config_version == 1
                    && profile.schema_version < adapter.config_version()
                {
                    let migration = || -> Result<serde_json::Value> {
                        let mut value = profile.module_data.clone();
                        for version in profile.schema_version..adapter.config_version() {
                            value = adapter.migrate_module(version, value)?;
                        }
                        Ok(value)
                    };
                    if let Ok(data) = migration().and_then(|data| {
                        adapter.validate_module(&data)?;
                        for id in adapter.asset_references(&data)? {
                            storage::asset(&host.root, &id)?;
                        }
                        host.validate_config_assets(&profile.config)?;
                        adapter.validate(&profile.config)?;
                        storage::validate_device_config(
                            &profile.config,
                            &adapter.definition().normalized(),
                        )?;
                        Ok(data)
                    }) {
                        profile.module_data = data;
                        profile.schema_version = adapter.config_version();
                        migrated = true;
                    }
                }
            }
        }
        if migrated {
            if let Err(error) = storage::save_workspace(&host.root, &workspace) {
                host.log(
                    "error",
                    format!("模块配置迁移未保存，原配置已保留：{error}"),
                );
                workspace = original_workspace;
            }
        }
        let (changed, _) = broadcast::channel(32);
        let manager = Arc::new(Self {
            host: host.clone(),
            adapters,
            runtimes: Mutex::new(BTreeMap::new()),
            discovered: Mutex::new(BTreeMap::new()),
            retries: Mutex::new(vec![]),
            discovery_gate: Mutex::new(()),
            collectors: crate::scenes::SharedCollectors::new(),
            started: Default::default(),
            cancel: Default::default(),
            tasks: Mutex::new(vec![]),
            resource: Mutex::new(None),
            adapter_cancels: Default::default(),
            #[cfg(windows)]
            raw: Mutex::new(None),
            changed,
        });
        let _ = host.manager.set(Arc::downgrade(&manager));
        for profile in &workspace.profiles {
            manager.add_runtime(profile)?;
        }
        Ok(manager)
    }
    fn add_runtime(self: &Arc<Self>, profile: &DeviceProfile) -> Result<()> {
        let definition = self
            .adapters
            .get(&profile.adapter_id)
            .map(|a| a.definition().normalized())
            .unwrap_or_else(|| definition(&profile.adapter_id));
        let error = match self.adapters.get(&profile.adapter_id) {
            None => Some("设备模块未启用".to_owned()),
            Some(adapter) => profile.config_error.clone().or_else(|| {
                if profile.config_version != 1 {
                    Some("公共配置版本不支持".into())
                } else if profile.schema_version != adapter.config_version() {
                    Some("模块配置版本不支持".into())
                } else {
                    storage::validate_device_config(&profile.config, &definition)
                        .and_then(|_| adapter.validate(&profile.config))
                        .and_then(|_| adapter.validate_module(&profile.module_data))
                        .and_then(|_| self.host.validate_config_assets(&profile.config))
                        .and_then(|_| {
                            for id in adapter.asset_references(&profile.module_data)? {
                                storage::asset(&self.host.root, &id)?;
                            }
                            Ok(())
                        })
                        .err()
                        .map(|e| e.to_string())
                }
            }),
        };
        let mut core = Core::for_profile(&self.host, profile, definition)?;
        Arc::get_mut(&mut core).unwrap().profile_error = error;
        if let Some(adapter) = self.adapters.get(&profile.adapter_id) {
            let _ = core.adapter.set(adapter.clone());
        }
        {
            let host = self.host.snapshot();
            let mut inner = core.inner.lock().unwrap();
            inner.snapshot.plugins = host.plugins;
            inner.snapshot.config.disabled_plugins = host.config.disabled_plugins;
        }
        core.inner
            .lock()
            .unwrap()
            .widgets
            .seed_from(&self.host.inner.lock().unwrap().widgets);
        let _ = core.manager.set(Arc::downgrade(self));
        let scenes = crate::scenes::SceneManager::new(core.clone());
        scenes.use_collectors(self.collectors.clone());
        let runtime = Arc::new(Runtime {
            core: core.clone(),
            scenes,
        });
        self.runtimes
            .lock()
            .unwrap()
            .insert(profile.id.clone(), runtime.clone());
        Ok(())
    }
    pub fn start(self: &Arc<Self>, resource: Option<PathBuf>, hardware: bool) {
        if self.started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        *self.resource.lock().unwrap() = resource.clone();
        self.collectors.start(self.clone());
        for runtime in self.runtimes() {
            self.watch(runtime);
        }
        if hardware {
            #[cfg(windows)]
            {
                let specs = self
                    .adapters
                    .values()
                    .flat_map(|adapter| adapter.raw_input())
                    .collect::<Vec<_>>();
                if !specs.is_empty() {
                    match crate::raw_input::RawInputService::start(self.clone(), specs) {
                        Ok(service) => *self.raw.lock().unwrap() = Some(service),
                        Err(error) => self
                            .host
                            .log("error", format!("输入监听启动失败：{error:#}")),
                    }
                }
            }
            for adapter in self.adapters.values() {
                self.start_adapter(adapter.clone());
            }
        }
    }
    fn start_adapter(self: &Arc<Self>, adapter: Arc<dyn DeviceAdapter>) {
        let context = self.adapter_host(adapter.id());
        let status = context.clone();
        let id = adapter.id().to_owned();
        // Setup runs on the UI thread, outside an entered Tokio runtime.
        let task = tauri::async_runtime::handle().inner().spawn(async move {
            let result = tokio::spawn(async move { adapter.run(context).await }).await;
            if !status.cancel.is_cancelled() {
                let error = match result {
                    Ok(Err(error)) => error.to_string(),
                    Err(error) => format!("设备后端异常退出：{error}"),
                    Ok(Ok(())) => "设备后端提前退出".into(),
                };
                status.cancel.cancel();
                status.api.backend_status("error", Some(error));
            }
        });
        self.tasks.lock().unwrap().push((id, task));
    }
    fn watch(self: &Arc<Self>, runtime: Arc<Runtime>) {
        if runtime.core.profile_error.is_none() {
            runtime.scenes.start();
        }
        let manager = self.clone();
        let mut changes = runtime.core.changed.subscribe();
        let mut events = runtime.core.events.subscribe();
        let mut scene_changes = runtime.scenes.changed.subscribe();
        let mut stop = self.host.stop.subscribe();
        let mut errors = runtime.core.action_errors.subscribe();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.changed() => break,
                    _ = manager.cancel.cancelled()=>break,
                    event = events.recv() => { if let Ok(event) = event { let _ = manager.host.events.send(event); } },
                    error = errors.recv() => { if let Ok(error) = error { let _ = manager.host.action_errors.send(error); } },
                    _ = scene_changes.recv() => { let _ = manager.changed.send(()); },
                    _ = changes.recv() => { let _ = manager.changed.send(()); },
                }
            }
        });
    }
    /// Fair scheduling by complete batch, scoped to the owning adapter.
    pub fn next_batch(
        &self,
        adapter_id: &str,
        cursor: &mut usize,
    ) -> Option<(Arc<Core>, Vec<crate::core::Work>)> {
        let candidates = self.runtimes_for(adapter_id);
        for offset in 0..candidates.len() {
            let index = (*cursor + offset) % candidates.len();
            let core = candidates[index].core.clone();
            let batch = core.pop_batch();
            if !batch.is_empty() {
                *cursor = (index + 1) % candidates.len();
                return Some((core, batch));
            }
        }
        None
    }
    pub fn runtimes_for(&self, adapter_id: &str) -> Vec<Arc<Runtime>> {
        self.runtimes()
            .into_iter()
            .filter(|runtime| runtime.core.adapter_id == adapter_id)
            .collect()
    }
    pub fn runtimes(&self) -> Vec<Arc<Runtime>> {
        self.runtimes.lock().unwrap().values().cloned().collect()
    }
    pub fn profile(&self, id: &str) -> Result<Arc<Core>> {
        self.runtimes
            .lock()
            .unwrap()
            .get(id)
            .map(|r| r.core.clone())
            .context(alchemy_device_api::UiFailure::new("backend.b041", "未知设备配置"))
    }
    pub fn scene_manager(&self, profile_id: &str) -> Result<Arc<crate::scenes::SceneManager>> {
        self.runtimes
            .lock()
            .unwrap()
            .get(profile_id)
            .map(|r| r.scenes.clone())
            .context(alchemy_device_api::UiFailure::new("backend.b041", "未知设备配置"))
    }
    pub fn device(&self, id: &str) -> Result<Arc<Core>> {
        let workspace = storage::load_workspace(&self.host.root)?;
        let profile = workspace
            .profiles
            .iter()
            .find(|p| p.device_id.as_deref() == Some(id))
            .ok_or_else(|| {
                alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::NotFound, "backend.e017", "未知设备")
            })?;
        if !self.adapters.contains_key(&profile.adapter_id) {
            return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::Unsupported, "backend.e018", "设备模块未启用")
            .into());
        }
        let core = self.profile(&profile.id)?;
        if let Some(error) = &core.profile_error {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ConfigUnavailable,
                error,
            )
            .into());
        }
        if !core.inner.lock().unwrap().paths.contains_key(id) {
            return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::Offline, "backend.e019", "设备已离线")
            .into());
        }
        Ok(core)
    }
    pub fn device_profile(&self, id: &str) -> Result<Arc<Core>> {
        let workspace = storage::load_workspace(&self.host.root)?;
        let profile = workspace
            .profiles
            .iter()
            .find(|p| p.device_id.as_deref() == Some(id))
            .ok_or_else(|| {
                alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::NotFound, "backend.e017", "未知设备")
            })?;
        self.profile(&profile.id)
    }
    /// Register the complete discovery set before opening/binding any device.
    pub fn discovered_for(
        self: &Arc<Self>,
        adapter_id: &str,
        devices: Vec<DeviceDescriptor>,
    ) -> Result<()> {
        let _gate = self.discovery_gate.lock().unwrap();
        if !self.adapters.contains_key(adapter_id) {
            return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::Unsupported, "backend.e018", "设备模块未启用")
            .into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for device in &devices {
            if device.adapter_id != adapter_id || device.id.is_empty() || !ids.insert(&device.id) {
                bail!(alchemy_device_api::UiFailure::new("backend.b292", "设备身份重复或模块归属无效"))
            }
        }
        let mut all = self.discovered.lock().unwrap().clone();
        all.retain(|_, device| device.adapter_id != adapter_id);
        for device in devices {
            if all.contains_key(&device.id) {
                bail!(alchemy_device_api::UiFailure::new("backend.b293", "跨模块设备身份冲突"))
            }
            all.insert(device.id.clone(), device);
        }
        self.reconcile_discovered(all.into_values().collect())
    }
    pub fn discovered(self: &Arc<Self>, devices: Vec<DeviceDescriptor>) -> Result<()> {
        let _gate = self.discovery_gate.lock().unwrap();
        self.reconcile_discovered(devices)
    }
    fn reconcile_discovered(self: &Arc<Self>, devices: Vec<DeviceDescriptor>) -> Result<()> {
        let next: BTreeMap<_, _> = devices.iter().map(|d| (d.id.clone(), d.clone())).collect();
        if next.len() != devices.len() {
            bail!(alchemy_device_api::UiFailure::new("backend.b294", "设备身份重复，无法安全绑定"))
        }
        let discovery_changed = serde_json::to_value(&*self.discovered.lock().unwrap())?
            != serde_json::to_value(&next)?;
        let mut profiles_changed = false;
        storage::edit_workspace(&self.host.root, |workspace| {
            for adapter_id in self.adapters.keys() {
                let unbound: Vec<_> = workspace
                    .profiles
                    .iter()
                    .filter(|p| p.adapter_id == *adapter_id && p.device_id.is_none())
                    .map(|p| p.id.clone())
                    .collect();
                let unmatched: Vec<_> = devices
                    .iter()
                    .filter(|d| {
                        d.adapter_id == *adapter_id
                            && !workspace
                                .profiles
                                .iter()
                                .any(|p| p.device_id.as_deref() == Some(&d.id))
                    })
                    .collect();
                if !unbound.is_empty() {
                    if unbound.len() == 1 && unmatched.len() == 1 {
                        workspace
                            .profiles
                            .iter_mut()
                            .find(|p| p.id == unbound[0])
                            .unwrap()
                            .device_id = Some(unmatched[0].id.clone());
                        profiles_changed = true;
                    }
                    // Unbound profiles require explicit binding when discovery is ambiguous.
                    continue;
                }
                for device in unmatched {
                    let adapter = &self.adapters[adapter_id];
                    let config = adapter.default_config();
                    let module_data = adapter.default_module_config();
                    storage::validate_device_config(&config, &adapter.definition().normalized())?;
                    adapter.validate(&config)?;
                    adapter.validate_module(&module_data)?;
                    self.host.validate_config_assets(&config)?;
                    for id in adapter.asset_references(&module_data)? {
                        storage::asset(&self.host.root, &id)?;
                    }
                    profiles_changed = true;
                    workspace.profiles.push(DeviceProfile {
                        config_version: 1,
                        module_data,
                        raw: serde_json::Value::Null,
                        config_error: None,
                        id: uuid::Uuid::new_v4().to_string(),
                        adapter_id: adapter_id.clone(),
                        schema_version: adapter.config_version(),
                        name: device.model.clone(),
                        device_id: Some(device.id.clone()),
                        config,
                    });
                }
            }
            Ok(())
        })?;
        *self.discovered.lock().unwrap() = next;
        let workspace = storage::load_workspace(&self.host.root)?;
        for profile in &workspace.profiles {
            if !self.runtimes.lock().unwrap().contains_key(&profile.id) {
                self.add_runtime(profile)?;
                let runtime = self.runtimes.lock().unwrap()[&profile.id].clone();
                if self.started.load(std::sync::atomic::Ordering::SeqCst) {
                    self.watch(runtime);
                }
            }
        }
        if discovery_changed || profiles_changed {
            let _ = self.changed.send(());
        }
        Ok(())
    }
    pub fn bind(self: &Arc<Self>, profile_id: &str, device_id: &str) -> Result<()> {
        let device = self
            .discovered
            .lock()
            .unwrap()
            .get(device_id)
            .cloned()
            .context(alchemy_device_api::UiFailure::new("backend.b286", "设备未连接"))?;
        storage::edit_workspace(&self.host.root, |workspace| {
            if workspace
                .profiles
                .iter()
                .any(|p| p.device_id.as_deref() == Some(device_id))
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b295", "设备已经绑定配置"))
            }
            let profile = workspace
                .profiles
                .iter_mut()
                .find(|p| p.id == profile_id)
                .context(alchemy_device_api::UiFailure::new("backend.b041", "未知设备配置"))?;
            if profile.device_id.is_some() || profile.adapter_id != device.adapter_id {
                bail!(alchemy_device_api::UiFailure::new("backend.b296", "设备与配置不匹配"))
            }
            profile.device_id = Some(device_id.into());
            Ok(())
        })?;

        let _ = self.changed.send(());
        Ok(())
    }
    pub fn retry(self: &Arc<Self>, device_id: &str) -> Result<()> {
        let core = self.device_profile(device_id)?;
        if !self.adapters.contains_key(&core.adapter_id) {
            return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::Unsupported, "backend.e018", "设备模块未启用")
            .into());
        }
        let restart = {
            let mut tasks = self.tasks.lock().unwrap();
            let finished = tasks
                .iter()
                .any(|(id, task)| id == &core.adapter_id && task.is_finished());
            if finished {
                tasks.retain(|(id, task)| id != &core.adapter_id || !task.is_finished());
            }
            finished
        };
        if restart {
            self.adapter_cancels
                .lock()
                .unwrap()
                .insert(core.adapter_id.clone(), self.cancel.child_token());
            self.start_adapter(self.adapters[&core.adapter_id].clone());
        }
        let mut retries = self.retries.lock().unwrap();
        if !retries.iter().any(|id| id == device_id) {
            retries.push(device_id.into());
        }
        #[cfg(windows)]
        if let Some(raw) = self.raw.lock().unwrap().as_ref() {
            raw.wake();
        }
        Ok(())
    }
    pub fn take_retries_for(&self, adapter_id: &str) -> Vec<String> {
        let Ok(workspace) = storage::load_workspace(&self.host.root) else {
            return vec![];
        };
        let mut retries = self.retries.lock().unwrap();
        let mut mine = vec![];
        retries.retain(|id| {
            if workspace
                .profiles
                .iter()
                .any(|p| p.adapter_id == adapter_id && p.device_id.as_ref() == Some(id))
            {
                mine.push(id.clone());
                false
            } else {
                true
            }
        });
        mine
    }
    pub fn snapshot(&self) -> Result<WorkspaceSnapshot> {
        let host = self.host.snapshot();
        let workspace = storage::load_workspace(&self.host.root)?;
        let mut snapshot = WorkspaceSnapshot {
            global: workspace.global,
            discovered: self.discovered.lock().unwrap().values().cloned().collect(),
            plugins: host.plugins.clone(),
            logs: host.logs,
            devices: workspace
                .profiles
                .iter()
                .filter_map(|profile| {
                    let runtime = self.runtimes.lock().unwrap().get(&profile.id).cloned()?;
                    let mut state = runtime.core.snapshot();
                    state.plugins = host.plugins.clone();
                    state.config.disabled_plugins = host.config.disabled_plugins.clone();
                    state.config.theme = host.config.theme.clone();
                    let module_state = runtime.core.inner.lock().unwrap().module_state.clone();
                    Some(DeviceState { ui_message: runtime.core.profile_error.as_ref().map(|_| UiMessage::new("errors.configurationFailed")),
                        module_state,
                        config_error: runtime.core.profile_error.clone(),
                        module_available: self.adapters.contains_key(&profile.adapter_id),
                        profile_id: profile.id.clone(),
                        adapter_id: profile.adapter_id.clone(),
                        name: profile.name.clone(),
                        device_id: profile.device_id.clone(),
                        layout: runtime.core.layout.clone(),
                        definition: runtime.core.definition(),
                        state,
                        scene_status: runtime.scenes.status(),
                    })
                })
                .collect(),
        };
        for descriptor in &mut snapshot.discovered {
            if let Some(enriched) = snapshot
                .devices
                .iter()
                .flat_map(|d| &d.state.devices)
                .find(|d| d.id == descriptor.id)
            {
                descriptor.serial = enriched.serial.clone();
                descriptor.firmware = enriched.firmware.clone();
            }
        }
        Ok(snapshot)
    }
    pub fn sync_plugins(&self) {
        let host = self.host.snapshot();
        for runtime in self.runtimes() {
            let mut inner = runtime.core.inner.lock().unwrap();
            inner.snapshot.plugins = host.plugins.clone();
            inner.snapshot.config.disabled_plugins = host.config.disabled_plugins.clone();
        }
    }
    pub async fn restore_owner(&self, owner: &str) -> Result<()> {
        let mut error = None;
        for runtime in self.runtimes() {
            if let Err(e) = runtime.core.restore_plugin(owner).await {
                error = Some(e);
            }
        }
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    pub async fn stop_automation(&self) {
        self.collectors.shutdown().await;
        for runtime in self.runtimes() {
            runtime.scenes.shutdown().await;
        }
    }
    pub async fn shutdown(&self) {
        #[cfg(windows)]
        {
            let raw = self.raw.lock().unwrap().take();
            if let Some(raw) = raw {
                let _ = tokio::task::spawn_blocking(move || raw.stop()).await;
            }
        }
        self.stop_automation().await;
        for runtime in self.runtimes() {
            runtime.scenes.shutdown().await;
            if let Err(e) = runtime.core.reset_led_for_shutdown().await {
                self.host.log("warning", e.to_string());
            }
            let _ = runtime.core.stop.send(true);
        }
        self.cancel.cancel();
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap());
        for (id, mut task) in tasks {
            match tokio::time::timeout(std::time::Duration::from_secs(5), &mut task).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => self
                    .host
                    .log("error", format!("设备后端异常退出 {id}：{error}")),
                Err(_) => {
                    self.host.log(
                        "error",
                        format!("设备后端关闭超时 {id}；保留任务及原生库所有权"),
                    );
                    self.tasks.lock().unwrap().push((id, task));
                }
            }
        }
    }
}

/// Used by IPC, MCP and plugin entry points. There is no implicit UI selection.
pub fn resolve_device(host: &Arc<Core>, id: &str, online: bool) -> Result<Arc<Core>> {
    if id.is_empty() {
        return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::InvalidArgument, "backend.e020", "必须指定 device_id")
        .into());
    }
    if let Some(manager) = host.device_manager() {
        let core = if online {
            manager.device(id)?
        } else {
            manager.device_profile(id)?
        };
        if let Some(error) = &core.profile_error {
            return Err(alchemy_device_api::ApiError::new(
                alchemy_device_api::ErrorCode::ConfigUnavailable,
                error,
            )
            .into());
        }
        Ok(core)
    } else {
        let inner = host.inner.lock().unwrap();
        if inner.snapshot.selected_device.as_deref() != Some(id) {
            return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::NotFound, "backend.e017", "未知设备")
            .into());
        }
        if online && !inner.paths.contains_key(id) {
            return Err(alchemy_device_api::ApiError::localized(alchemy_device_api::ErrorCode::Offline, "backend.e019", "设备已离线")
            .into());
        }
        Ok(host.clone())
    }
}

#[cfg(all(test, feature = "device-m18"))]
mod tests;

#[cfg(test)]
mod feature_tests {
    #[test]
    fn fresh_workspace_starts_with_only_the_enabled_registry() {
        use super::*;
        let root = std::env::temp_dir().join(format!("alchemy-feature-{}", uuid::Uuid::new_v4()));
        let host = Core::new(root.clone()).unwrap();
        let manager = DeviceManager::new(host).unwrap();
        let snapshot = manager.snapshot().unwrap();
        for profile in &snapshot.devices {
            assert!(manager.adapters.contains_key(&profile.adapter_id));
            assert!(profile.module_available);
        }
        #[cfg(not(feature = "device-m18"))]
        {
            assert!(snapshot.devices.is_empty());
            assert!(manager.host.snapshot().config.pages.is_empty());
        }
        #[cfg(not(any(feature = "device-m18", feature = "device-ms-audio-dock")))]
        assert!(manager.adapters.is_empty());
        drop(manager);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
mod platform_tests;
