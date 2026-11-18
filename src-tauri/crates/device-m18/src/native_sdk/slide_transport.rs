//! Pre-encoded intermediate pages and the last images accepted by this SDK owner.
use super::{encode_jpeg, Prepared};
use crate::{slide, transition::M18PageTransition};
use alchemy_device_api::HardwareCommand;
use anyhow::Result;
use image::{imageops, DynamicImage, RgbImage};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct DisplayCache {
    keys: BTreeMap<u8, Vec<u8>>,
    background: Option<Vec<u8>>,
}
impl DisplayCache {
    pub fn invalidate(&mut self) {
        self.keys.clear();
        self.background = None;
    }
    pub fn key(&mut self, key: u8, jpeg: &[u8]) {
        self.keys.insert(key, jpeg.to_vec());
    }
    pub fn matches_static(&self, item: &Prepared) -> bool {
        match &item.command {
            HardwareCommand::KeyImage { key, .. } | HardwareCommand::KeyData { key, .. } => {
                self.keys.get(key).is_some_and(|jpeg| jpeg == &item.image)
            }
            HardwareCommand::Background {
                persistent: false, ..
            }
            | HardwareCommand::BackgroundData {
                persistent: false, ..
            } => self.background.as_ref() == Some(&item.image),
            _ => false, // GIFs must still install their playback state.
        }
    }
    pub fn record(&mut self, command: &HardwareCommand, jpeg: &[u8]) {
        use HardwareCommand::*;
        match command {
            KeyImage { key, .. } | KeyData { key, .. } | KeyGif { key, .. } => self.key(*key, jpeg),
            BulkImage { keys, .. } => {
                for key in keys {
                    self.key(*key, jpeg);
                }
            }
            Background {
                persistent: false, ..
            }
            | BackgroundData {
                persistent: false, ..
            } => self.background = Some(jpeg.to_vec()),
            Background {
                persistent: true, ..
            }
            | BackgroundData {
                persistent: true, ..
            } => self.background = None,
            ClearKey { key } => {
                self.keys.remove(key);
            }
            ClearAll => self.invalidate(),
            _ => {}
        }
    }
    fn canvases(&self) -> Result<Option<(RgbImage, RgbImage)>> {
        let Some(background) = &self.background else {
            return Ok(None);
        };
        if !(1..=15).all(|key| self.keys.contains_key(&key)) {
            return Ok(None);
        }
        let mut grid = RgbImage::new(slide::GRID_WIDTH, slide::GRID_HEIGHT);
        for key in 1..=15u8 {
            let image = image::load_from_memory(&self.keys[&key])?.to_rgb8();
            if image.dimensions() != (64, 64) {
                return Ok(None);
            }
            imageops::replace(
                &mut grid,
                &image,
                i64::from((key - 1) % 5) * 64,
                i64::from((key - 1) / 5) * 64,
            );
        }
        let background = image::load_from_memory(background)?.to_rgb8();
        if background.dimensions() != (480, 272) {
            return Ok(None);
        }
        Ok(Some((grid, background)))
    }
}

pub(super) fn prepared_bytes(prepared: &[Prepared]) -> usize {
    prepared
        .iter()
        .map(|item| {
            item.image.len()
                + item
                    .animation
                    .as_ref()
                    .map(|a| a.frames.iter().map(|(bytes, _)| bytes.len()).sum::<usize>())
                    .unwrap_or(0)
        })
        .sum()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Progress {
    Pending,
    Complete,
    Cancelled,
}
pub(super) struct Presentation {
    target: Vec<Prepared>,
    frames: Vec<Option<Vec<Prepared>>>,
    timeline: slide::Timeline,
    terminal: Option<Progress>,
}
impl Presentation {
    pub fn new(
        target: Vec<Prepared>,
        frames: Vec<Option<Vec<Prepared>>>,
        duration_ms: u16,
        now: std::time::Instant,
    ) -> Self {
        Self {
            target,
            frames,
            timeline: slide::Timeline::new(duration_ms, now),
            terminal: None,
        }
    }
    pub fn into_target(self) -> Vec<Prepared> {
        self.target
    }
    pub fn tick(
        &mut self,
        now: std::time::Instant,
        current: impl Fn() -> bool,
        mut submit: impl FnMut(Vec<Prepared>) -> Result<()>,
    ) -> Result<Progress> {
        if let Some(terminal) = self.terminal {
            return Ok(terminal);
        }
        if !current() {
            self.terminal = Some(Progress::Cancelled);
            return Ok(Progress::Cancelled);
        }
        let Some(step) = self.timeline.due(now) else {
            return Ok(Progress::Pending);
        };
        let final_step = step == self.timeline.final_step();
        let frame = if final_step {
            std::mem::take(&mut self.target)
        } else {
            self.frames[step - 1]
                .take()
                .ok_or_else(|| anyhow::anyhow!("Slide frame already submitted"))?
        };
        if let Err(error) = submit(frame) {
            self.terminal = Some(Progress::Cancelled);
            return Err(error);
        }
        self.timeline.submitted(step);
        for frame in self.frames.iter_mut().take(step.saturating_sub(1)) {
            *frame = None;
        }
        if final_step {
            self.terminal = Some(Progress::Complete);
        }
        Ok(self.terminal.unwrap_or(Progress::Pending))
    }
}

/// A missing known source or an exceeded optional animation budget falls back to
/// the destination page. No source pixels are read from the hardware.
pub(super) fn prepare_frames(
    old: &DisplayCache,
    target: &[Prepared],
    settings: &M18PageTransition,
    byte_budget: usize,
    current: impl Fn() -> bool,
) -> Result<Option<Vec<Option<Vec<Prepared>>>>> {
    let Some((old_grid, old_background)) = old.canvases()? else {
        return Ok(None);
    };
    let mut destination = DisplayCache::default();
    for item in target {
        destination.record(&item.command, &item.image);
    }
    let Some((new_grid, new_background)) = destination.canvases()? else {
        return Ok(None);
    };
    let steps = slide::Timeline::steps(settings.duration_ms);
    let mode = settings
        .mode
        .resolve_slide(uuid::Uuid::new_v4().as_bytes()[0]);
    let background_moves = old_background != new_background;
    let mut frames = Vec::with_capacity(steps - 1);
    let mut bytes = 0usize;
    for step in 1..steps {
        anyhow::ensure!(
            current(),
            alchemy_device_api::UiFailure::new("backend.b311", "操作已过期")
        );
        let progress = step as f64 / steps as f64;
        let grid = slide::viewport(&old_grid, &new_grid, mode, progress)?;
        let mut frame = Vec::with_capacity(17);
        if background_moves {
            let background = slide::viewport(&old_background, &new_background, mode, progress)?;
            frame.push(Prepared {
                command: HardwareCommand::BackgroundData {
                    data: String::new(),
                    persistent: false,
                },
                image: encode_jpeg(&DynamicImage::ImageRgb8(background))?,
                animation: None,
            });
        }
        for key in 1..=15u8 {
            anyhow::ensure!(
                current(),
                alchemy_device_api::UiFailure::new("backend.b311", "操作已过期")
            );
            let tile = imageops::crop_imm(
                &grid,
                u32::from((key - 1) % 5) * 64,
                u32::from((key - 1) / 5) * 64,
                64,
                64,
            )
            .to_image();
            frame.push(Prepared {
                command: HardwareCommand::KeyImage {
                    key,
                    asset_id: String::new(),
                },
                image: encode_jpeg(&DynamicImage::ImageRgb8(tile))?,
                animation: None,
            });
        }
        frame.push(Prepared {
            command: HardwareCommand::Refresh,
            image: vec![],
            animation: None,
        });
        bytes = bytes.saturating_add(prepared_bytes(&frame));
        if bytes > byte_budget {
            return Ok(None);
        }
        frames.push(Some(frame));
    }
    Ok(Some(frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(color: [u8; 3]) -> Vec<Prepared> {
        let mut items = vec![Prepared {
            command: HardwareCommand::BackgroundData {
                data: String::new(),
                persistent: false,
            },
            image: encode_jpeg(&DynamicImage::ImageRgb8(RgbImage::from_pixel(
                480,
                272,
                image::Rgb(color),
            )))
            .unwrap(),
            animation: None,
        }];
        for key in 1..=15 {
            items.push(Prepared {
                command: HardwareCommand::KeyImage {
                    key,
                    asset_id: String::new(),
                },
                image: encode_jpeg(&DynamicImage::ImageRgb8(RgbImage::from_pixel(
                    64,
                    64,
                    image::Rgb([color[0], key * 10, color[2]]),
                )))
                .unwrap(),
                animation: None,
            });
        }
        items
    }
    #[test]
    fn motion_frames_are_preencoded_complete_public_key_batches_with_bounded_memory() {
        let old = page([20, 30, 40]);
        let target = page([180, 190, 200]);
        let mut cache = DisplayCache::default();
        for item in &old {
            cache.record(&item.command, &item.image);
        }
        let settings = M18PageTransition {
            mode: crate::transition::M18TransitionMode::SlideLeft,
            duration_ms: 150,
        };
        let frames = prepare_frames(&cache, &target, &settings, 64 * 1024 * 1024, || true)
            .unwrap()
            .unwrap();
        let first = frames[0].as_ref().unwrap();
        let top = image::load_from_memory(&first[1].image).unwrap().to_rgb8();
        let bottom = image::load_from_memory(&first[11].image).unwrap().to_rgb8();
        assert!((i16::from(top.get_pixel(0, 0)[1]) - 20).abs() < 5);
        assert!((i16::from(bottom.get_pixel(0, 0)[1]) - 120).abs() < 5);
        for frame in frames.into_iter().flatten() {
            assert!(matches!(
                frame.first().unwrap().command,
                HardwareCommand::BackgroundData {
                    persistent: false,
                    ..
                }
            ));
            for (slot, item) in frame[1..16].iter().enumerate() {
                assert!(
                    matches!(item.command,HardwareCommand::KeyImage { key,.. } if usize::from(key)==slot+1)
                );
                assert_eq!(image::load_from_memory(&item.image).unwrap().width(), 64);
            }
            assert!(matches!(
                frame.last().unwrap().command,
                HardwareCommand::Refresh
            ));
        }
        assert!(prepare_frames(&cache, &target, &settings, 1, || true)
            .unwrap()
            .is_none());
        assert!(prepare_frames(&cache, &target, &settings, 64 * 1024 * 1024, || false).is_err());
        let mut same_background = page([180, 190, 200]);
        same_background[0].image = old[0].image.clone();
        let light_frames = prepare_frames(
            &cache,
            &same_background,
            &settings,
            64 * 1024 * 1024,
            || true,
        )
        .unwrap()
        .unwrap();
        assert!(light_frames
            .into_iter()
            .flatten()
            .all(|frame| matches!(frame[0].command, HardwareCommand::KeyImage { key: 1, .. })));
        assert!(cache.matches_static(&old[0]) && cache.matches_static(&old[1]));
        assert!(!cache.matches_static(&target[1]));
        let gif = Prepared {
            command: HardwareCommand::KeyGif {
                key: 1,
                asset_id: String::new(),
            },
            image: old[1].image.clone(),
            animation: None,
        };
        assert!(!cache.matches_static(&gif));
        // A random page must match one whole fixed-direction animation, including
        // its background; resolving separately per frame would fail this check.
        let random_settings = M18PageTransition {
            mode: crate::transition::M18TransitionMode::SlideRandom,
            ..settings.clone()
        };
        let images = |frames: Vec<Option<Vec<Prepared>>>| {
            frames
                .into_iter()
                .flatten()
                .map(|frame| frame.into_iter().map(|item| item.image).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };
        let random = images(
            prepare_frames(&cache, &target, &random_settings, 64 * 1024 * 1024, || true)
                .unwrap()
                .unwrap(),
        );
        assert!((0..4).any(|choice| {
            let fixed = M18PageTransition {
                mode: random_settings.mode.resolve_slide(choice),
                ..settings.clone()
            };
            random
                == images(
                    prepare_frames(&cache, &target, &fixed, 64 * 1024 * 1024, || true)
                        .unwrap()
                        .unwrap(),
                )
        }));
        cache.record(&HardwareCommand::ClearKey { key: 1 }, &[]);
        assert!(
            prepare_frames(&cache, &target, &settings, 64 * 1024 * 1024, || true)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn presentation_cancels_unsent_frames_skips_slow_steps_and_completes_only_after_final_submission(
    ) {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let frame = |key| {
            vec![Prepared {
                command: HardwareCommand::KeyImage {
                    key,
                    asset_id: String::new(),
                },
                image: vec![],
                animation: None,
            }]
        };
        let make = || {
            Presentation::new(
                frame(15),
                (1..10).map(|key| Some(frame(key))).collect(),
                500,
                start,
            )
        };
        let mut motion = make();
        let mut writes = Vec::new();
        let mut submit = |items: Vec<Prepared>| {
            if let HardwareCommand::KeyImage { key, .. } = items[0].command {
                writes.push(key);
            }
            Ok(())
        };
        assert_eq!(
            motion
                .tick(start + Duration::from_millis(250), || true, &mut submit)
                .unwrap(),
            Progress::Pending
        );
        assert_eq!(
            motion
                .tick(start + Duration::from_millis(900), || true, &mut submit)
                .unwrap(),
            Progress::Complete
        );
        assert_eq!(
            motion
                .tick(start + Duration::from_secs(2), || true, &mut submit)
                .unwrap(),
            Progress::Complete
        );
        assert_eq!(writes, vec![5, 15]);
        let mut cancelled = make();
        assert_eq!(
            cancelled
                .tick(
                    start + Duration::from_millis(100),
                    || false,
                    |_| panic!("Stale frame submitted")
                )
                .unwrap(),
            Progress::Cancelled
        );
        assert_eq!(
            cancelled
                .tick(
                    start + Duration::from_secs(1),
                    || true,
                    |_| panic!("Cancelled page replayed")
                )
                .unwrap(),
            Progress::Cancelled
        );
        let mut failed = make();
        assert!(failed
            .tick(
                start + Duration::from_millis(100),
                || true,
                |_| anyhow::bail!("SDK failure")
            )
            .is_err());
        assert_eq!(
            failed
                .tick(
                    start + Duration::from_secs(1),
                    || true,
                    |_| panic!("Failed batch replayed")
                )
                .unwrap(),
            Progress::Cancelled
        );
    }
}
