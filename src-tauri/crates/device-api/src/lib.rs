//! Source-level device extension contract. No application/runtime dependency.
pub mod assets;
pub mod led;
pub mod model;
pub mod ownership;
pub mod scenes;
use anyhow::{bail, Result};
pub use model::*;
use serde_json::Value;
use std::{future::Future, path::PathBuf, pin::Pin, sync::Arc};
use tokio_util::sync::CancellationToken;

pub type AdapterFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
pub type ServiceFuture<'a> = Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub device_id: String,
    pub path: String,
    pub epoch: u64,
}
#[derive(Clone)]
pub struct TransportWork {
    pub command: HardwareCommand,
    pub image_frame: Option<Arc<Vec<u8>>>,
    pub led_command: Option<HardwareCommand>,
}
pub struct TransportBatch {
    pub id: String,
    pub connection: Connection,
    pub page: bool,
    pub work: Vec<TransportWork>,
}
#[derive(Clone)]
pub struct RawInputSpec {
    pub adapter_id: &'static str,
    pub name: &'static str,
    pub vendor: u16,
    pub product: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub owner_mutex: Option<&'static str>,
    pub decode: fn(&[u8]) -> Vec<InputValue>,
}
impl RawInputSpec {
    pub fn matches(&self, vendor: u32, product: u32, page: u16, usage: u16) -> bool {
        (vendor, product, page, usage)
            == (
                self.vendor.into(),
                self.product.into(),
                self.usage_page,
                self.usage,
            )
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputValue {
    Button { control_id: String, pressed: bool },
    Rotary { control_id: String, delta: i32 },
    Analog { control_id: String, value: f64 },
}
/// Host-issued scoped operations. Implementations are private to the application.
pub trait Host: Send + Sync {
    fn discover(&self, devices: Vec<DeviceDescriptor>) -> Result<()>;
    fn connected(&self, device: DeviceDescriptor, path: String) -> Result<Connection>;
    fn disconnected(&self, connection: &Connection);
    fn connections(&self) -> Vec<Connection>;
    fn input(&self, connection: &Connection, input: InputValue);
    fn next_batch(&self, cursor: &mut usize) -> Option<TransportBatch>;
    fn current(&self, batch: &str) -> bool;
    fn complete(&self, batch: &str, result: Result<()>);
    fn connection_current(&self, connection: &Connection) -> bool;
    fn retry_requests(&self) -> Vec<String>;
    fn module_config(&self, connection: &Connection) -> Result<Value>;
    fn save_module_config(&self, connection: &Connection, value: Value) -> Result<()>;
    fn tick(&self);
    fn log(&self, level: &str, message: String);
    fn state(&self, connection: &Connection, value: Value) -> Result<()>;
    fn backend_status(&self, state: &str, error: Option<String>);
    fn cached_value(&self, connection: &Connection, key: &str, value: String) -> Result<()>;
}
#[derive(Clone)]
pub struct AdapterHost {
    pub api: Arc<dyn Host>,
    pub root: PathBuf,
    pub resource: Option<PathBuf>,
    pub cancel: CancellationToken,
}
pub trait DeviceAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn layout(&self) -> DeviceLayout;
    fn definition(&self) -> DeviceDefinition {
        let layout = self.layout();
        DeviceDefinition {
            adapter_id: self.id().into(),
            name: self.id().into(),
            supports_pages: false,
            inputs: vec![],
            displays: vec![],
            input_names: layout
                .input_keys
                .iter()
                .map(|key| (*key, format!("按键 {key}")))
                .collect(),
            layout,
        }
    }
    fn public_types(&self) -> Vec<String> {
        vec![]
    }
    fn operations(&self) -> &'static [&'static str] {
        &[]
    }
    fn raw_input(&self) -> Vec<RawInputSpec> {
        vec![]
    }
    fn default_config(&self) -> AppConfig;
    fn new_page(&self, name: &str) -> Page {
        default_page(name, &self.definition().normalized())
    }
    fn validate(&self, _config: &AppConfig) -> Result<()> {
        Ok(())
    }
    fn default_module_config(&self) -> Value {
        serde_json::json!({})
    }
    fn config_version(&self) -> u32 {
        1
    }
    fn batch_limits(&self) -> BatchLimits {
        BatchLimits::default()
    }
    fn validate_module(&self, data: &Value) -> Result<()> {
        if !data.is_object() {
            bail!(crate::UiFailure::new("backend.b336", "模块配置须为对象"))
        }
        Ok(())
    }
    /// Convert exactly one version: `from` to `from + 1`. The host chains steps.
    fn migrate_module(&self, from: u32, data: Value) -> Result<Value> {
        if from != self.config_version() {
            bail!(crate::UiFailure::new("backend.b337", "模块配置版本不支持"))
        }
        Ok(data)
    }
    fn asset_references(&self, _data: &Value) -> Result<Vec<String>> {
        Ok(vec![])
    }
    fn run(self: Arc<Self>, host: AdapterHost) -> AdapterFuture;
    fn request<'a>(
        &'a self,
        _host: AdapterHost,
        _connection: Connection,
        _operation: String,
        _payload: Value,
    ) -> ServiceFuture<'a> {
        Box::pin(async { bail!(crate::UiFailure::new("backend.b324", "模块操作不支持")) })
    }
}
pub trait DeviceSession {
    type Prepared;
    fn prepare_batch(
        &self,
        work: &[TransportWork],
        root: &std::path::Path,
        cancel: &CancellationToken,
    ) -> Result<Self::Prepared>;
    fn submit_batch(
        &mut self,
        prepared: Self::Prepared,
        page: bool,
        host: &AdapterHost,
    ) -> Result<()>;
    fn close(self)
    where
        Self: Sized,
    {
        drop(self);
    }
}
pub fn device_id(adapter_id: &str, local_id: &str) -> String {
    use base64::Engine;
    format!(
        "{}/{}",
        adapter_id,
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(local_id)
    )
}

pub fn default_page(name: &str, definition: &DeviceDefinition) -> Page {
    Page {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        background_asset: None,
        bindings: definition
            .clone()
            .normalized()
            .inputs
            .iter()
            .filter(|i| matches!(i.kind, InputKind::Button))
            .map(|i| ControlBinding {
                control_id: i.id.clone(),
                enabled: true,
                trigger: InputTrigger::Press,
                action: KeyAction::None,
            })
            .collect(),
        keys: definition
            .layout
            .input_keys
            .iter()
            .chain(definition.layout.display_keys.iter())
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .iter()
            .map(|key| KeyVisual {
                content_mode: None,
                show_text: true,
                text_position: TextPosition::Center,
                key: *key,
                text: definition
                    .input_names
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| key.to_string()),
                foreground: "#F8FAFC".into(),
                background: "#252936".into(),
                asset_id: None,
                plugin_icon: None,
                default_icon_asset_id: None,
                widget: None,
            })
            .collect(),
    }
}

#[cfg(windows)]
pub mod identity;

pub mod controls;
pub use controls::*;

pub mod external;
pub use external::*;

#[derive(Clone, Copy)]
pub struct BatchLimits {
    pub operations: usize,
    pub bytes: usize,
}
impl Default for BatchLimits {
    fn default() -> Self {
        Self {
            operations: MAX_PAGE_OPERATIONS,
            bytes: MAX_PAGE_BYTES,
        }
    }
}
