use crate::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputKind {
    Button,
    Rotary,
    Analog { min: f64, max: f64, unit: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct InputControl {
    pub id: String,
    pub name: String,
    pub kind: InputKind,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct DisplayRegion {
    pub id: String,
    pub key: u8,
    pub width: u16,
    pub height: u16,
    pub column: u16,
    pub row: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputTrigger {
    Press,
    Clockwise,
    Counterclockwise,
    Above { threshold: f64, hysteresis: f64 },
    Below { threshold: f64, hysteresis: f64 },
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct ControlBinding {
    pub control_id: String,
    pub enabled: bool,
    pub trigger: InputTrigger,
    pub action: KeyAction,
}
impl DeviceDefinition {
    pub fn normalized(mut self) -> Self {
        if self.inputs.is_empty() {
            self.inputs = self
                .layout
                .input_keys
                .iter()
                .map(|key| InputControl {
                    id: key.to_string(),
                    name: self
                        .input_names
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| key.to_string()),
                    kind: InputKind::Button,
                })
                .collect();
        }
        if self.displays.is_empty() {
            let columns = self.layout.columns.max(1);
            self.displays = self
                .layout
                .display_keys
                .iter()
                .enumerate()
                .map(|(index, key)| DisplayRegion {
                    id: key.to_string(),
                    key: *key,
                    width: self.layout.key_width,
                    height: self.layout.key_height,
                    column: index as u16 % columns,
                    row: index as u16 / columns,
                })
                .collect();
        }
        self
    }
}
pub const MAX_PAGE_OPERATIONS: usize = 1024;
pub const MAX_PAGE_BYTES: usize = 64 * 1024 * 1024;
