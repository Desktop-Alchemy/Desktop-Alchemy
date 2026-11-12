//! Application-side implementation of the narrow, adapter-scoped host contract.
use super::*;
use alchemy_device_api::{Connection, Host, InputValue, TransportBatch, TransportWork};
use std::sync::Weak;
type InflightBatch = (Arc<Core>, Vec<crate::core::Work>);
pub struct Bridge {
    manager: Weak<DeviceManager>,
    adapter_id: String,
    inflight: Mutex<BTreeMap<String, InflightBatch>>,
}
pub fn connection(core: &Core) -> Option<Connection> {
    let inner = core.inner.lock().unwrap();
    let id = inner.snapshot.selected_device.as_ref()?;
    Some(Connection {
        device_id: id.clone(),
        path: inner.paths.get(id)?.clone(),
        epoch: inner.connection_epoch,
    })
}
impl Bridge {
    pub fn new(manager: Weak<DeviceManager>, adapter_id: String) -> Self {
        Self {
            manager,
            adapter_id,
            inflight: Default::default(),
        }
    }
    fn manager(&self) -> Result<Arc<DeviceManager>> {
        self.manager.upgrade().context(alchemy_device_api::UiFailure::new("backend.b282", "设备宿主已停止"))
    }
    fn core(&self, id: &str) -> Result<Arc<Core>> {
        let core = self.manager()?.device_profile(id)?;
        if core.adapter_id != self.adapter_id {
            bail!(alchemy_device_api::UiFailure::new("backend.b283", "目标不属于此设备模块"))
        }
        Ok(core)
    }
    fn descriptor(&self, mut d: DeviceDescriptor) -> Result<DeviceDescriptor> {
        if d.adapter_id != self.adapter_id {
            bail!(alchemy_device_api::UiFailure::new("backend.b284", "设备模块归属无效"))
        }
        d.id = alchemy_device_api::device_id(&self.adapter_id, &d.id);
        Ok(d)
    }
}
impl Host for Bridge {
    fn discover(&self, devices: Vec<DeviceDescriptor>) -> Result<()> {
        let devices = devices
            .into_iter()
            .map(|d| self.descriptor(d))
            .collect::<Result<Vec<_>>>()?;
        self.manager()?.discovered_for(&self.adapter_id, devices)
    }
    fn connected(&self, device: DeviceDescriptor, path: String) -> Result<Connection> {
        let descriptor = self.descriptor(device)?;
        let id = descriptor.id.clone();
        let core = self.core(&id)?;
        if core.profile_error.is_some() {
            bail!(alchemy_device_api::UiFailure::new("backend.b285", "配置不可用"))
        }
        core.attach_input_device(descriptor, path);
        if core.supports_pages {
            let _ = core.enqueue(HardwareCommand::Firmware, None);
            let runtime = core.clone();
            tauri::async_runtime::spawn(async move {
                let page = runtime.snapshot().active_page;
                if let Err(e) = runtime
                    .activate_from(&page, crate::core::ActivationSource::Reconnect)
                    .await
                {
                    runtime.log("error", e.to_string());
                }
            });
        }
        connection(&core).context(alchemy_device_api::UiFailure::new("backend.b286", "设备未连接"))
    }
    fn disconnected(&self, token: &Connection) {
        if let Ok(core) = self.core(&token.device_id) {
            core.disconnect_session(token);
        }
    }
    fn connections(&self) -> Vec<Connection> {
        self.manager()
            .map(|m| {
                m.runtimes_for(&self.adapter_id)
                    .iter()
                    .filter_map(|r| connection(&r.core))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn input(&self, token: &Connection, input: InputValue) {
        if let Ok(core) = self.core(&token.device_id) {
            core.dispatch_input(token, input);
        }
    }
    fn next_batch(&self, cursor: &mut usize) -> Option<TransportBatch> {
        let (core, work) = self.manager().ok()?.next_batch(&self.adapter_id, cursor)?;
        let token = connection(&core)?;
        let id = uuid::Uuid::new_v4().to_string();
        let batch = TransportBatch {
            id: id.clone(),
            connection: token,
            page: work.first()?.page_batch.is_some(),
            work: work
                .iter()
                .map(|w| TransportWork {
                    command: w.command.clone(),
                    led_command: w.led_command.clone(),
                    image_frame: w
                        .image_frame
                        .clone()
                        .or_else(|| w.widget_frame.as_ref().map(|f| f.png.clone())),
                })
                .collect(),
        };
        self.inflight.lock().unwrap().insert(id, (core, work));
        Some(batch)
    }
    fn current(&self, id: &str) -> bool {
        if self
            .manager()
            .map(|m| m.cancel.is_cancelled())
            .unwrap_or(true)
        {
            return false;
        }
        self.inflight
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|(core, works)| works.iter().all(|w| core.work_is_current(w)))
    }
    fn complete(&self, id: &str, result: Result<()>) {
        if let Some((core, works)) = self.inflight.lock().unwrap().remove(id) {
            let count = works.iter().filter(|w| w.widget_frame.is_some()).count() as u64;
            if count > 0 {
                let mut inner = core.inner.lock().unwrap();
                if result.is_ok() {
                    inner.widgets.submitted += count
                } else {
                    inner.widgets.failures += count
                }
            }
            for work in &works {
                if let Some(id) = &work.receipt_id {
                    match &result {
                        Ok(()) => {
                            core.receipt(id, "sent", "操作已发送");
                            core.receipt(id, "acknowledged", "设备接口已接受此操作");
                        }
                        Err(e) => core.receipt(
                            id,
                            if core.work_is_current(work) {
                                "failed"
                            } else {
                                "cancelled"
                            },
                            e.to_string(),
                        ),
                    }
                }
            }
        }
    }
    fn connection_current(&self, token: &Connection) -> bool {
        self.core(&token.device_id)
            .ok()
            .is_some_and(|core| connection(&core).as_ref() == Some(token) && !*core.stop.borrow())
    }
    fn module_config(&self, token: &Connection) -> Result<serde_json::Value> {
        if !self.connection_current(token) {
            bail!(alchemy_device_api::UiFailure::new("backend.b287", "设备连接已改变"))
        }
        let core = self.core(&token.device_id)?;
        let workspace = storage::load_workspace(&core.root)?;
        Ok(workspace
            .profiles
            .iter()
            .find(|p| Some(&p.id) == core.profile_id.as_ref())
            .context(alchemy_device_api::UiFailure::new("backend.b041", "未知设备配置"))?
            .module_data
            .clone())
    }
    fn save_module_config(&self, token: &Connection, value: serde_json::Value) -> Result<()> {
        let core = self.core(&token.device_id)?;
        let inner = core.inner.lock().unwrap();
        if inner.connection_epoch != token.epoch
            || inner.paths.get(&token.device_id) != Some(&token.path)
            || *core.stop.borrow()
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b287", "设备连接已改变"))
        }
        if core.profile_error.is_some() {
            bail!(alchemy_device_api::UiFailure::new("backend.b285", "配置不可用"))
        }
        let adapter = core.adapter.get().context(alchemy_device_api::UiFailure::new("backend.b288", "设备模块未启用"))?;
        storage::validate_device_config(&inner.snapshot.config, &core.definition())?;
        adapter.validate(&inner.snapshot.config)?;
        adapter.validate_module(&value)?;
        for id in adapter.asset_references(&value)? {
            storage::asset(&core.root, &id)?;
        }
        storage::edit_workspace(&core.root, |workspace| {
            let profile = workspace
                .profiles
                .iter_mut()
                .find(|p| Some(&p.id) == core.profile_id.as_ref())
                .context(alchemy_device_api::UiFailure::new("backend.b041", "未知设备配置"))?;
            if profile.config_version != 1 || profile.schema_version != adapter.config_version() {
                bail!(alchemy_device_api::UiFailure::new("backend.b042", "配置版本不支持"))
            }
            profile.module_data = value;
            Ok(())
        })?;
        drop(inner);
        core.emit();
        Ok(())
    }
    fn retry_requests(&self) -> Vec<String> {
        self.manager()
            .map(|m| m.take_retries_for(&self.adapter_id))
            .unwrap_or_default()
    }
    fn tick(&self) {
        if let Ok(m) = self.manager() {
            for runtime in m.runtimes_for(&self.adapter_id) {
                runtime.core.tick_led_effect();
            }
        }
    }
    fn log(&self, level: &str, message: String) {
        if let Ok(m) = self.manager() {
            m.host.log(level, message);
        }
    }
    fn state(&self, token: &Connection, value: serde_json::Value) -> Result<()> {
        let core = self.core(&token.device_id)?;
        let mut inner = core.inner.lock().unwrap();
        if inner.connection_epoch != token.epoch
            || inner.paths.get(&token.device_id) != Some(&token.path)
            || *core.stop.borrow()
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b287", "设备连接已改变"))
        }
        inner.module_state = value;
        drop(inner);
        core.emit();
        Ok(())
    }
    fn backend_status(&self, state: &str, error: Option<String>) {
        if let Ok(m) = self.manager() {
            for runtime in m.runtimes_for(&self.adapter_id) {
                if state == "error" {
                    runtime.core.clear_connection();
                }
                runtime.core.connection(state, error.clone());
            }
            if let Some(e) = error {
                m.host.log("error", e);
            }
        }
    }
    fn cached_value(&self, token: &Connection, key: &str, value: String) -> Result<()> {
        let core = self.core(&token.device_id)?;
        let mut inner = core.inner.lock().unwrap();
        if inner.connection_epoch != token.epoch
            || inner.paths.get(&token.device_id) != Some(&token.path)
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b287", "设备连接已改变"))
        }
        match key {
            "gif_status" => inner.snapshot.gif_status = Some(value),
            "firmware" | "serial" => {
                for device in &mut inner.snapshot.devices {
                    if device.id == token.device_id {
                        if key == "firmware" {
                            device.firmware = value.clone();
                        } else {
                            device.serial = value.clone();
                        }
                    }
                }
            }
            _ => bail!(alchemy_device_api::UiFailure::new("backend.b289", "未知设备信息字段")),
        }
        drop(inner);
        core.emit();
        Ok(())
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        for (_, (core, works)) in self.inflight.get_mut().unwrap().iter() {
            for work in works {
                if let Some(id) = &work.receipt_id {
                    core.receipt(id, "cancelled", "设备后端已停止");
                }
            }
        }
    }
}
