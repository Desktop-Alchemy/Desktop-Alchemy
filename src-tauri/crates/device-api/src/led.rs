//! Software effects for an adapter-described LED layout.
//! The device has no native animation API, so effects are sent as coalesced RGB frames.
use crate::{assets as storage, model::*};
use anyhow::{bail, Result};
use std::time::Duration;

#[cfg(test)]
const LED_COUNT: usize = 24;

impl LedMode {
    pub fn is_animated(self) -> bool {
        matches!(
            self,
            Self::Breathing | Self::ColorCycle | Self::Rainbow | Self::Wave | Self::Comet
        )
    }

    pub fn requires_individual_leds(self) -> bool {
        self == Self::Individual || self.is_animated()
    }
}

fn scale(rgb: [u8; 3], level: u8) -> [u8; 3] {
    rgb.map(|v| ((u16::from(v) * u16::from(level) + 127) / 255) as u8)
}

/// Speed 1 is a 9-second cycle and speed 10 is a 1.8-second cycle.
pub fn cycle_period(speed: u8) -> Duration {
    let speed = speed.clamp(1, 10);
    Duration::from_millis(9_000 - u64::from(speed - 1) * 800)
}

fn hue_rgb(hue: f64) -> [u8; 3] {
    let h = hue.rem_euclid(1.0) * 6.0;
    let x = 255.0 * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u8 {
        0 => (255.0, x, 0.0),
        1 => (x, 255.0, 0.0),
        2 => (0.0, 255.0, x),
        3 => (0.0, x, 255.0),
        4 => (x, 0.0, 255.0),
        _ => (255.0, 0.0, x),
    };
    [r.round() as u8, g.round() as u8, b.round() as u8]
}

fn apply_level(rgb: [u8; 3], brightness: u8, effect_level: f64) -> [u8; 3] {
    let level = (f64::from(brightness) / 255.0) * effect_level.clamp(0.0, 1.0);
    rgb.map(|v| (f64::from(v) * level).round() as u8)
}

/// Render one frame in the device's 24-element LED order.
pub fn render_frame(settings: &DeviceSettings, elapsed: Duration) -> Result<Vec<[u8; 3]>> {
    render_frame_count(settings, elapsed, settings.led_colors.len())
}
pub fn render_frame_count(
    settings: &DeviceSettings,
    elapsed: Duration,
    count: usize,
) -> Result<Vec<[u8; 3]>> {
    if !settings.led_mode.is_animated() {
        bail!("当前 LED 模式不是软件动态灯效")
    }
    let primary = storage::color(&settings.led_color)?;
    let period = cycle_period(settings.led_speed).as_secs_f64();
    let phase = (elapsed.as_secs_f64() / period).fract();
    let colors = (0..count)
        .map(|index| {
            let position = index as f64 / count as f64;
            match settings.led_mode {
                LedMode::Breathing => {
                    let level = 0.5 - 0.5 * (std::f64::consts::TAU * phase).cos();
                    apply_level(primary, settings.led_brightness, level)
                }
                LedMode::ColorCycle => apply_level(hue_rgb(phase), settings.led_brightness, 1.0),
                LedMode::Rainbow => {
                    apply_level(hue_rgb(position + phase), settings.led_brightness, 1.0)
                }
                LedMode::Wave => {
                    let wave = (std::f64::consts::TAU * (position - phase)).cos();
                    let level = 0.08 + 0.92 * (0.5 + 0.5 * wave);
                    apply_level(primary, settings.led_brightness, level)
                }
                LedMode::Comet => {
                    let head = phase * count as f64;
                    let distance = (head - index as f64).rem_euclid(count as f64);
                    let level = if distance < 5.0 {
                        (1.0 - distance / 5.0).powi(2)
                    } else {
                        0.0
                    };
                    apply_level(primary, settings.led_brightness, level)
                }
                LedMode::Solid | LedMode::Individual | LedMode::DeviceDefault => [0; 3],
            }
        })
        .collect();
    Ok(colors)
}

impl DeviceSettings {
    /// The modes that are represented directly by one SDK command.
    pub fn saved_led_command(&self) -> Option<HardwareCommand> {
        match self.led_mode {
            LedMode::Solid => Some(HardwareCommand::LedColor {
                color: self.led_color.clone(),
            }),
            LedMode::Individual => Some(HardwareCommand::LedColors {
                colors: self.led_colors.clone(),
            }),
            LedMode::DeviceDefault => Some(HardwareCommand::ResetLed),
            _ => None,
        }
    }

    /// Return an immutable wire command so queue merging cannot lose colour/level.
    /// None means this command is unrelated to LEDs.
    pub fn resolve_led(&mut self, command: &HardwareCommand) -> Result<Option<HardwareCommand>> {
        self.resolve_led_at(command, Duration::ZERO)
    }

    pub fn resolve_led_at(
        &mut self,
        command: &HardwareCommand,
        elapsed: Duration,
    ) -> Result<Option<HardwareCommand>> {
        match command {
            HardwareCommand::LedBrightness { value } => {
                if self.led_mode == LedMode::DeviceDefault {
                    bail!("设备默认灯效由固件控制，无法可靠调节亮度；请先选择静态灯色")
                }
                self.led_brightness = *value;
            }
            HardwareCommand::LedColor { color } => {
                storage::color(color)?;
                self.led_color = color.clone();
                self.led_mode = LedMode::Solid;
            }
            HardwareCommand::LedColors { colors } => {
                if colors.len() != self.led_colors.len() {
                    bail!("逐灯颜色数量与设备不一致")
                }
                self.led_colors = colors.clone();
                self.led_mode = LedMode::Individual;
            }
            HardwareCommand::ResetLed => {
                self.led_mode = LedMode::DeviceDefault;
                return Ok(Some(HardwareCommand::ResetLed));
            }
            _ => return Ok(None),
        }

        if self.led_mode.is_animated() {
            return Ok(Some(HardwareCommand::LedColors {
                colors: render_frame(self, elapsed)?,
            }));
        }

        Ok(Some(match self.led_mode {
            LedMode::Solid => {
                let [r, g, b] = scale(storage::color(&self.led_color)?, self.led_brightness);
                HardwareCommand::LedColor {
                    color: format!("#{r:02X}{g:02X}{b:02X}"),
                }
            }
            LedMode::Individual => HardwareCommand::LedColors {
                colors: self
                    .led_colors
                    .iter()
                    .map(|rgb| scale(*rgb, self.led_brightness))
                    .collect(),
            },
            LedMode::DeviceDefault => unreachable!(),
            _ => unreachable!("dynamic modes returned above"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: LedMode) -> DeviceSettings {
        DeviceSettings {
            led_mode: mode,
            led_speed: 5,
            led_brightness: 255,
            ..DeviceSettings {
                led_colors: vec![[89, 102, 243]; 24],
                led_color: "#5966F3".into(),
                ..Default::default()
            }
        }
    }

    #[test]
    fn animation_modes_render_exactly_24_leds_and_apply_brightness() {
        for mode in [
            LedMode::Breathing,
            LedMode::ColorCycle,
            LedMode::Rainbow,
            LedMode::Wave,
            LedMode::Comet,
        ] {
            let full = render_frame(&settings(mode), Duration::from_millis(500)).unwrap();
            assert_eq!(full.len(), LED_COUNT);
            let dim = render_frame(
                &DeviceSettings {
                    led_brightness: 128,
                    ..settings(mode)
                },
                Duration::from_millis(500),
            )
            .unwrap();
            assert_eq!(dim.len(), LED_COUNT);
            assert!(dim.iter().zip(full).all(|(a, b)| {
                a.iter()
                    .zip(b)
                    .all(|(dim, full)| u16::from(*dim) <= u16::from(full))
            }));
        }
    }

    #[test]
    fn dimming_preserves_base_colour_and_recovers_after_zero() {
        let mut s = DeviceSettings {
            led_colors: vec![[89, 102, 243]; 24],
            led_color: "#5966F3".into(),
            ..Default::default()
        };
        s.led_brightness = 255;
        s.resolve_led(&HardwareCommand::LedColor {
            color: "#804020".into(),
        })
        .unwrap();
        let out = s
            .resolve_led(&HardwareCommand::LedBrightness { value: 128 })
            .unwrap()
            .unwrap();
        assert!(matches!(out, HardwareCommand::LedColor { color } if color == "#402010"));
        let out = s
            .resolve_led(&HardwareCommand::LedBrightness { value: 0 })
            .unwrap()
            .unwrap();
        assert!(matches!(out, HardwareCommand::LedColor { color } if color == "#000000"));
        let out = s
            .resolve_led(&HardwareCommand::LedBrightness { value: 255 })
            .unwrap()
            .unwrap();
        assert!(matches!(out, HardwareCommand::LedColor { color } if color == "#804020"));
        assert_eq!(s.led_color, "#804020");
    }

    #[test]
    fn individual_colours_and_default_effect_have_distinct_behaviour() {
        let mut s = DeviceSettings {
            led_colors: vec![[89, 102, 243]; 24],
            led_color: "#5966F3".into(),
            ..Default::default()
        };
        s.led_brightness = 255;
        s.resolve_led(&HardwareCommand::LedColors {
            colors: vec![[200, 100, 50]; LED_COUNT],
        })
        .unwrap();
        let out = s
            .resolve_led(&HardwareCommand::LedBrightness { value: 0 })
            .unwrap()
            .unwrap();
        assert!(
            matches!(out, HardwareCommand::LedColors { colors } if colors == vec![[0, 0, 0]; LED_COUNT])
        );
        assert_eq!(s.led_colors, vec![[200, 100, 50]; LED_COUNT]);
        s.resolve_led(&HardwareCommand::ResetLed).unwrap();
        assert!(s
            .resolve_led(&HardwareCommand::LedBrightness { value: 100 })
            .is_err());
        assert!(matches!(
            s.saved_led_command(),
            Some(HardwareCommand::ResetLed)
        ));
        s.resolve_led(&HardwareCommand::LedColor {
            color: "#FFFFFF".into(),
        })
        .unwrap();
        assert!(matches!(s.led_mode, LedMode::Solid));
    }

    #[test]
    fn existing_configuration_defaults_to_static_colour_and_medium_speed() {
        let s: DeviceSettings = serde_json::from_str(
            r##"{"screen_brightness":70,"led_brightness":100,"led_color":"#5966F3"}"##,
        )
        .unwrap();
        assert!(matches!(s.led_mode, LedMode::Solid));
        assert_eq!(s.led_speed, 5);
        assert!(s.led_colors.is_empty());
    }
}
