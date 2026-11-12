//! Stable external DTOs; internal workspace state never crosses this boundary.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;
pub const PLUGIN_API_VERSION: u32 = 3;
pub const HOST_API_VERSION: u32 = 1;
pub const METHODS: &[&str] = &[
    "ping",
    "events.subscribe",
    "devices.list",
    "pages.list",
    "pages.activate",
    "display.key",
    "display.background",
    "device.command",
    "assets.import",
    "icons.list",
    "icons.publish",
    "icons.remove",
    "widgets.publish",
    "widgets.publish_batch",
    "widgets.sources.publish",
];
pub const EVENTS: &[&str] = &[
    "input",
    "device.connected",
    "device.disconnected",
    "button.down",
    "button.up",
    "page.changed",
];
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DeviceDto {
    pub device_id: Option<String>,
    pub adapter_id: String,
    pub name: String,
    pub connection: String,
    pub error: Option<String>,
    pub definition: DeviceDefinition,
    pub capabilities: DeviceCapabilities,
    pub inputs: BTreeMap<String, InputValue>,
    pub firmware: Option<String>,
    pub gif_status: Option<String>,
}
impl DeviceDto {
    pub fn from_state(device: &DeviceState) -> Self {
        Self {
            device_id: device.device_id.clone(),
            adapter_id: device.adapter_id.clone(),
            name: device.name.clone(),
            connection: device.state.connection.clone(),
            error: device
                .config_error
                .clone()
                .or_else(|| device.state.error.clone()),
            definition: device.definition.clone(),
            capabilities: {
                let mut caps = device
                    .state
                    .devices
                    .first()
                    .map(|d| d.capabilities.clone())
                    .unwrap_or_default();
                for id in &device.state.config.disabled_capabilities {
                    if let Some(cap) = caps.get_mut(id) {
                        cap.available = false;
                    }
                }
                caps
            },
            inputs: device.state.input_values.clone(),
            firmware: device.state.devices.first().map(|d| d.firmware.clone()),
            gif_status: device.state.gif_status.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArgument,
    NotFound,
    Offline,
    Unsupported,
    ConfigUnavailable,
    ResourceExhausted,
    StaleOperation,
    VersionUnsupported,
    Internal,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct ApiError {
    #[serde(skip)]
    #[ts(optional)]
    pub ui_message: Option<crate::UiMessage>,
    pub code: ErrorCode,
    pub message: String,
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ApiError {}
impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            ui_message: None,
            code,
            message: message.into(),
        }
    }
    pub fn localized(code: ErrorCode, key: &str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            ui_message: Some(crate::UiMessage::new(key)),
        }
    }
    pub fn from_error(error: &anyhow::Error) -> Self {
        error
            .downcast_ref::<Self>()
            .cloned()
            .unwrap_or_else(|| Self::new(ErrorCode::Internal, error.to_string()))
    }
}

pub fn strict<T: serde::de::DeserializeOwned + Serialize>(
    value: serde_json::Value,
) -> anyhow::Result<T> {
    let result: T = serde_json::from_value(value.clone())
        .map_err(|e| ApiError::new(ErrorCode::InvalidArgument, e.to_string()))?;
    fn check(actual: &serde_json::Value, known: &serde_json::Value) -> anyhow::Result<()> {
        match (actual, known) {
            (serde_json::Value::Object(actual), serde_json::Value::Object(known)) => {
                for (key, value) in actual {
                    let field = known.get(key).ok_or_else(|| {
                        ApiError::new(ErrorCode::InvalidArgument, format!("未知参数：{key}"))
                    })?;
                    check(value, field)?;
                }
            }
            (serde_json::Value::Array(actual), serde_json::Value::Array(known)) => {
                for (a, k) in actual.iter().zip(known) {
                    check(a, k)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    check(&value, &serde_json::to_value(&result)?)?;
    Ok(result)
}

/// Separate from HardwareCommand so future internal additions are never exposed automatically.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceCommand {
    /// Screen brightness 0–100.
    ScreenBrightness {
        value: u8,
    },
    /// RGB-scaled LED brightness 0–255; unavailable in firmware-default LED mode.
    LedBrightness {
        value: u8,
    },
    /// Unscaled RGB hex colour, #RRGGBB.
    LedColor {
        color: String,
    },
    /// RGB triples in the target device LED order, each component 0–255.
    LedColors {
        colors: Vec<[u8; 3]>,
    },
    ResetLed,
    /// A display key from the target device layout and an imported asset ID.
    KeyImage {
        key: u8,
        asset_id: String,
    },
    KeyData {
        key: u8,
        data: String,
    },
    /// Same asset on each key, not a composite image for the whole device.
    BulkImage {
        keys: Vec<u8>,
        asset_id: String,
    },
    /// Temporary background only; persistent startup background is unavailable.
    Background {
        asset_id: String,
    },
    BackgroundData {
        data: String,
    },
    ClearKey {
        key: u8,
    },
    ClearAll,
    Refresh,
    KeyGif {
        key: u8,
        asset_id: String,
    },
    ClearGif {
        key: u8,
    },
    StartGif,
    StopGif,
    GifStatus,
    Firmware,
}
impl From<DeviceCommand> for HardwareCommand {
    fn from(c: DeviceCommand) -> Self {
        match c {
            DeviceCommand::ScreenBrightness { value } => Self::ScreenBrightness { value },
            DeviceCommand::LedBrightness { value } => Self::LedBrightness { value },
            DeviceCommand::LedColor { color } => Self::LedColor { color },
            DeviceCommand::LedColors { colors } => Self::LedColors { colors },
            DeviceCommand::ResetLed => Self::ResetLed,
            DeviceCommand::KeyImage { key, asset_id } => Self::KeyImage { key, asset_id },
            DeviceCommand::KeyData { key, data } => Self::KeyData { key, data },
            DeviceCommand::BulkImage { keys, asset_id } => Self::BulkImage { keys, asset_id },
            DeviceCommand::Background { asset_id } => Self::Background {
                asset_id,
                persistent: false,
            },
            DeviceCommand::BackgroundData { data } => Self::BackgroundData {
                data,
                persistent: false,
            },
            DeviceCommand::ClearKey { key } => Self::ClearKey { key },
            DeviceCommand::ClearAll => Self::ClearAll,
            DeviceCommand::Refresh => Self::Refresh,
            DeviceCommand::KeyGif { key, asset_id } => Self::KeyGif { key, asset_id },
            DeviceCommand::ClearGif { key } => Self::ClearGif { key },
            DeviceCommand::StartGif => Self::StartGif,
            DeviceCommand::StopGif => Self::StopGif,
            DeviceCommand::GifStatus => Self::GifStatus,
            DeviceCommand::Firmware => Self::Firmware,
        }
    }
}

/// Temporary display request, independent of persistent page/editor fields.
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisplayVisual {
    pub key: u8,
    pub text: String,
    pub foreground: String,
    pub background: String,
    pub asset_id: Option<String>,
    pub plugin_icon: Option<PluginIconRef>,
}
impl From<DisplayVisual> for KeyVisual {
    fn from(v: DisplayVisual) -> Self {
        Self {
            content_mode: None,
            show_text: true,
            text_position: crate::TextPosition::Center,
            key: v.key,
            text: v.text,
            foreground: v.foreground,
            background: v.background,
            asset_id: v.asset_id,
            plugin_icon: v.plugin_icon,
            default_icon_asset_id: None,
            widget: None,
        }
    }
}
