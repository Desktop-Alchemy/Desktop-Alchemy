use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioDockConnection {
    Loading,
    Ready,
    Offline,
    Busy,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioDockNoiseSuppression {
    Standard,
    Low,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema, PartialEq, Eq)]
pub struct AudioDockEqualizerPreset {
    pub id: u8,
    pub name: String,
    /// Low, mid-low, mid, mid-high, high; signed dB, -12..=12.
    pub gains: [i8; 5],
    pub factory: bool,
}

#[derive(
    Clone, Debug, Default, Serialize, Deserialize, TS, schemars::JsonSchema, PartialEq, Eq,
)]
pub struct AudioDockDeviceInfo {
    pub model: Option<String>,
    pub serial: Option<String>,
    pub hardware_version: Option<String>,
    pub color_code: Option<String>,
    pub firmware_version: Option<String>,
}

#[derive(
    Clone, Debug, Default, Serialize, Deserialize, TS, schemars::JsonSchema, PartialEq, Eq,
)]
pub struct AudioDockSettings {
    pub prompt_tone: Option<bool>,
    pub prompt_volume: Option<u8>,
    pub push_to_talk: Option<bool>,
    pub noise_suppression: Option<AudioDockNoiseSuppression>,
    pub active_equalizer: Option<u8>,
    pub equalizer_presets: Vec<AudioDockEqualizerPreset>,
    pub free_equalizer_slots: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct AudioDockState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<alchemy_device_api::UiMessage>,
    pub device_id: String,
    pub connection: AudioDockConnection,
    pub settings: AudioDockSettings,
    pub info: AudioDockDeviceInfo,
    pub error: Option<String>,
}
impl AudioDockState {
    pub fn unavailable(
        device_id: String,
        connection: AudioDockConnection,
        error: Option<String>,
    ) -> Self {
        Self {
            ui_message: error
                .as_ref()
                .map(|_| alchemy_device_api::UiMessage::new("errors.deviceFailed")),
            device_id,
            connection,
            settings: Default::default(),
            info: Default::default(),
            error,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum AudioDockSettingsUpdate {
    PromptTone {
        enabled: bool,
    },
    PromptVolume {
        value: u8,
    },
    PushToTalk {
        enabled: bool,
    },
    NoiseSuppression {
        mode: AudioDockNoiseSuppression,
    },
    SelectEqualizer {
        id: u8,
    },
    /// None creates in a free slot; Some explicitly replaces that custom slot.
    SaveEqualizer {
        id: Option<u8>,
        name: String,
        gains: [i8; 5],
    },
    /// Name guards against deleting a different preset after a stale UI selection.
    DeleteEqualizer {
        id: u8,
        name: String,
    },
}
