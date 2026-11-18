//! A moving viewport over two adjacent page canvases. No hardware or file access.
use crate::transition::M18TransitionMode;
use anyhow::{ensure, Result};
use image::{imageops, RgbImage};
use std::time::{Duration, Instant};

pub const FRAME_INTERVAL: Duration = Duration::from_millis(50);
pub const GRID_WIDTH: u32 = 5 * 64;
pub const GRID_HEIGHT: u32 = 3 * 64;

pub fn eased(progress: f64) -> f64 {
    let t = progress.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The direction names describe content motion: left brings the new page from the right.
/// Old and new canvases are adjacent; cropping moves every tile by the same distance.
pub fn viewport(
    old: &RgbImage,
    new: &RgbImage,
    mode: M18TransitionMode,
    progress: f64,
) -> Result<RgbImage> {
    ensure!(
        old.dimensions() == new.dimensions() && old.width() > 0 && old.height() > 0,
        "Page canvases must have matching dimensions"
    );
    ensure!(
        mode.slides() && mode != M18TransitionMode::SlideRandom && progress.is_finite(),
        "Invalid slide viewport"
    );
    let (w, h) = old.dimensions();
    let horizontal = matches!(
        mode,
        M18TransitionMode::SlideLeft | M18TransitionMode::SlideRight
    );
    let forward = matches!(
        mode,
        M18TransitionMode::SlideLeft | M18TransitionMode::SlideUp
    );
    let extent = if horizontal { w } else { h };
    let offset = (f64::from(extent) * eased(progress)).round() as u32;
    // Compose the visible spans directly; no black seam or duplicated edge pixels.
    let mut output = RgbImage::new(w, h);
    if forward {
        if horizontal {
            imageops::replace(
                &mut output,
                &imageops::crop_imm(old, offset, 0, w - offset, h).to_image(),
                0,
                0,
            );
            imageops::replace(
                &mut output,
                &imageops::crop_imm(new, 0, 0, offset, h).to_image(),
                (w - offset).into(),
                0,
            );
        } else {
            imageops::replace(
                &mut output,
                &imageops::crop_imm(old, 0, offset, w, h - offset).to_image(),
                0,
                0,
            );
            imageops::replace(
                &mut output,
                &imageops::crop_imm(new, 0, 0, w, offset).to_image(),
                0,
                (h - offset).into(),
            );
        }
    } else if horizontal {
        imageops::replace(
            &mut output,
            &imageops::crop_imm(new, w - offset, 0, offset, h).to_image(),
            0,
            0,
        );
        imageops::replace(
            &mut output,
            &imageops::crop_imm(old, 0, 0, w - offset, h).to_image(),
            offset.into(),
            0,
        );
    } else {
        imageops::replace(
            &mut output,
            &imageops::crop_imm(new, 0, h - offset, w, offset).to_image(),
            0,
            0,
        );
        imageops::replace(
            &mut output,
            &imageops::crop_imm(old, 0, 0, w, h - offset).to_image(),
            0,
            offset.into(),
        );
    }
    Ok(output)
}

/// Absolute time chooses the latest frame. The final destination is always submitted.
pub(crate) struct Timeline {
    started: Instant,
    duration: Duration,
    steps: usize,
    submitted: usize,
}
impl Timeline {
    pub fn steps(duration_ms: u16) -> usize {
        (usize::from(duration_ms) / FRAME_INTERVAL.as_millis() as usize).clamp(3, 20)
    }
    pub fn new(duration_ms: u16, started: Instant) -> Self {
        Self {
            started,
            duration: Duration::from_millis(duration_ms.into()),
            steps: Self::steps(duration_ms),
            submitted: 0,
        }
    }
    /// Steps 1..steps-1 are encoded viewports; steps is the original destination batch.
    pub fn due(&self, now: Instant) -> Option<usize> {
        let elapsed = now.saturating_duration_since(self.started);
        let step = ((elapsed.as_secs_f64() / self.duration.as_secs_f64()) * self.steps as f64)
            .floor() as usize;
        let step = step.min(self.steps);
        (step > self.submitted).then_some(step)
    }
    pub fn submitted(&mut self, step: usize) {
        self.submitted = step;
    }
    pub fn final_step(&self) -> usize {
        self.steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;
    #[test]
    fn viewport_has_exact_endpoints_and_moves_whole_rows_and_columns_without_seams() {
        let old = RgbImage::from_fn(10, 6, |x, y| Rgb([x as u8, y as u8, 1]));
        let new = RgbImage::from_fn(10, 6, |x, y| Rgb([x as u8 + 20, y as u8 + 20, 2]));
        for mode in [
            M18TransitionMode::SlideLeft,
            M18TransitionMode::SlideRight,
            M18TransitionMode::SlideUp,
            M18TransitionMode::SlideDown,
        ] {
            assert_eq!(viewport(&old, &new, mode, 0.0).unwrap(), old);
            assert_eq!(viewport(&old, &new, mode, 1.0).unwrap(), new);
            let half = viewport(&old, &new, mode, 0.5).unwrap();
            for (x, y, pixel) in half.enumerate_pixels() {
                let expected = match mode {
                    M18TransitionMode::SlideLeft => {
                        if x < 5 {
                            old.get_pixel(x + 5, y)
                        } else {
                            new.get_pixel(x - 5, y)
                        }
                    }
                    M18TransitionMode::SlideRight => {
                        if x < 5 {
                            new.get_pixel(x + 5, y)
                        } else {
                            old.get_pixel(x - 5, y)
                        }
                    }
                    M18TransitionMode::SlideUp => {
                        if y < 3 {
                            old.get_pixel(x, y + 3)
                        } else {
                            new.get_pixel(x, y - 3)
                        }
                    }
                    _ => {
                        if y < 3 {
                            new.get_pixel(x, y + 3)
                        } else {
                            old.get_pixel(x, y - 3)
                        }
                    }
                };
                assert_eq!(pixel, expected);
            }
        }
        assert!(viewport(&old, &new, M18TransitionMode::None, 0.5).is_err());
        assert!(viewport(
            &old,
            &RgbImage::new(1, 1),
            M18TransitionMode::SlideLeft,
            0.5
        )
        .is_err());
    }
    #[test]
    fn slow_submission_skips_intermediate_steps_and_never_skips_the_final_page() {
        let start = Instant::now();
        let mut timeline = Timeline::new(500, start);
        assert_eq!(timeline.due(start), None);
        assert_eq!(timeline.due(start + Duration::from_millis(50)), Some(1));
        timeline.submitted(1);
        assert_eq!(timeline.due(start + Duration::from_millis(50)), None);
        assert_eq!(timeline.due(start + Duration::from_millis(350)), Some(7));
        timeline.submitted(7);
        assert_eq!(
            timeline.due(start + Duration::from_secs(2)),
            Some(timeline.final_step())
        );
        timeline.submitted(timeline.final_step());
        assert_eq!(timeline.due(start + Duration::from_secs(3)), None);
    }
}
