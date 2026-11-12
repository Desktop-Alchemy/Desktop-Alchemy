use crate::{core::Core, model::*, plugin_package, storage};
use alchemy_device_api::{ApiError, ErrorCode};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{watch, Mutex},
};
struct RunState {
    stop: watch::Sender<bool>,
    done: watch::Receiver<Option<std::result::Result<(), String>>>,
    pid: Arc<AtomicU32>,
    control: tokio::sync::mpsc::Sender<Value>,
}
pub struct PluginManager {
    pub core: Arc<Core>,
    runs: Mutex<BTreeMap<String, RunState>>,
    lifecycle: std::sync::Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    shutting_down: AtomicBool,
    discovery: std::sync::Mutex<BTreeMap<(String, String), std::time::Instant>>,
}
const PERMISSIONS: &[&str] = &[
    "events",
    "devices.read",
    "pages.read",
    "pages.activate",
    "display.write",
    "device.write",
    "widgets.write",
    "icons.write",
];
impl PluginManager {
    pub fn new(core: Arc<Core>) -> Arc<Self> {
        let this = Arc::new(Self {
            core,
            runs: Mutex::new(BTreeMap::new()),
            lifecycle: std::sync::Mutex::new(BTreeMap::new()),
            shutting_down: AtomicBool::new(false),
            discovery: Default::default(),
        });
        for id in this.core.snapshot().config.managed_plugin_ids {
            if let Err(e) = plugin_package::validate_id(&id).and_then(|()| {
                let path = this.core.root.join("plugins").join(&id).join("plugin.json");
                let manifest: PluginManifest = serde_json::from_slice(&std::fs::read(&path)?)?;
                if manifest.id != id {
                    bail!(alchemy_device_api::UiFailure::new("backend.b112", "插件目录 ID 与清单不一致"));
                }
                this.register(&path, false).map(|_| ())
            }) {
                this.core
                    .log("warning", format!("插件 {id} 不可用，请重新导入：{e}"));
            }
        }
        this
    }
    pub fn register(&self, path: &Path, persist: bool) -> Result<String> {
        if persist {
            bail!(alchemy_device_api::UiFailure::new("backend.b113", "外部插件必须通过 .m18plugin 包导入"));
        }
        let path = path.canonicalize()?;
        let manifest: PluginManifest = serde_json::from_slice(&std::fs::read(&path)?)?;
        validate_manifest(&manifest)?;
        let dir = path.parent().context(alchemy_device_api::UiFailure::new("backend.b114", "插件目录无效"))?;
        if let Some(executable) = &manifest.executable {
            let exe = dir.join(executable).canonicalize()?;
            if !exe.starts_with(dir) {
                bail!(alchemy_device_api::UiFailure::new("backend.b115", "插件程序必须位于插件目录内"));
            }
        }
        let icon_mount = self.core.icons.prepare(&manifest, dir)?;
        let id = manifest.id.clone();
        let mut i = self.core.inner.lock().unwrap();
        if let Some(existing) = i.snapshot.plugins.iter().find(|p| p.manifest.id == id) {
            if existing.path != path.to_string_lossy() {
                bail!(alchemy_device_api::UiFailure::new("backend.b116", "插件 ID 已被其他目录使用"))
            };
            return Ok(id);
        }
        i.snapshot.plugins.push(PluginInfo { ui_message: None,
            manifest,
            path: path.to_string_lossy().into(),
            status: "stopped".into(),
            error: None,
            built_in: false,
        });
        drop(i);
        self.core.icons.mount(&id, icon_mount);
        self.core.emit();
        Ok(id)
    }
    pub fn status(&self, id: &str, status: &str, error: Option<String>) {
        let mut i = self.core.inner.lock().unwrap();
        if let Some(p) = i.snapshot.plugins.iter_mut().find(|p| p.manifest.id == id) {
            p.status = status.into();
            p.ui_message = error.as_ref().map(|_| UiMessage::new("errors.pluginFailed"));
            p.error = error;
        }
        drop(i);
        self.core.emit();
    }
    /// Built-ins resolve from this installation, never from a backed-up executable path.
    pub fn register_performance(&self, resource: &Path) -> Result<String> {
        let dir = resource.join("plugins/performance");
        let manifest: PluginManifest =
            serde_json::from_slice(&std::fs::read(dir.join("plugin.json"))?)?;
        if manifest.id != "dev.masterain.performance" {
            bail!(alchemy_device_api::UiFailure::new("backend.b117", "内置性能插件标识不符"));
        }
        let id = self.register(&dir.join("plugin.json"), false)?;
        if let Some(plugin) = self
            .core
            .inner
            .lock()
            .unwrap()
            .snapshot
            .plugins
            .iter_mut()
            .find(|p| p.manifest.id == id)
        {
            plugin.built_in = true;
        }
        self.core.emit();
        Ok(id)
    }
    fn operation(&self, id: &str) -> Arc<Mutex<()>> {
        self.lifecycle
            .lock()
            .unwrap()
            .entry(id.to_ascii_lowercase())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
    fn enabled(&self, id: &str) -> bool {
        !self
            .core
            .inner
            .lock()
            .unwrap()
            .snapshot
            .config
            .disabled_plugins
            .iter()
            .any(|p| p == id)
    }
    pub async fn running_pid(&self, id: &str) -> Option<u32> {
        self.runs
            .lock()
            .await
            .get(id)
            .map(|r| r.pid.load(Ordering::SeqCst))
            .filter(|p| *p != 0)
    }
    pub async fn discover_widgets(
        self: &Arc<Self>,
        id: &str,
        lease: &str,
    ) -> Result<WidgetCatalog> {
        if !crate::widgets::identifier(lease) {
            bail!(alchemy_device_api::UiFailure::new("backend.b118", "发现会话无效"));
        }
        if !self.enabled(id) {
            bail!(alchemy_device_api::UiFailure::new("backend.b119", "插件已停用，请先在扩展中启用"));
        }
        if !self
            .core
            .snapshot()
            .plugins
            .iter()
            .any(|p| p.manifest.id == id && p.manifest.widgets.iter().any(|w| w.select_source))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b120", "插件不支持数据来源"));
        }
        {
            let mut leases = self.discovery.lock().unwrap();
            leases.retain(|_, end| *end > std::time::Instant::now());
            if leases.len() >= 64 && !leases.contains_key(&(id.into(), lease.into())) {
                bail!(alchemy_device_api::UiFailure::new("backend.b121", "发现会话过多"));
            }
            leases.insert(
                (id.into(), lease.into()),
                std::time::Instant::now() + Duration::from_secs(15),
            );
        }
        self.reconcile().await;
        Ok(self.core.widget_catalog(id))
    }
    pub async fn release_discovery(self: &Arc<Self>, id: &str, lease: &str) {
        self.discovery
            .lock()
            .unwrap()
            .remove(&(id.into(), lease.into()));
        self.reconcile().await;
    }
    pub async fn enable_hardware_sensors(self: &Arc<Self>) -> Result<()> {
        let id = "dev.masterain.performance";
        self.discover_widgets(id, "hardware-authorize").await?;
        let runs = self.runs.lock().await;
        let run = runs.get(id).context(alchemy_device_api::UiFailure::new("backend.b122", "性能插件未运行，请重试"))?;
        run.control
            .try_send(json!({"jsonrpc":"2.0","method":"sensors.enable","params":{}}))
            .context(alchemy_device_api::UiFailure::new("backend.b123", "硬件采集请求繁忙"))?;
        Ok(())
    }
    pub async fn set_enabled(self: &Arc<Self>, id: &str, enabled: bool) -> Result<()> {
        let operation = self.operation(id);
        let guard = operation.lock().await;
        let plugin = self
            .core
            .snapshot()
            .plugins
            .into_iter()
            .find(|p| p.manifest.id == id)
            .context(alchemy_device_api::UiFailure::new("backend.b124", "插件不存在"))?;
        if plugin.manifest.executable.is_none() {
            bail!(alchemy_device_api::UiFailure::new("backend.b125", "图标包安装后即可使用，无需启停"));
        }
        let saved = {
            let mut i = self.core.inner.lock().unwrap();
            let mut next = i.snapshot.config.clone();
            next.disabled_plugins.retain(|p| p != id);
            if !enabled {
                next.disabled_plugins.push(id.into());
            }
            let saved = plugin_package::ensure_ready(&self.core.root)
                .and_then(|()| storage::save(&self.core.root, &next));
            if saved.is_ok() || !enabled {
                i.snapshot.config = next;
            }
            saved
        };
        if !enabled {
            self.discovery
                .lock()
                .unwrap()
                .retain(|(owner, _), _| owner != id);
            self.stop_inner(id).await?;
            self.core.emit();
            if let Err(error) = saved {
                self.status(
                    id,
                    "error",
                    Some("插件已停止，但停用状态保存失败，请重试".into()),
                );
                return Err(error.context(alchemy_device_api::UiFailure::new("backend.b126", "插件已停止，但停用状态保存失败，请重试")));
            }
            self.status(id, "stopped", None);
        } else {
            saved?;
            if plugin.status == "error" {
                self.status(id, "stopped", None);
            }
            if plugin.manifest.widgets.is_empty()
                && !["running", "starting"].contains(&plugin.status.as_str())
            {
                self.start_inner(id).await?;
            }
        }
        self.core.emit();
        drop(guard);
        self.reconcile().await;
        Ok(())
    }
    pub fn inspect_package(&self, path: &Path) -> Result<PluginPackageSummary> {
        let mut summary = plugin_package::inspect(path)?;
        summary.installed_version = self
            .core
            .snapshot()
            .plugins
            .into_iter()
            .find(|p| p.manifest.id.eq_ignore_ascii_case(&summary.manifest.id))
            .map(|p| p.manifest.version);
        Ok(summary)
    }
    pub async fn install_package(
        self: &Arc<Self>,
        path: PathBuf,
        fingerprint: String,
    ) -> Result<String> {
        let root = self.core.root.clone();
        let stage =
            tokio::task::spawn_blocking(move || plugin_package::stage(&root, &path, &fingerprint))
                .await??;
        let id = stage.summary.manifest.id.clone();
        let operation = self.operation(&id);
        let guard = operation.lock().await;
        if self.shutting_down.load(Ordering::SeqCst) {
            bail!(alchemy_device_api::UiFailure::new("backend.b127", "客户端正在退出"));
        }
        let previous = self
            .core
            .snapshot()
            .plugins
            .into_iter()
            .find(|p| p.manifest.id.eq_ignore_ascii_case(&id));
        if let Some(previous) = &previous {
            if previous.built_in || previous.manifest.id != id {
                bail!(alchemy_device_api::UiFailure::new("backend.b128", "插件 ID 与已有插件冲突"));
            }
        }
        let was_running = previous
            .as_ref()
            .is_some_and(|p| ["starting", "running"].contains(&p.status.as_str()));
        self.stop_inner(&id).await?;
        let icon_mount = match self
            .core
            .icons
            .prepare(&stage.summary.manifest, &stage.directory)
        {
            Ok(mount) => mount,
            Err(error) => {
                if was_running && self.enabled(&id) {
                    let _ = self.start_inner(&id).await;
                }
                return Err(error);
            }
        };
        let result = {
            let mut i = self.core.inner.lock().unwrap();
            let mut next = i.snapshot.config.clone();
            if next
                .managed_plugin_ids
                .iter()
                .any(|existing| existing != &id && existing.eq_ignore_ascii_case(&id))
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b129", "插件 ID 大小写冲突"));
            }
            if !next.managed_plugin_ids.contains(&id) {
                next.managed_plugin_ids.push(id.clone());
            }
            if stage.summary.manifest.executable.is_none() {
                next.disabled_plugins.retain(|p| p != &id);
            }
            if stage.summary.manifest.executable.is_some()
                && previous
                    .as_ref()
                    .is_none_or(|p| p.manifest.executable.is_none())
                && !next.disabled_plugins.contains(&id)
            {
                next.disabled_plugins.push(id.clone());
            }
            plugin_package::commit(
                &self.core.root,
                &id,
                &i.snapshot.config,
                &next,
                Some(&stage.directory),
            )
            .map(|()| {
                i.snapshot.config = next;
                i.snapshot.plugins.retain(|p| p.manifest.id != id);
                i.snapshot.plugins.push(PluginInfo { ui_message: None,
                    manifest: stage.summary.manifest.clone(),
                    path: self
                        .core
                        .root
                        .join("plugins")
                        .join(&id)
                        .join("plugin.json")
                        .to_string_lossy()
                        .into(),
                    status: "stopped".into(),
                    error: None,
                    built_in: false,
                });
            })
        };
        if result.is_ok() {
            self.core.icons.mount(&id, icon_mount);
        }
        let has_executable = if result.is_ok() {
            stage.summary.manifest.executable.is_some()
        } else {
            previous
                .as_ref()
                .is_some_and(|p| p.manifest.executable.is_some())
        };
        if was_running && self.enabled(&id) && has_executable {
            let _ = self.start_inner(&id).await;
        }
        self.core.emit();
        result?;
        drop(guard);
        self.reconcile().await;
        Ok(id)
    }
    fn all_pages(&self) -> Vec<Page> {
        self.core
            .device_manager()
            .map(|manager| {
                manager
                    .runtimes()
                    .iter()
                    .flat_map(|r| r.core.snapshot().config.pages)
                    .collect()
            })
            .unwrap_or_else(|| self.core.snapshot().config.pages)
    }
    async fn reconcile(self: &Arc<Self>) {
        if *self.core.stop.borrow() || self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        self.discovery
            .lock()
            .unwrap()
            .retain(|_, end| *end > std::time::Instant::now());
        let snapshot = self.core.snapshot();
        for plugin in &snapshot.plugins {
            if plugin.manifest.widgets.is_empty() {
                continue;
            }
            let wanted = !snapshot
                .config
                .disabled_plugins
                .contains(&plugin.manifest.id)
                && (self
                    .discovery
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|((owner, _), end)| {
                        owner == &plugin.manifest.id && *end > std::time::Instant::now()
                    })
                    || self.all_pages().iter().flat_map(|p| &p.keys).any(|k| {
                        k.widget.as_ref().is_some_and(|w| {
                            w.plugin_id == plugin.manifest.id
                                && plugin
                                    .manifest
                                    .widgets
                                    .iter()
                                    .any(|definition| definition.id == w.widget_id)
                        })
                    }));
            if wanted && plugin.status == "stopped" {
                // Another reconciliation may already have started the same run.
                // Spawn/protocol failures are reported by that run's cleanup task.
                let _ = self.start(&plugin.manifest.id).await;
            } else if !wanted && ["running", "starting"].contains(&plugin.status.as_str()) {
                let _ = self.stop(&plugin.manifest.id).await;
            }
        }
    }
    pub async fn supervise(self: Arc<Self>) {
        let mut changes = self.core.changed.subscribe();
        let mut stop = self.core.stop.subscribe();
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                _ = changes.recv() => self.reconcile().await,
                _ = ticker.tick() => {
                    self.reconcile().await;
                    let core = self.core.clone();
                    match tokio::task::spawn_blocking(move || core.refresh_widgets()).await {
                        Ok(Ok(())) => {},
                        result => self.core.log("error", format!("插件显示更新失败：{result:?}")),
                    }
                }
            }
        }
    }
    pub async fn start(self: &Arc<Self>, id: &str) -> Result<()> {
        let operation = self.operation(id);
        let _guard = operation.lock().await;
        self.start_inner(id).await
    }
    async fn start_inner(self: &Arc<Self>, id: &str) -> Result<()> {
        plugin_package::ensure_ready(&self.core.root)?;
        if self.shutting_down.load(Ordering::SeqCst) || *self.core.stop.borrow() {
            bail!(alchemy_device_api::UiFailure::new("backend.b127", "客户端正在退出"));
        }
        if !self.enabled(id) {
            bail!(alchemy_device_api::UiFailure::new("backend.b130", "插件已停用，请先启用"));
        }
        let plugin = self
            .core
            .snapshot()
            .plugins
            .into_iter()
            .find(|p| p.manifest.id == id)
            .context(alchemy_device_api::UiFailure::new("backend.b124", "插件不存在"))?;
        if plugin.manifest.executable.is_none() {
            bail!(alchemy_device_api::UiFailure::new("backend.b131", "图标包无需启动"));
        }
        let mut runs = self.runs.lock().await;
        if runs.contains_key(id) {
            bail!(alchemy_device_api::UiFailure::new("backend.b132", "插件已运行或正在停止"));
        }
        let (stop, rx) = watch::channel(false);
        let (control, control_rx) = tokio::sync::mpsc::channel(8);
        let (done, completion) = watch::channel(None);
        let pid = Arc::new(AtomicU32::new(0));
        runs.insert(
            id.into(),
            RunState {
                stop,
                done: completion,
                pid: pid.clone(),
                control,
            },
        );
        self.status(id, "starting", None);
        drop(runs);
        let manager = self.clone();
        tokio::spawn(async move {
            let id = plugin.manifest.id.clone();
            let (result, terminated) = manager.run(plugin, rx, pid.clone(), control_rx).await;
            manager.core.cancel_owner(&id);
            manager.core.emit_widgets();
            if terminated {
                pid.store(0, Ordering::SeqCst);
                manager.status(&id, "stopping", None);
                if !*manager.core.stop.borrow() {
                    match tokio::time::timeout(
                        Duration::from_secs(3),
                        manager.core.restore_plugin(&id),
                    )
                    .await
                    {
                        Ok(Ok(())) => {}
                        error => manager.core.log(
                            "error",
                            format!("插件 {id} 已退出，画面恢复失败：{error:?}"),
                        ),
                    }
                }
                match &result {
                    Ok(()) => manager.status(&id, "stopped", None),
                    Err(e) => {
                        manager.status(&id, "error", Some(e.to_string()));
                        manager.core.log("error", format!("插件 {id}：{e:#}"));
                    }
                }
                manager.runs.lock().await.remove(&id);
                done.send_replace(Some(result.map_err(|e| e.to_string())));
            } else {
                manager.status(
                    &id,
                    "error",
                    Some("插件进程回收失败，禁止重新启动或替换".into()),
                );
                done.send_replace(Some(Err(result
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_default())));
            }
        });
        Ok(())
    }
    pub async fn stop(&self, id: &str) -> Result<()> {
        let operation = self.operation(id);
        let _guard = operation.lock().await;
        self.stop_inner(id).await
    }
    async fn stop_inner(&self, id: &str) -> Result<()> {
        self.core.cancel_owner(id);
        self.core.emit_widgets();
        let completion = {
            let runs = self.runs.lock().await;
            runs.get(id).map(|run| {
                run.stop.send_replace(true);
                self.status(id, "stopping", None);
                run.done.clone()
            })
        };
        if let Some(mut completion) = completion {
            loop {
                if let Some(result) = completion.borrow().clone() {
                    return result.map_err(anyhow::Error::msg);
                }
                completion.changed().await.context(alchemy_device_api::UiFailure::new("backend.b133", "插件停止任务意外结束"))?;
            }
        }
        Ok(())
    }
    pub async fn stop_all(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let ids: Vec<_> = self.runs.lock().await.keys().cloned().collect();
        for id in ids {
            if let Err(e) = self.stop(&id).await {
                self.core.log("error", format!("插件 {id} 停止失败：{e}"));
            }
        }
    }
    pub async fn remove(self: &Arc<Self>, id: &str) -> Result<()> {
        let operation = self.operation(id);
        let _guard = operation.lock().await;
        plugin_package::validate_id(id)?;
        let plugin = self
            .core
            .snapshot()
            .plugins
            .into_iter()
            .find(|p| p.manifest.id == id);
        if id.eq_ignore_ascii_case("dev.masterain.performance")
            || plugin.as_ref().is_some_and(|p| p.built_in)
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b134", "内置插件不能移除，可以停用"));
        }
        if plugin.is_none()
            && !self
                .core
                .snapshot()
                .config
                .managed_plugin_ids
                .iter()
                .any(|p| p == id)
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b124", "插件不存在"));
        }
        self.stop_inner(id).await?;
        let mut i = self.core.inner.lock().unwrap();
        let mut next = i.snapshot.config.clone();
        next.managed_plugin_ids.retain(|p| p != id);
        next.disabled_plugins.retain(|p| p != id);
        plugin_package::commit(&self.core.root, id, &i.snapshot.config, &next, None)?;
        i.snapshot.config = next;
        i.snapshot.plugins.retain(|p| p.manifest.id != id);
        drop(i);
        self.core.icons.unmount(id);
        self.core.emit();
        Ok(())
    }
    #[cfg(windows)]
    async fn run(
        self: &Arc<Self>,
        plugin: PluginInfo,
        mut stop: watch::Receiver<bool>,
        pid: Arc<AtomicU32>,
        mut control: tokio::sync::mpsc::Receiver<Value>,
    ) -> (Result<()>, bool) {
        let spawned = (|| -> Result<crate::process_lock::SpawnedPlugin> {
            if *stop.borrow() || !self.enabled(&plugin.manifest.id) {
                bail!(alchemy_device_api::UiFailure::new("backend.b135", "插件启动已取消"));
            }
            let dir = PathBuf::from(&plugin.path)
                .parent()
                .context(alchemy_device_api::UiFailure::new("backend.b114", "插件目录无效"))?
                .canonicalize()?;
            let exe = dir
                .join(
                    plugin
                        .manifest
                        .executable
                        .as_ref()
                        .context(alchemy_device_api::UiFailure::new("backend.b136", "图标包没有可执行程序"))?,
                )
                .canonicalize()?;
            if !exe.starts_with(&dir) {
                bail!(alchemy_device_api::UiFailure::new("backend.b137", "插件程序越过目录边界"));
            }
            crate::process_lock::PluginProcess::spawn(&exe, &dir)
        })();
        let crate::process_lock::SpawnedPlugin {
            process,
            mut input,
            output,
            error,
        } = match spawned {
            Ok(spawned) => spawned,
            Err(e) => return (if *stop.borrow() { Ok(()) } else { Err(e) }, true),
        };
        pid.store(process.pid, Ordering::SeqCst);
        let mut output = BufReader::new(output);
        let core = self.core.clone();
        let id = plugin.manifest.id.clone();
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(error);
            while let Ok(Some(s)) = bounded_line(&mut reader).await {
                core.log("plugin", format!("{id}：{s}"));
            }
        });
        let mut host_stop = self.core.stop.subscribe();
        let result = {
            let protocol = async {
                write(&mut input, &json!({"jsonrpc":"2.0","id":"initialize","method":"initialize","params":{"apiVersion":3,"pluginId":plugin.manifest.id,"methods":alchemy_device_api::METHODS,"events":alchemy_device_api::EVENTS,"hostApiVersion":1,"capabilities":{"input_kinds":["button","rotary","analog"],"device_description":true,"temporary_control":true,"module_settings":false}}})).await?;
                let line = tokio::time::timeout(Duration::from_secs(5), bounded_line(&mut output))
                    .await
                    .context(alchemy_device_api::UiFailure::new("backend.b138", "插件握手超时"))??
                    .context(alchemy_device_api::UiFailure::new("backend.b139", "插件在握手前退出"))?;
                let hello: Value = serde_json::from_str(&line)?;
                if hello["jsonrpc"] != "2.0"
                    || hello["id"] != "initialize"
                    || hello["result"]["apiVersion"] != 3
                {
                    return Err(ApiError::localized(ErrorCode::VersionUnsupported, "backend.e023", "插件握手响应或协议版本不符")
                    .into());
                }
                self.status(&plugin.manifest.id, "running", None);
                let mut events = self.core.events.subscribe();
                let mut subscriptions = HashSet::<String>::new();
                let mut ticker = tokio::time::interval(Duration::from_secs(10));
                ticker.tick().await;
                let mut awaiting: Option<String> = None;
                loop {
                    tokio::select! {
                        Some(message) = control.recv() => write(&mut input, &message).await?,
                        _ = ticker.tick() => {
                            if awaiting.is_some() { bail!(alchemy_device_api::UiFailure::new("backend.b140", "插件心跳请求超时")); }
                            let id = format!("ping-{}", now());
                            write(&mut input, &json!({"jsonrpc":"2.0","id":id,"method":"ping","params":{}})).await?;
                            awaiting = Some(id);
                        },
                        line = bounded_line(&mut output) => {
                            let msg: Value = serde_json::from_str(&line?.context(alchemy_device_api::UiFailure::new("backend.b141", "插件进程已退出"))?)?;
                            if msg["jsonrpc"] != "2.0" { bail!(alchemy_device_api::UiFailure::new("backend.b142", "插件消息缺少 jsonrpc 2.0")); }
                            if msg.get("method").is_none() {
                                if awaiting.as_ref().is_some_and(|id| msg["id"].as_str() == Some(id)) { awaiting = None; }
                                continue;
                            }
                            let id = msg.get("id").cloned();
                            let response = match tokio::time::timeout(Duration::from_secs(5), self.dispatch(&plugin.manifest, &msg, &mut subscriptions)).await {
                                Ok(result) => result, Err(_) => Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new("backend.b143", "插件请求处理超时"))),
                            };
                            if let Some(id) = id {
                                let reply = match response {
                                    Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                                    Err(e) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":e.to_string(),"data":alchemy_device_api::ApiError::from_error(&e)}}),
                                };
                                write(&mut input, &reply).await?;
                            }
                        },
                        event = events.recv() => match event {
                            Ok(e) if subscriptions.contains(&e.kind) => write(&mut input, &json!({"jsonrpc":"2.0","method":"event","params":e})).await?,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => self.core.log("warning", format!("插件事件积压，已跳过 {n} 条旧事件")),
                            _ => {},
                        },
                    }
                }
                #[allow(unreachable_code)]
                Ok::<(), anyhow::Error>(())
            };
            if *stop.borrow() || *host_stop.borrow() {
                Ok(())
            } else {
                tokio::select! { biased;
                    _ = stop.changed() => Ok(()), _ = host_stop.changed() => Ok(()), result = protocol => result,
                }
            }
        };
        self.core.cancel_owner(&plugin.manifest.id);
        let graceful = tokio::time::timeout(
            Duration::from_secs(if plugin.manifest.id == "dev.masterain.performance" {
                7
            } else {
                1
            }),
            async {
                let _ = write(
                    &mut input,
                    &json!({"jsonrpc":"2.0","id":"shutdown","method":"shutdown","params":{}}),
                )
                .await;
                while !process.exited()? {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Ok::<(), anyhow::Error>(())
            },
        )
        .await;
        let result = if plugin.manifest.id == "dev.masterain.performance" && result.is_ok() {
            match graceful {
                Ok(Ok(())) => match process.exit_code() {
                    Ok(0) => result,
                    Ok(code) => Err(anyhow::anyhow!(
                        "性能采集停止异常（退出码 {code}），请检查采集进程"
                    )),
                    Err(error) => Err(error),
                },
                _ => Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new("backend.b144", "性能采集未在期限内停止，已回收普通进程；请检查提权采集状态"))),
            }
        } else {
            result
        };
        let teardown = async {
            process.job.terminate()?;
            tokio::time::timeout(Duration::from_secs(3), async {
                while !process.job.is_empty()? || !process.exited()? {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Ok::<(), anyhow::Error>(())
            })
            .await
            .context(alchemy_device_api::UiFailure::new("backend.b145", "插件进程回收超时"))??;
            Ok::<(), anyhow::Error>(())
        }
        .await;
        stderr_task.abort();
        match teardown {
            Ok(()) => (result, true),
            Err(e) => (Err(e), false),
        }
    }
    #[cfg(not(windows))]
    async fn run(
        self: &Arc<Self>,
        _plugin: PluginInfo,
        _stop: watch::Receiver<bool>,
        _pid: Arc<AtomicU32>,
        _control: tokio::sync::mpsc::Receiver<Value>,
    ) -> (Result<()>, bool) {
        (Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new("backend.b146", "插件仅支持 Windows"))), true)
    }
    async fn dispatch(
        &self,
        manifest: &PluginManifest,
        msg: &Value,
        subs: &mut HashSet<String>,
    ) -> Result<Value> {
        if !self.enabled(&manifest.id) {
            return Err(ApiError::localized(ErrorCode::StaleOperation, "backend.e024", "插件已停用").into());
        }
        let method = msg["method"]
            .as_str()
            .ok_or_else(|| ApiError::localized(ErrorCode::InvalidArgument, "backend.e025", "插件方法无效"))?;
        let p = &msg["params"];
        validate_request_keys(method, p)?;
        let permission = match method {
            "ping" => None,
            "events.subscribe" => Some("events"),
            "devices.list" => Some("devices.read"),
            "pages.list" => Some("pages.read"),
            "pages.activate" => Some("pages.activate"),
            "display.key" | "display.background" | "assets.import" => Some("display.write"),
            "device.command" => Some("device.write"),
            "icons.list" | "icons.publish" | "icons.remove" => Some("icons.write"),
            "widgets.publish" | "widgets.publish_batch" | "widgets.sources.publish" => {
                Some("widgets.write")
            }
            _ => bail!("未知插件方法：{method}"),
        };
        if permission.is_some_and(|perm| !manifest.permissions.iter().any(|p| p == perm)) {
            return Err(
                ApiError::localized(ErrorCode::Unsupported, "backend.e026", "插件未声明所需的客户端 API 权限").into(),
            );
        }
        let core = if matches!(
            method,
            "pages.list"
                | "pages.activate"
                | "display.key"
                | "display.background"
                | "device.command"
        ) {
            crate::devices::resolve_device(
                &self.core,
                p["device_id"].as_str().context(alchemy_device_api::UiFailure::new("backend.b147", "缺少 device_id"))?,
                method != "pages.list",
            )?
        } else {
            self.core.clone()
        };
        match method {
            "icons.list" => Ok(serde_json::to_value(core.icons.list(&manifest.id))?),
            "icons.publish" => {
                let id = p["id"].as_str().context(alchemy_device_api::UiFailure::new("backend.b148", "缺少 id"))?;
                let name = p["name"].as_str().context(alchemy_device_api::UiFailure::new("backend.b149", "缺少 name"))?;
                let data = storage::decode_data(p["data"].as_str().context(alchemy_device_api::UiFailure::new("backend.b150", "缺少 data"))?)?;
                Ok(serde_json::to_value(core.icons.publish(
                    &manifest.id,
                    id,
                    name,
                    &data,
                )?)?)
            }
            "icons.remove" => Ok(
                json!({"removed": core.icons.remove(&manifest.id, p["id"].as_str().context(alchemy_device_api::UiFailure::new("backend.b148", "缺少 id"))?)?}),
            ),
            "widgets.sources.publish" => {
                core.publish_widget_catalog(&manifest.id, alchemy_device_api::strict(p.clone())?)?;
                Ok(json!({"accepted":true}))
            }
            "widgets.publish_batch" => {
                core.publish_widgets(&manifest.id, alchemy_device_api::strict(p.clone())?)?;
                Ok(json!({"accepted":true}))
            }
            "widgets.publish" => {
                core.publish_widget(&manifest.id, alchemy_device_api::strict(p.clone())?)?;
                Ok(json!({"accepted":true}))
            }
            "ping" => Ok(json!({"ok":true})),
            "events.subscribe" => {
                let kinds: Vec<String> = serde_json::from_value(p["kinds"].clone())?;
                for k in &kinds {
                    if ![
                        "input",
                        "device.connected",
                        "device.disconnected",
                        "button.down",
                        "button.up",
                        "page.changed",
                    ]
                    .contains(&k.as_str())
                    {
                        return Err(ApiError::new(
                            ErrorCode::InvalidArgument,
                            format!("未知事件：{k}"),
                        )
                        .into());
                    }
                }
                *subs = kinds.into_iter().collect();
                Ok(json!({"ok":true}))
            }
            "devices.list" => Ok(serde_json::to_value(
                self.core
                    .device_manager()
                    .map(|m| {
                        m.snapshot().map(|s| {
                            s.devices
                                .iter()
                                .map(alchemy_device_api::DeviceDto::from_state)
                                .collect::<Vec<_>>()
                        })
                    })
                    .transpose()?
                    .unwrap_or_default(),
            )?),
            "pages.list" => Ok(
                json!({"device_id":p["device_id"],"active_page":core.snapshot().active_page,"pages":core.snapshot().config.pages.iter().map(|p|json!({"id":p.id,"name":p.name})).collect::<Vec<_>>()}),
            ),
            "pages.activate" => {
                core.activate_from(
                    p["page_id"].as_str().context(alchemy_device_api::UiFailure::new("backend.b151", "缺少 page_id"))?,
                    crate::core::ActivationSource::External,
                )
                .await?;
                Ok(json!({"ok":true}))
            }
            "assets.import" => {
                let data = storage::decode_data(p["data"].as_str().context(alchemy_device_api::UiFailure::new("backend.b150", "缺少 data"))?)?;
                Ok(json!({"assetId":storage::put_asset(&core.root,&data)?}))
            }
            "display.key" => {
                let visual: KeyVisual = alchemy_device_api::strict::<
                    alchemy_device_api::DisplayVisual,
                >(p["visual"].clone())?
                .into();
                if !core.layout.display_keys.contains(&visual.key) {
                    bail!(alchemy_device_api::UiFailure::new("backend.b152", "目标按键不支持显示"))
                };
                storage::color(&visual.foreground)?;
                storage::color(&visual.background)?;
                if visual.text.chars().count() > 80 {
                    bail!(alchemy_device_api::UiFailure::new("backend.b005", "文字过长"))
                };
                let epoch = core.inner.lock().unwrap().epoch;
                if visual.plugin_icon.is_some() {
                    let core = core.clone();
                    let owner = manifest.id.clone();
                    let receipt = tokio::task::spawn_blocking(move || {
                        core.plugin_visual(&owner, visual, epoch)
                    })
                    .await??;
                    return Ok(json!({"receiptId":receipt}));
                }
                let key = visual.key;
                let rendering = core.clone();
                let png = tokio::task::spawn_blocking(move || rendering.key_png(&visual)).await??;
                use base64::Engine;
                let receipt = core.plugin_command(
                    &manifest.id,
                    HardwareCommand::KeyData {
                        key,
                        data: base64::engine::general_purpose::STANDARD.encode(png),
                    },
                    Some(epoch),
                )?;
                Ok(json!({"receiptId":receipt}))
            }
            "display.background" => {
                let cmd = HardwareCommand::Background {
                    asset_id: p["asset_id"].as_str().context(alchemy_device_api::UiFailure::new("backend.b153", "缺少 asset_id"))?.into(),
                    persistent: false,
                };
                let epoch = core.inner.lock().unwrap().epoch;
                Ok(json!({"receiptId":core.plugin_command(&manifest.id,cmd,Some(epoch))?}))
            }
            "device.command" => {
                let cmd: HardwareCommand = alchemy_device_api::strict::<
                    alchemy_device_api::DeviceCommand,
                >(p["command"].clone())?
                .into();
                if matches!(
                    cmd,
                    HardwareCommand::Background {
                        persistent: true,
                        ..
                    } | HardwareCommand::BackgroundData {
                        persistent: true,
                        ..
                    }
                ) {
                    bail!(alchemy_device_api::UiFailure::new("backend.b154", "插件不可写持久开机背景"))
                };
                let epoch = if cmd.visual() {
                    Some(core.inner.lock().unwrap().epoch)
                } else {
                    None
                };
                Ok(json!({"receiptId":core.plugin_command(&manifest.id,cmd,epoch)?}))
            }
            _ => bail!(alchemy_device_api::UiFailure::new("backend.b155", "未知方法")),
        }
    }
}
/// Validate the complete self-contained sensor payload without opening hardware or UAC.
pub fn verify_sensor_resources(resources: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = resources
        .join("plugins/performance/sensors/publish")
        .canonicalize()
        .context(alchemy_device_api::UiFailure::new("backend.b156", "传感器组件目录缺失"))?;
    let bytes = std::fs::read(root.join("FILES.json")).context(alchemy_device_api::UiFailure::new("backend.b157", "传感器组件清单缺失"))?;
    // Windows PowerShell 5 writes a UTF-8 BOM.
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let manifest: Value = serde_json::from_slice(bytes)?;
    let files = manifest["files"]
        .as_object()
        .context(alchemy_device_api::UiFailure::new("backend.b158", "传感器组件清单无效"))?;
    if files.is_empty() || files.len() > 4096 {
        bail!(alchemy_device_api::UiFailure::new("backend.b159", "传感器组件文件数量无效"));
    }
    for required in [
        "m18-sensors.exe",
        "m18-sensors.dll",
        "coreclr.dll",
        "LibreHardwareMonitorLib.dll",
        "SOURCES.json",
        "driver/PawnIO_setup.exe",
        "licenses/LibreHardwareMonitor-MPL-2.0.txt",
        "licenses/PawnIO-GPL-2.0.txt",
        "licenses/PawnIO-license-and-exception.txt",
    ] {
        if !files.contains_key(required) {
            bail!("传感器组件清单缺少 {required}");
        }
    }
    for (name, entry) in files {
        let relative = Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b160", "传感器组件文件路径无效"));
        }
        let file = root
            .join(relative)
            .canonicalize()
            .with_context(|| format!("传感器组件缺少 {name}"))?;
        if !file.starts_with(&root) {
            bail!(alchemy_device_api::UiFailure::new("backend.b161", "传感器组件越过资源目录"));
        }
        let data = std::fs::read(&file)?;
        if entry["size"].as_u64() != Some(data.len() as u64)
            || entry["sha256"].as_str() != Some(format!("{:x}", Sha256::digest(&data)).as_str())
        {
            bail!("传感器组件校验失败：{name}");
        }
    }
    Ok(())
}
pub fn validate_manifest(m: &PluginManifest) -> Result<()> {
    plugin_package::validate_id(&m.id)?;
    if m.name.trim().is_empty()
        || m.name.chars().count() > 120
        || m.name.chars().any(char::is_control)
        || m.version.is_empty()
        || m.version.len() > 100
        || m.version.chars().any(char::is_control)
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b162", "插件名称或版本无效"));
    }
    if m.widgets.len() > 32 {
        bail!(alchemy_device_api::UiFailure::new("backend.b163", "每个插件最多声明 32 个显示控件"));
    }
    let mut widget_ids = HashSet::new();
    for widget in &m.widgets {
        let mut presentations = HashSet::new();
        if widget.presentations.len() > 16 {
            bail!(alchemy_device_api::UiFailure::new("backend.b164", "每个控件最多声明 16 种显示方式"));
        }
        for presentation in &widget.presentations {
            if !crate::widgets::identifier(&presentation.id)
                || !presentations.insert(&presentation.id)
                || presentation.name.trim().is_empty()
                || presentation.name.chars().count() > 32
                || presentation.name.chars().any(char::is_control)
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b165", "插件显示方式声明无效"));
            }
        }
        if !crate::widgets::identifier(&widget.id)
            || !widget_ids.insert(&widget.id)
            || widget.name.trim().is_empty()
            || widget.name.chars().count() > 32
            || widget.name.chars().any(char::is_control)
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b166", "插件显示控件声明无效"));
        }
        crate::widgets::validate_binding(&WidgetBinding {
            source_id: None,
            source_name: None,
            presentation_id: None,
            plugin_id: m.id.clone(),
            widget_id: widget.id.clone(),
            style: widget.default_style.clone(),
        })?;
    }
    if m.id.is_empty()
        || m.id.len() > 100
        || !m
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b167", "插件 ID 无效"))
    };
    if m.api_version != 3 {
        bail!(alchemy_device_api::UiFailure::new("backend.b168", "插件协议已升级为 v3，请更新插件"))
    };
    crate::plugin_icons::validate_definitions(&m.icons)?;
    if let Some(executable) = &m.executable {
        if executable.is_empty()
            || Path::new(executable).is_absolute()
            || Path::new(executable)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b169", "插件程序须为目录内的相对路径"));
        }
    } else if m.icons.is_empty() || !m.permissions.is_empty() || !m.widgets.is_empty() {
        bail!(alchemy_device_api::UiFailure::new("backend.b170", "无 EXE 的图标包须声明图标，且不能声明运行权限或显示控件"));
    }
    if m.permissions
        .iter()
        .any(|p| !PERMISSIONS.contains(&p.as_str()))
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b171", "插件权限无效"))
    };
    Ok(())
}
async fn write(w: &mut (impl tokio::io::AsyncWrite + Unpin), v: &Value) -> Result<()> {
    let mut b = serde_json::to_vec(v)?;
    b.push(b'\n');
    tokio::time::timeout(Duration::from_secs(3), w.write_all(&b))
        .await
        .context(alchemy_device_api::UiFailure::new("backend.b172", "插件输入管道超时"))??;
    tokio::time::timeout(Duration::from_secs(3), w.flush())
        .await
        .context(alchemy_device_api::UiFailure::new("backend.b173", "插件输入管道刷新超时"))??;
    Ok(())
}
pub async fn bounded_line<R: AsyncBufRead + Unpin>(r: &mut R) -> Result<Option<String>> {
    let mut out = Vec::new();
    loop {
        let available = r.fill_buf().await?;
        if available.is_empty() {
            if out.is_empty() {
                return Ok(None);
            } else {
                bail!(alchemy_device_api::UiFailure::new("backend.b174", "插件输出了未结束的消息"))
            }
        }
        let n = available
            .iter()
            .position(|b| *b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(available.len());
        if out.len() + n > 1_048_576 {
            bail!(alchemy_device_api::UiFailure::new("backend.b175", "插件消息超过 1 MiB"))
        };
        let end = available[n - 1] == b'\n';
        out.extend_from_slice(&available[..n]);
        r.consume(n);
        if end {
            return Ok(Some(String::from_utf8(out)?.trim_end().to_string()));
        }
    }
}

fn validate_request_keys(method: &str, params: &Value) -> Result<()> {
    use alchemy_device_api::{ApiError, ErrorCode};
    let allowed: &[&str] = match method {
        "ping" | "devices.list" | "icons.list" => &[],
        "events.subscribe" => &["kinds"],
        "pages.list" => &["device_id"],
        "pages.activate" => &["device_id", "page_id"],
        "assets.import" => &["data"],
        "display.key" => &["device_id", "visual"],
        "display.background" => &["device_id", "asset_id"],
        "device.command" => &["device_id", "command"],
        "icons.publish" => &["id", "name", "data"],
        "icons.remove" => &["id"],
        "widgets.publish" | "widgets.publish_batch" | "widgets.sources.publish" => return Ok(()),
        _ => return Err(ApiError::localized(ErrorCode::Unsupported, "backend.e028", "未知插件方法").into()),
    };
    if params.is_null() && allowed.is_empty() {
        return Ok(());
    }
    let object = params
        .as_object()
        .ok_or_else(|| ApiError::localized(ErrorCode::InvalidArgument, "backend.e029", "参数须为对象"))?;
    for key in allowed {
        let value = object
            .get(*key)
            .ok_or_else(|| ApiError::new(ErrorCode::InvalidArgument, format!("缺少参数：{key}")))?;
        let valid = match *key {
            "visual" | "command" => value.is_object(),
            "kinds" => value.is_array(),
            _ => value.as_str().is_some_and(|text| !text.is_empty()),
        };
        if !valid {
            return Err(
                ApiError::new(ErrorCode::InvalidArgument, format!("参数无效：{key}")).into(),
            );
        }
    }
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(ApiError::new(ErrorCode::InvalidArgument, format!("未知参数：{key}")).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn icon_api_checks_permission_owner_and_disable_without_unmounting() {
        use base64::Engine;
        let root = std::env::temp_dir().join(format!("m18-icons-api-{}", uuid::Uuid::new_v4()));
        let core = Core::new(root.clone()).unwrap();
        let manager = PluginManager::new(core.clone());
        let mut manifest: PluginManifest=serde_json::from_value(json!({"id":"example.api","name":"API","version":"1","api_version":3,"executable":"fixture.exe","permissions":[]})).unwrap();
        core.icons
            .mount(&manifest.id, core.icons.prepare(&manifest, &root).unwrap());
        let data = base64::engine::general_purpose::STANDARD.encode(include_bytes!(
            "../resources/key-icons/transparent/light/play.png"
        ));
        let message = json!({"method":"icons.publish","params":{"id":"play","name":"播放","data":data,"plugin_id":"victim"}});
        let mut subscriptions = HashSet::new();
        assert!(manager
            .dispatch(&manifest, &message, &mut subscriptions)
            .await
            .is_err());
        manifest.permissions.push("icons.write".into());
        assert!(manager
            .dispatch(&manifest, &message, &mut subscriptions)
            .await
            .is_err());
        let message =
            json!({"method":"icons.publish","params":{"id":"play","name":"播放","data":data}});
        manager
            .dispatch(&manifest, &message, &mut subscriptions)
            .await
            .unwrap();
        assert_eq!(core.icons.list(&manifest.id).len(), 1);
        assert!(core.icons.list("victim").is_empty());
        let list = manager
            .dispatch(
                &manifest,
                &json!({"method":"icons.list","params":{}}),
                &mut subscriptions,
            )
            .await
            .unwrap();
        assert_eq!(list[0]["id"], "play");
        core.inner
            .lock()
            .unwrap()
            .snapshot
            .config
            .disabled_plugins
            .push(manifest.id.clone());
        assert!(manager
            .dispatch(&manifest, &message, &mut subscriptions)
            .await
            .is_err());
        assert_eq!(core.icons.list(&manifest.id).len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn refuses_escaping_executable() {
        let m = PluginManifest {
            id: "example".into(),
            name: "x".into(),
            version: "1".into(),
            api_version: 3,
            executable: Some("../evil.exe".into()),
            permissions: vec![],
            widgets: vec![],
            icons: vec![],
        };
        assert!(validate_manifest(&m).is_err());
    }
    #[tokio::test]
    async fn oversized_line_is_bounded() {
        let b = vec![b'a'; 1_048_577];
        assert!(bounded_line(&mut BufReader::new(b.as_slice()))
            .await
            .is_err());
    }
    #[tokio::test]
    async fn widget_publishing_requires_permission_and_declared_id() {
        let root = std::env::temp_dir().join(format!("m18-widget-api-{}", uuid::Uuid::new_v4()));
        let core = Core::new(root.clone()).unwrap();
        let mut manifest: PluginManifest =
            serde_json::from_str(include_str!("../../plugins/performance/plugin.json")).unwrap();
        validate_manifest(&manifest).unwrap();
        core.inner
            .lock()
            .unwrap()
            .snapshot
            .plugins
            .push(PluginInfo { ui_message: None,
                manifest: manifest.clone(),
                path: "test".into(),
                status: "running".into(),
                error: None,
                built_in: false,
            });
        let manager = PluginManager::new(core.clone());
        let mut subs = HashSet::new();
        let message = json!({"method":"widgets.publish","params":{"widget_id":"cpu","title":"CPU","value":"25%","detail":null,"progress":0.25,"valid":true}});
        assert_eq!(
            manager
                .dispatch(&manifest, &message, &mut subs)
                .await
                .unwrap()["accepted"],
            true
        );
        manifest.permissions.clear();
        assert!(manager
            .dispatch(&manifest, &message, &mut subs)
            .await
            .unwrap_err()
            .to_string()
            .contains("权限"));
        manifest.permissions.push("widgets.write".into());
        let mut invalid = message.clone();
        invalid["params"]["widget_id"] = json!("undeclared");
        assert!(manager
            .dispatch(&manifest, &invalid, &mut subs)
            .await
            .is_err());
        invalid = message.clone();
        invalid["params"]["progress"] = json!(1.01);
        assert!(manager
            .dispatch(&manifest, &invalid, &mut subs)
            .await
            .is_err());
        invalid = message;
        invalid["params"]["value"] = json!("x".repeat(25));
        assert!(manager
            .dispatch(&manifest, &invalid, &mut subs)
            .await
            .is_err());
        manifest.widgets.push(manifest.widgets[0].clone());
        assert!(validate_manifest(&manifest).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
