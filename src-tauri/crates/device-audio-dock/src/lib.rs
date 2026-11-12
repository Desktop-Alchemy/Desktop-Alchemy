use alchemy_device_api::*;
use anyhow::{bail, ensure, Result};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
#[cfg(windows)]
mod native;
pub mod protocol;
pub mod types;
pub const ID: &str = "microsoft.audio-dock";
#[derive(Default)]
pub struct Adapter {
    queues: Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>,
}
pub fn decode(report: &[u8]) -> Vec<InputValue> {
    if report.len() < 2 || report[0] != 0x9b {
        return vec![];
    }
    match report[1] {
        0 | 1 => vec![InputValue::Button {
            control_id: "1".into(),
            pressed: report[1] == 1,
        }],
        _ => vec![],
    }
}
impl DeviceAdapter for Adapter {
    fn public_types(&self) -> Vec<String> {
        use ts_rs::TS;
        use types::*;
        vec![
            AudioDockConnection::decl(),
            AudioDockNoiseSuppression::decl(),
            AudioDockEqualizerPreset::decl(),
            AudioDockDeviceInfo::decl(),
            AudioDockSettings::decl(),
            AudioDockState::decl(),
            AudioDockSettingsUpdate::decl(),
        ]
    }

    fn operations(&self) -> &'static [&'static str] {
        &["read", "update"]
    }
    fn id(&self) -> &'static str {
        ID
    }
    fn layout(&self) -> DeviceLayout {
        DeviceLayout {
            input_keys: vec![1],
            ..Default::default()
        }
    }
    fn definition(&self) -> DeviceDefinition {
        DeviceDefinition {
            inputs: vec![],
            displays: vec![],
            adapter_id: ID.into(),
            name: "Microsoft Audio Dock".into(),
            layout: self.layout(),
            supports_pages: false,
            input_names: BTreeMap::from([(1, "Teams 键".into())]),
        }
    }
    fn default_config(&self) -> AppConfig {
        AppConfig {
            bindings: vec![ControlBinding {
                control_id: "1".into(),
                enabled: false,
                trigger: InputTrigger::Press,
                action: KeyAction::None,
            }],
            ..AppConfig::empty()
        }
    }
    fn raw_input(&self) -> Vec<RawInputSpec> {
        vec![RawInputSpec {
            adapter_id: ID,
            name: "Microsoft Audio Dock",
            vendor: 0x045e,
            product: 0x084d,
            usage_page: 0xff99,
            usage: 1,
            owner_mutex: Some("Global\\MsAudioDockRemapper.SingleInstance"),
            decode,
        }]
    }
    fn run(self: Arc<Self>, host: AdapterHost) -> AdapterFuture {
        Box::pin(async move {
            let mut seen = BTreeMap::new();
            let mut reads = tokio::task::JoinSet::new();
            loop {
                for id in host.api.retry_requests() {
                    seen.remove(&id);
                }
                for connection in host.api.connections() {
                    if seen.get(&connection.device_id) != Some(&connection.epoch) {
                        seen.insert(connection.device_id.clone(), connection.epoch);
                        let adapter = self.clone();
                        let h = host.clone();
                        reads.spawn(async move {
                            let _ = adapter
                                .request(h, connection, "read".into(), serde_json::Value::Null)
                                .await;
                        });
                    }
                }
                tokio::select! {_=host.cancel.cancelled()=>{while reads.join_next().await.is_some(){} let queues=self.queues.lock().unwrap().values().cloned().collect::<Vec<_>>();for queue in queues{let _guard=queue.lock_owned().await;}return Ok(())},_=tokio::time::sleep(std::time::Duration::from_millis(250))=>{}}
            }
        })
    }
    fn request<'a>(
        &'a self,
        host: AdapterHost,
        connection: Connection,
        operation: String,
        payload: serde_json::Value,
    ) -> ServiceFuture<'a> {
        Box::pin(async move {
            let update = match operation.as_str() {
                "read" => {
                    ensure!(payload.is_null(), alchemy_device_api::UiFailure::new("backend.e030", "读取不接受参数"));
                    None
                }
                "update" => Some(serde_json::from_value::<types::AudioDockSettingsUpdate>(
                    payload,
                )?),
                _ => bail!(alchemy_device_api::UiFailure::new("backend.b324", "模块操作不支持")),
            };
            let queue = self
                .queues
                .lock()
                .unwrap()
                .entry(connection.device_id.clone())
                .or_default()
                .clone();
            let guard = queue.lock_owned().await;
            ensure!(host.api.connection_current(&connection), alchemy_device_api::UiFailure::new("backend.e031", "设备连接已改变"));
            host.api.state(
                &connection,
                serde_json::to_value(types::AudioDockState::unavailable(
                    connection.device_id.clone(),
                    types::AudioDockConnection::Loading,
                    None,
                ))?,
            )?;
            let h = host.clone();
            let token = connection.clone();
            let (result, _guard) = tokio::task::spawn_blocking(move || {
                let _guard = guard;
                let current = || h.api.connection_current(&token) && !h.cancel.is_cancelled();
                let result = (|| -> Result<types::AudioDockState> {
                    #[cfg(windows)]
                    {
                        let mut session = native::Session::open(&token.path, &current)?;
                        match update {
                            Some(update) => {
                                protocol::update(&mut session, &token.device_id, &update)
                            }
                            None => protocol::read(&mut session, &token.device_id),
                        }
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = (current, update);
                        bail!(alchemy_device_api::UiFailure::new("backend.b325", "Audio Dock 仅支持 Windows"))
                    }
                })();
                (result, _guard)
            })
            .await?;
            ensure!(host.api.connection_current(&connection), alchemy_device_api::UiFailure::new("backend.e031", "设备连接已改变"));
            match result {
                Ok(state) => {
                    if let Some(serial) = &state.info.serial {
                        host.api
                            .cached_value(&connection, "serial", serial.clone())?;
                    }
                    if let Some(firmware) = &state.info.firmware_version {
                        host.api
                            .cached_value(&connection, "firmware", firmware.clone())?;
                    }
                    let value = serde_json::to_value(state)?;
                    host.api.state(&connection, value.clone())?;
                    Ok(value)
                }
                Err(e) => {
                    let state = types::AudioDockState::unavailable(
                        connection.device_id.clone(),
                        types::AudioDockConnection::Error,
                        Some(e.to_string()),
                    );
                    host.api.state(&connection, serde_json::to_value(state)?)?;
                    Err(e)
                }
            }
        })
    }
}
