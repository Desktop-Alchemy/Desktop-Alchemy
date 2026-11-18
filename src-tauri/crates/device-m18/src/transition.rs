//! M18 page-transition settings and the brightness fade scheduler.
use alchemy_device_api::UiFailure;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum M18TransitionMode {
    #[default]
    None,
    FadeIn,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    SlideRandom,
}
impl M18TransitionMode {
    pub fn slides(self) -> bool {
        matches!(
            self,
            Self::SlideLeft
                | Self::SlideRight
                | Self::SlideUp
                | Self::SlideDown
                | Self::SlideRandom
        )
    }
    /// Resolve once per page, before preparing any grid or background frame.
    pub(crate) fn resolve_slide(self, choice: u8) -> Self {
        if self == Self::SlideRandom {
            [
                Self::SlideLeft,
                Self::SlideRight,
                Self::SlideUp,
                Self::SlideDown,
            ][usize::from(choice % 4)]
        } else {
            self
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct M18PageTransition {
    pub mode: M18TransitionMode,
    pub duration_ms: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct M18TransitionEdit {
    pub settings: M18PageTransition,
    pub expected: M18PageTransition,
}
impl Default for M18PageTransition {
    fn default() -> Self {
        Self {
            mode: M18TransitionMode::None,
            duration_ms: 300,
        }
    }
}
impl M18PageTransition {
    pub fn validate(&self) -> Result<()> {
        if !(150..=1000).contains(&self.duration_ms) {
            bail!(UiFailure::new(
                "errors.pageTransitionDuration",
                "页面渐显时长须为 150–1000 毫秒"
            ));
        }
        Ok(())
    }
    pub fn from_module(value: &serde_json::Value) -> Result<Self> {
        let settings: Self = value
            .get("page_transition")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        settings.validate()?;
        Ok(settings)
    }
}

pub(crate) struct FadeIn {
    pub target: u8,
    started: Instant,
    duration: Duration,
    next: Instant,
    last: u8,
}
impl FadeIn {
    pub fn new(target: u8, duration_ms: u16, now: Instant) -> Self {
        Self {
            target,
            started: now,
            duration: Duration::from_millis(duration_ms.into()),
            next: now,
            last: 0,
        }
    }
    /// A single latest brightness value; slow SDK calls never cause catch-up bursts.
    pub fn sample(&mut self, now: Instant) -> Option<(u8, bool)> {
        if now < self.next {
            return None;
        }
        let progress =
            (now.duration_since(self.started).as_secs_f64() / self.duration.as_secs_f64()).min(1.0);
        let value =
            (f64::from(self.target) * progress * progress * (3.0 - 2.0 * progress)).round() as u8;
        self.next = now + Duration::from_millis(25);
        let done = progress >= 1.0;
        if value == self.last && !done {
            return None;
        }
        self.last = value;
        Some((value, done))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn random_slide_resolves_to_each_direction_and_preserves_fixed_choices() {
        let directions = [
            M18TransitionMode::SlideLeft,
            M18TransitionMode::SlideRight,
            M18TransitionMode::SlideUp,
            M18TransitionMode::SlideDown,
        ];
        for choice in 0..=u8::MAX {
            assert_eq!(
                M18TransitionMode::SlideRandom.resolve_slide(choice),
                directions[usize::from(choice % 4)]
            );
            for direction in directions {
                assert_eq!(direction.resolve_slide(choice), direction);
            }
        }
        let settings = M18PageTransition::from_module(&serde_json::json!({
            "page_transition": {"mode": "slide_random", "duration_ms": 650}
        }))
        .unwrap();
        assert_eq!(settings.mode, M18TransitionMode::SlideRandom);
        assert_eq!(
            serde_json::to_value(settings).unwrap()["mode"],
            "slide_random"
        );
    }
    #[test]
    fn legacy_settings_default_to_off_and_invalid_values_are_rejected() {
        assert_eq!(
            M18PageTransition::from_module(&serde_json::json!({})).unwrap(),
            M18PageTransition::default()
        );
        for value in [
            serde_json::json!({"mode":"unknown","duration_ms":300}),
            serde_json::json!({"mode":"fade_in","duration_ms":149}),
            serde_json::json!({"mode":"none","duration_ms":1001}),
        ] {
            assert!(
                M18PageTransition::from_module(&serde_json::json!({"page_transition":value}))
                    .is_err()
            );
        }
    }
    #[test]
    fn fade_is_monotonic_bounded_and_finishes_at_saved_brightness_without_catch_up() {
        let now = Instant::now();
        let mut fade = FadeIn::new(70, 300, now);
        let mut last = 0;
        for ms in 0..=300 {
            if let Some((value, done)) = fade.sample(now + Duration::from_millis(ms)) {
                assert!(value >= last && value <= 70);
                assert_eq!(done, ms == 300);
                last = value;
            }
        }
        assert_eq!(last, 70);
        let mut delayed = FadeIn::new(42, 300, now);
        assert_eq!(
            delayed.sample(now + Duration::from_secs(1)),
            Some((42, true))
        );
    }
}
