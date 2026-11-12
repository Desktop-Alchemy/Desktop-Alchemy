use crate::UiMessage;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DayFilter {
    #[default]
    EveryDay,
    Weekdays {
        days: Vec<u8>,
    },
    Date {
        date: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct SceneTime {
    pub days: DayFilter,
    pub time: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneTrigger {
    Continuous,
    Scheduled { schedule: SceneTime },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessMatcher {
    Name { value: String },
    Path { value: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneCondition {
    All {
        children: Vec<SceneCondition>,
    },
    Any {
        children: Vec<SceneCondition>,
    },
    Not {
        child: Box<SceneCondition>,
    },
    Process {
        matcher: ProcessMatcher,
    },
    Foreground {
        matcher: ProcessMatcher,
    },
    MediaPlaying,
    TimeRange {
        days: DayFilter,
        start: String,
        end: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub page_id: String,
    pub trigger: SceneTrigger,
    pub condition: SceneCondition,
    pub enter_delay_ms: u32,
    pub exit_delay_ms: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneFallback {
    Keep,
    #[default]
    Return,
    Page {
        page_id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(default)]
pub struct SceneSettings {
    pub enabled: bool,
    pub paused: bool,
    pub process_interval_ms: u32,
    pub fallback: SceneFallback,
    pub scenes: Vec<Scene>,
}
impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            process_interval_ms: 1000,
            fallback: SceneFallback::Return,
            scenes: vec![],
        }
    }
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConditionTruth {
    True,
    False,
    #[default]
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct RunningProgram {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct SceneState {
    pub id: String,
    pub truth: ConditionTruth,
    pub status: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
pub struct SceneStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_message: Option<UiMessage>,
    pub states: Vec<SceneState>,
    pub winner_id: Option<String>,
    pub retained_id: Option<String>,
    pub error: Option<String>,
}
