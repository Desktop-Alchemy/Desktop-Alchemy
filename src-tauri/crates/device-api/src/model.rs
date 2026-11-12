pub use crate::scenes::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrayAction {
    Device,
    Pages,
    Plugins,
    Show,
    Settings,
    Exit,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq, schemars::JsonSchema)]
pub struct TrayState {
    pub connection: String,
    pub theme: String,
}
impl From<&Snapshot> for TrayState {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            connection: snapshot.connection.clone(),
            theme: snapshot.config.theme.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    Unverified,
    Verified,
    Unsupported,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct Capability {
    pub available: bool,
    pub verification: Verification,
    pub reason: String,
}
pub type DeviceCapabilities = BTreeMap<String, Capability>;
/// Optional features; neither a display nor lighting is required by the host.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DeviceLayout {
    pub input_keys: Vec<u8>,
    pub display_keys: Vec<u8>,
    pub columns: u16,
    pub key_width: u16,
    pub key_height: u16,
    pub background_width: u16,
    pub background_height: u16,
    pub led_count: u16,
}
#[derive(Clone, Debug, TS, schemars::JsonSchema)]
pub struct DeviceProfile {
    pub id: String,
    pub adapter_id: String,
    pub schema_version: u32,
    pub name: String,
    pub device_id: Option<String>,
    pub config: AppConfig,
    pub config_version: u32,
    pub module_data: serde_json::Value,
    #[ts(skip)]
    pub raw: serde_json::Value,
    #[ts(skip)]
    pub config_error: Option<String>,
}
impl DeviceProfile {
    pub fn new(
        adapter_id: String,
        name: String,
        device_id: Option<String>,
        config: AppConfig,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            adapter_id,
            schema_version: 1,
            name,
            device_id,
            config,
            config_version: 1,
            module_data: serde_json::json!({}),
            raw: serde_json::Value::Null,
            config_error: None,
        }
    }
}
impl Serialize for DeviceProfile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.config_error.is_some() {
            return self.raw.serialize(serializer);
        }
        if !self.raw.is_null() {
            if let Ok(before) = serde_json::from_value::<Self>(self.raw.clone()) {
                if self.id == before.id
                    && self.adapter_id == before.adapter_id
                    && self.name == before.name
                    && self.device_id == before.device_id
                    && self.config_version == before.config_version
                    && self.schema_version == before.schema_version
                    && self.module_data == before.module_data
                    && serde_json::to_value(&self.config).map_err(serde::ser::Error::custom)?
                        == serde_json::to_value(&before.config)
                            .map_err(serde::ser::Error::custom)?
                {
                    return self.raw.serialize(serializer);
                }
            }
        }
        let mut raw = self.raw.clone();
        if !raw.is_object() {
            raw = serde_json::json!({});
        }
        for (key, value) in [
            ("id", serde_json::json!(self.id)),
            ("adapter_id", serde_json::json!(self.adapter_id)),
            ("schema_version", serde_json::json!(self.schema_version)),
            ("name", serde_json::json!(self.name)),
            ("device_id", serde_json::json!(self.device_id)),
            ("config_version", serde_json::json!(self.config_version)),
            ("module_data", self.module_data.clone()),
        ] {
            raw[key] = value;
        }
        if self.config_error.is_none() {
            merge_preserving_unknown(
                &mut raw["config"],
                serde_json::to_value(&self.config).map_err(serde::ser::Error::custom)?,
            );
        }
        raw.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for DeviceProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(deserializer)?;
        use sha2::{Digest, Sha256};
        let fallback_id = format!(
            "invalid-{:x}",
            Sha256::digest(serde_json::to_vec(&raw).map_err(serde::de::Error::custom)?)
        );
        let envelope_error = if ["id", "adapter_id", "name"]
            .iter()
            .any(|key| raw[key].as_str().is_none_or(str::is_empty))
            || (!raw["device_id"].is_null() && !raw["device_id"].is_string())
        {
            Some("设备配置外壳字段无效".to_owned())
        } else {
            None
        };
        let text = |key: &str, fallback: &str| {
            raw[key]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback)
                .to_owned()
        };
        let parsed = serde_json::from_value::<AppConfig>(raw["config"].clone());
        let (config, config_error) = match parsed {
            Ok(c) => (c, None),
            Err(e) => (AppConfig::empty(), Some(e.to_string())),
        };
        Ok(Self {
            id: text("id", &fallback_id),
            adapter_id: text("adapter_id", "invalid.module"),
            name: text("name", "不可用配置"),
            device_id: raw["device_id"].as_str().map(str::to_owned),
            schema_version: raw["schema_version"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
            config_version: raw["config_version"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
            module_data: raw
                .get("module_data")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            config,
            config_error: config_error.or(envelope_error),
            raw,
        })
    }
}
/// Merge known fields while retaining future fields. Stable IDs prevent deleted
/// or reordered pages/controls from inheriting another object's extension data.
pub fn merge_preserving_unknown(original: &mut serde_json::Value, current: serde_json::Value) {
    match (original, current) {
        (serde_json::Value::Object(old), serde_json::Value::Object(new)) => {
            if old.get("type") != new.get("type") {
                *old = new;
                return;
            }
            for (key, value) in new {
                merge_preserving_unknown(old.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (serde_json::Value::Array(old), serde_json::Value::Array(new)) => {
            let mut previous = std::mem::take(old);
            *old = new
                .into_iter()
                .map(|value| {
                    let key = ["id", "key", "control_id"]
                        .into_iter()
                        .find(|key| value.get(key).is_some());
                    let mut base = key
                        .and_then(|key| {
                            previous.iter().position(|item| {
                                item.get(key) == value.get(key)
                                    && (key != "control_id"
                                        || item.pointer("/trigger/type")
                                            == value.pointer("/trigger/type"))
                            })
                        })
                        .map(|index| previous.remove(index))
                        .unwrap_or(serde_json::Value::Null);
                    merge_preserving_unknown(&mut base, value);
                    base
                })
                .collect();
        }
        (old, new) => *old = new,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct GlobalConfig {
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub theme: String,
    pub managed_plugin_ids: Vec<String>,
    pub disabled_plugins: Vec<String>,
}
impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            extensions: Default::default(),
            theme: "system".into(),
            managed_plugin_ids: vec![],
            disabled_plugins: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WorkspaceConfig {
    pub format: String,
    pub version: u32,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub global: GlobalConfig,
    pub profiles: Vec<DeviceProfile>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DeviceState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<UiMessage>,
    #[serde(default)]
    pub module_state: serde_json::Value,
    pub module_available: bool,
    pub config_error: Option<String>,
    pub definition: DeviceDefinition,
    pub profile_id: String,
    pub adapter_id: String,
    pub name: String,
    pub device_id: Option<String>,
    pub layout: DeviceLayout,
    pub state: Snapshot,
    pub scene_status: SceneStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WorkspaceSnapshot {
    pub global: GlobalConfig,
    pub devices: Vec<DeviceState>,
    pub discovered: Vec<DeviceDescriptor>,
    pub plugins: Vec<PluginInfo>,
    pub logs: Vec<LogEntry>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DeviceDescriptor {
    #[serde(default)]
    pub adapter_id: String,
    #[serde(default)]
    pub layout: Box<DeviceLayout>,
    pub id: String,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub vendor_id: u32,
    pub product_id: u32,
    pub capabilities: DeviceCapabilities,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DeviceEvent {
    pub input: Option<crate::InputValue>,
    pub at: f64,
    pub kind: String,
    pub device_id: String,
    pub key: Option<u8>,
    pub pressed: Option<bool>,
    pub page_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HardwareCommand {
    ScreenBrightness { value: u8 },
    LedBrightness { value: u8 },
    LedColor { color: String },
    LedColors { colors: Vec<[u8; 3]> },
    ResetLed,
    KeyImage { key: u8, asset_id: String },
    KeyData { key: u8, data: String },
    BulkImage { keys: Vec<u8>, asset_id: String },
    Background { asset_id: String, persistent: bool },
    BackgroundData { data: String, persistent: bool },
    ClearKey { key: u8 },
    ClearAll,
    Refresh,
    KeyGif { key: u8, asset_id: String },
    ClearGif { key: u8 },
    StartGif,
    StopGif,
    GifStatus,
    Firmware,
}
impl HardwareCommand {
    pub fn capability(&self) -> &'static str {
        match self {
            Self::ScreenBrightness { .. } => "screen_brightness",
            Self::LedBrightness { .. } => "led_brightness",
            Self::LedColor { .. } | Self::ResetLed => "led_color",
            Self::LedColors { .. } => "individual_leds",
            Self::KeyGif { .. }
            | Self::ClearGif { .. }
            | Self::StartGif
            | Self::StopGif
            | Self::GifStatus => "key_gif",
            Self::Background {
                persistent: true, ..
            }
            | Self::BackgroundData {
                persistent: true, ..
            } => "startup_background",
            Self::Background { .. } | Self::BackgroundData { .. } => "temporary_background",
            Self::KeyImage { .. }
            | Self::KeyData { .. }
            | Self::BulkImage { .. }
            | Self::ClearAll
            | Self::ClearKey { .. } => "key_images",
            _ => "device_control",
        }
    }
    pub fn visual(&self) -> bool {
        matches!(
            self,
            Self::KeyImage { .. }
                | Self::KeyData { .. }
                | Self::BulkImage { .. }
                | Self::Background {
                    persistent: false,
                    ..
                }
                | Self::BackgroundData {
                    persistent: false,
                    ..
                }
                | Self::ClearKey { .. }
                | Self::ClearAll
                | Self::KeyGif { .. }
                | Self::ClearGif { .. }
                | Self::StartGif
                | Self::StopGif
                | Self::Refresh
        )
    }
    pub fn merge_key(&self) -> Option<&'static str> {
        match self {
            Self::ScreenBrightness { .. } => Some("screen_brightness"),
            Self::LedBrightness { .. }
            | Self::LedColor { .. }
            | Self::LedColors { .. }
            | Self::ResetLed => Some("led_state"),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegisteredIdentity {
    AppsFolder { parsing_name: String },
    Aumid { id: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LaunchTarget {
    Program {
        path: String,
        arguments: String,
        working_directory: Option<String>,
    },
    RegisteredApplication {
        identity: RegisteredIdentity,
        arguments: String,
    },
    Uri {
        uri: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DeviceDefinition {
    #[serde(default)]
    pub inputs: Vec<crate::InputControl>,
    #[serde(default)]
    pub displays: Vec<crate::DisplayRegion>,
    pub adapter_id: String,
    pub name: String,
    pub layout: DeviceLayout,
    pub supports_pages: bool,
    pub input_names: BTreeMap<u8, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyAction {
    Launch { target: LaunchTarget },
    None,
    Previous,
    Home,
    Next,
    Page { page_id: String },
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    Tray,
    Exit,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(default)]
pub struct AppSettings {
    pub language: LanguagePreference,
    pub close_behavior: CloseBehavior,
    pub model_appearances: BTreeMap<String, ModelAppearance>,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema,
)]
pub enum LanguagePreference {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en")]
    English,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub enum UiLocale {
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en")]
    English,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct UiLanguageState {
    pub preference: LanguagePreference,
    pub locale: UiLocale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct UiMessage {
    pub key: String,
    pub args: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct UiError {
    pub message: UiMessage,
    pub detail: String,
}
impl From<String> for UiError {
    fn from(detail: String) -> Self {
        Self {
            message: UiMessage {
                key: "errors.operationFailed".into(),
                args: Default::default(),
            },
            detail,
        }
    }
}
impl From<&str> for UiError {
    fn from(detail: &str) -> Self {
        detail.to_owned().into()
    }
}
impl UiError {
    pub fn from_error(error: anyhow::Error) -> Self {
        let message = error
            .downcast_ref::<UiFailure>()
            .map(|e| e.message.clone())
            .or_else(|| {
                error
                    .downcast_ref::<crate::ApiError>()
                    .and_then(|e| e.ui_message.clone())
            })
            .unwrap_or_else(|| UiMessage::new("errors.operationFailed"));
        Self {
            message,
            detail: format!("{error:#}"),
        }
    }
    pub fn localized(key: &str, detail: impl Into<String>) -> Self {
        Self {
            message: UiMessage::new(key),
            detail: detail.into(),
        }
    }
}
impl UiMessage {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.into(),
            args: Default::default(),
        }
    }
}
/// Typed presentation metadata carried through anyhow without changing diagnostic text.
#[derive(Clone, Debug)]
pub struct UiFailure {
    pub message: UiMessage,
    pub detail: String,
}
impl UiFailure {
    pub fn new(key: &str, detail: impl Into<String>) -> Self {
        Self {
            message: UiMessage::new(key),
            detail: detail.into(),
        }
    }
    pub fn arg(mut self, name: &str, value: impl std::fmt::Display) -> Self {
        self.message
            .args
            .insert(name.into(), serde_json::Value::String(value.to_string()));
        self
    }
}
impl std::fmt::Display for UiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.detail.fmt(f)
    }
}
impl std::error::Error for UiFailure {}
impl From<UiFailure> for UiError {
    fn from(value: UiFailure) -> Self {
        Self {
            message: value.message,
            detail: value.detail,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelAppearance {
    #[default]
    Default,
    Text {
        text: String,
    },
    Image {
        image: ImageRef,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct BackupSummary {
    pub format_version: u32,
    pub app_version: String,
    pub created_at: f64,
    pub pages: u32,
    pub scenes: u32,
    pub images: u32,
    pub folders: u32,
    pub library_items: u32,
    pub applications: u32,
    pub plugins: u32,
    pub total_bytes: f64,
    /// SHA-256 of the exact archive reviewed by the user.
    pub fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct BackupRestoreNotice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<UiMessage>,
    pub restored: bool,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct LibraryFolder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct LibraryItem {
    pub id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub asset_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct AssetLibrary {
    pub version: u32,
    pub folders: Vec<LibraryFolder>,
    pub items: Vec<LibraryItem>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq, schemars::JsonSchema)]
pub struct PluginIconRef {
    pub plugin_id: String,
    pub icon_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageRef {
    Asset { asset_id: String },
    PluginIcon { plugin_id: String, icon_id: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct CatalogFolder {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub plugin_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct CatalogItem {
    pub id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub image: ImageRef,
    pub revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct LibraryCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub personal_revision: Option<String>,
    #[ts(type = "number")]
    pub revision: u64,
    pub folders: Vec<CatalogFolder>,
    pub items: Vec<CatalogItem>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct PluginIconDefinition {
    pub id: String,
    pub name: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct PluginIconInfo {
    pub id: String,
    pub name: String,
    pub origin: String,
    pub revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct RegisteredApplication {
    pub id: String,
    pub name: String,
    pub target: LaunchTarget,
    pub icon_asset_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct ShortcutTarget {
    pub path: String,
    pub arguments: String,
    pub working_directory: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct ApplicationEntry {
    pub id: String,
    pub name: String,
    pub target: LaunchTarget,
    pub icon_asset_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, TS, schemars::JsonSchema)]
pub struct ApplicationLibrary {
    pub version: u32,
    pub items: Vec<ApplicationEntry>,
}
impl<'de> Deserialize<'de> for ApplicationLibrary {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            version: u32,
            items: Vec<ApplicationEntry>,
        }
        let w = Wire::deserialize(d)?;
        if w.version != 2 {
            return Err(serde::de::Error::custom("不支持的应用预设版本"));
        }
        Ok(Self {
            version: 2,
            items: w.items,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApplicationEdit {
    RefreshIcon {
        id: String,
    },
    Save {
        id: Option<String>,
        name: String,
        target: LaunchTarget,
    },
    Delete {
        id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct LibraryImportResult {
    pub path: String,
    pub item: Option<LibraryItem>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LibraryEdit {
    CreateFolder {
        parent_id: Option<String>,
        name: String,
    },
    RenameFolder {
        id: String,
        name: String,
    },
    RenameItem {
        id: String,
        name: String,
    },
    Move {
        folder_ids: Vec<String>,
        item_ids: Vec<String>,
        parent_id: Option<String>,
    },
    Delete {
        folder_ids: Vec<String>,
        item_ids: Vec<String>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyContentMode {
    None,
    Text,
    Pattern,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TextPosition {
    Top,
    #[default]
    Center,
    Bottom,
}

fn default_show_text() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct KeyVisual {
    pub key: u8,
    pub text: String,
    /// Missing on existing pages: infer the base content without changing its appearance.
    #[serde(default)]
    pub content_mode: Option<KeyContentMode>,
    #[serde(default = "default_show_text")]
    pub show_text: bool,
    #[serde(default)]
    pub text_position: TextPosition,
    pub foreground: String,
    pub background: String,
    pub asset_id: Option<String>,
    #[serde(default)]
    pub plugin_icon: Option<PluginIconRef>,
    /// Last automatically applied app icon. Explicit image selection clears this.
    #[serde(default)]
    pub default_icon_asset_id: Option<String>,
    #[serde(default)]
    pub widget: Option<WidgetBinding>,
}

impl KeyVisual {
    pub fn content_mode(&self) -> KeyContentMode {
        self.content_mode.unwrap_or_else(|| {
            if self.asset_id.is_some() || self.plugin_icon.is_some() {
                KeyContentMode::Pattern
            } else if !self.text.is_empty() {
                KeyContentMode::Text
            } else {
                KeyContentMode::None
            }
        })
    }

    pub fn renders_image(&self) -> bool {
        self.content_mode() == KeyContentMode::Pattern
    }

    pub fn animated_asset(&self) -> Option<&str> {
        self.asset_id
            .as_deref()
            .filter(|id| self.renders_image() && id.ends_with(".gif"))
    }

    pub fn renders_text(&self) -> bool {
        self.content_mode() != KeyContentMode::None
            && self.show_text
            && self.animated_asset().is_none()
    }

    pub fn has_default_content(&self) -> bool {
        self.widget.is_none()
            && self.content_mode.is_none()
            && self.asset_id.is_none()
            && self.plugin_icon.is_none()
            && self.show_text
            && self.text_position == TextPosition::Center
            && (self.text.is_empty() || self.text == format!("{:02}", self.key))
    }
}

#[cfg(test)]
mod key_visual_tests {
    use super::*;

    #[test]
    fn existing_visuals_keep_their_text_and_infer_content_without_rewriting_data() {
        let legacy = serde_json::json!({"key":1,"text":"Chrome","foreground":"#FFFFFF","background":"#000000","asset_id":"icon.png"});
        let visual: KeyVisual = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(visual.content_mode, None);
        assert_eq!(visual.content_mode(), KeyContentMode::Pattern);
        assert!(visual.renders_text());
        assert_eq!(visual.text_position, TextPosition::Center);
        let mut hidden = visual.clone();
        hidden.content_mode = Some(KeyContentMode::None);
        hidden.show_text = false;
        hidden.text_position = TextPosition::Bottom;
        let restored: KeyVisual =
            serde_json::from_slice(&serde_json::to_vec(&hidden).unwrap()).unwrap();
        assert_eq!(restored.text, visual.text);
        assert_eq!(restored.asset_id, visual.asset_id);
        assert!(!restored.renders_image());
        assert!(!restored.renders_text());
        assert_eq!(restored.text_position, TextPosition::Bottom);
        for field in ["content_mode", "text_position"] {
            let mut invalid = legacy.clone();
            invalid[field] = serde_json::json!("invalid");
            assert!(serde_json::from_value::<KeyVisual>(invalid).is_err());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetStyle {
    pub foreground: String,
    pub background: String,
    pub accent: String,
    pub show_progress: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetBinding {
    pub plugin_id: String,
    pub widget_id: String,
    pub style: WidgetStyle,
    #[serde(default)]
    pub presentation_id: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub source_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetDefinition {
    pub id: String,
    pub name: String,
    pub default_style: WidgetStyle,
    #[serde(default)]
    pub presentations: Vec<WidgetPresentation>,
    #[serde(default)]
    pub select_source: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<UiMessage>,
    pub sources: Vec<WidgetSource>,
    pub hardware_status: String,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<UiMessage>,
    pub widget_id: String,
    pub id: String,
    pub name: String,
    pub available: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetPresentation {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetContent {
    pub title: String,
    pub value: String,
    #[serde(default)]
    pub secondary_value: Option<String>,
    pub detail: Option<String>,
    pub progress: Option<f64>,
    pub valid: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetData {
    pub widget_id: String,
    #[serde(default)]
    pub source_id: Option<String>,
    pub title: String,
    pub value: String,
    #[serde(default)]
    pub secondary_value: Option<String>,
    pub detail: Option<String>,
    pub progress: Option<f64>,
    pub valid: bool,
    #[serde(default)]
    pub presentations: BTreeMap<String, WidgetContent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct WidgetSample {
    pub plugin_id: String,
    pub data: WidgetData,
    pub updated_at: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct Page {
    #[serde(default)]
    pub bindings: Vec<crate::ControlBinding>,
    pub id: String,
    pub name: String,
    pub keys: Vec<KeyVisual>,
    pub background_asset: Option<String>,
}
impl Page {
    pub fn new(name: &str) -> Self {
        Self {
            bindings: vec![],
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            keys: vec![],
            background_asset: None,
        }
    }
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LedMode {
    #[default]
    Solid,
    Individual,
    Breathing,
    ColorCycle,
    Rainbow,
    Wave,
    Comet,
    DeviceDefault,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DeviceSettings {
    pub screen_brightness: u8,
    pub led_brightness: u8,
    pub led_color: String,
    #[serde(default)]
    pub led_mode: LedMode,
    #[serde(default = "default_led_speed")]
    pub led_speed: u8,
    #[serde(default = "default_led_colors")]
    pub led_colors: Vec<[u8; 3]>,
}
impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            screen_brightness: 0,
            led_brightness: 0,
            led_color: "#000000".into(),
            led_mode: LedMode::Solid,
            led_speed: 5,
            led_colors: vec![],
        }
    }
}
fn default_led_speed() -> u8 {
    5
}
fn default_led_colors() -> Vec<[u8; 3]> {
    vec![]
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct AppConfig {
    #[serde(default)]
    pub disabled_capabilities: Vec<String>,
    #[serde(default)]
    pub bindings: Vec<crate::ControlBinding>,
    #[serde(default)]
    pub scene_settings: SceneSettings,
    pub version: u32,
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default)]
    pub active_page: String,
    pub theme: String,
    #[serde(default)]
    pub device_settings: DeviceSettings,
    pub verification: BTreeMap<String, DeviceCapabilities>,
    #[serde(default)]
    pub managed_plugin_ids: Vec<String>,
    #[serde(default)]
    pub disabled_plugins: Vec<String>,
}
impl Default for AppConfig {
    fn default() -> Self {
        Self::empty()
    }
}
impl AppConfig {
    pub fn empty() -> Self {
        Self {
            bindings: vec![],
            disabled_capabilities: vec![],
            version: 1,
            scene_settings: SceneSettings::default(),
            active_page: String::new(),
            pages: vec![],
            theme: "system".into(),
            device_settings: DeviceSettings::default(),
            verification: BTreeMap::new(),
            managed_plugin_ids: vec![],
            disabled_plugins: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub executable: Option<String>,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub widgets: Vec<WidgetDefinition>,
    #[serde(default)]
    pub icons: Vec<PluginIconDefinition>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct PluginInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<UiMessage>,
    pub manifest: PluginManifest,
    pub path: String,
    pub status: String,
    pub error: Option<String>,
    #[serde(default)]
    pub built_in: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct PluginPackageSummary {
    pub manifest: PluginManifest,
    pub fingerprint: String,
    pub installed_version: Option<String>,
    pub total_bytes: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct LogEntry {
    pub at: f64,
    pub level: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct CommandReceipt {
    #[serde(default)]
    pub device_id: String,
    pub id: String,
    pub command: String,
    pub status: String,
    pub at: f64,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct Snapshot {
    #[serde(default)]
    pub revisions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<UiMessage>,
    #[serde(default)]
    pub input_values: BTreeMap<String, crate::InputValue>,
    pub active_page: String,
    pub connection: String,
    pub error: Option<String>,
    pub devices: Vec<DeviceDescriptor>,
    pub selected_device: Option<String>,
    pub config: AppConfig,
    pub pressed: Vec<u8>,
    #[serde(default)]
    pub last_key_press: Option<f64>,
    pub logs: Vec<LogEntry>,
    pub receipts: Vec<CommandReceipt>,
    pub plugins: Vec<PluginInfo>,
    pub sdk_pid: Option<u32>,
    pub gif_status: Option<String>,
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn unchanged_profiles_keep_exact_envelope_and_missing_optional_fields() {
        let mut raw = json!({"id":"future","adapter_id":"new.module","name":"Future","schema_version":99,
            "config_version":1,"config":serde_json::to_value(AppConfig::empty()).unwrap(),"extension":{"keep":true}});
        raw["config"].as_object_mut().unwrap().remove("bindings");
        let profile: DeviceProfile = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(profile).unwrap(), raw);
    }
    #[test]
    fn nested_unknown_binding_data_survives_reorder_and_action_changes() {
        let mut raw = json!([{"control_id":"dial","trigger":{"type":"clockwise"},"action":{"type":"page","page_id":"old"},"extension":1},
            {"control_id":"dial","trigger":{"type":"counterclockwise"},"action":{"type":"none"},"extension":2}]);
        merge_preserving_unknown(
            &mut raw,
            json!([
            {"control_id":"dial","trigger":{"type":"counterclockwise"},"action":{"type":"none"}},
            {"control_id":"dial","trigger":{"type":"clockwise"},"action":{"type":"next"}}]),
        );
        assert_eq!(raw[0]["extension"], 2);
        assert_eq!(raw[1]["extension"], 1);
        assert_eq!(raw[1]["action"], json!({"type":"next"}));
    }
}
