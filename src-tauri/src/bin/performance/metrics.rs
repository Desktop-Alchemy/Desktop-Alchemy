use desktop_alchemy_core::model::{WidgetContent, WidgetData, WidgetSource};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Instant};

pub fn source(widget: &str, native_id: &str, name: String) -> WidgetSource {
    WidgetSource { ui_message: None,
        widget_id: widget.into(),
        id: format!("{widget}-{:x}", Sha256::digest(native_id.as_bytes())),
        name: name.chars().filter(|c| !c.is_control()).take(160).collect(),
        available: true,
        error: None,
    }
}
pub fn metric(widget: &str, source: Option<&WidgetSource>, title: &str) -> WidgetData {
    WidgetData {
        widget_id: widget.into(),
        source_id: source.map(|s| s.id.clone()),
        title: title.chars().filter(|c| !c.is_control()).take(32).collect(),
        value: "—".into(),
        secondary_value: None,
        detail: None,
        progress: None,
        valid: false,
        presentations: BTreeMap::new(),
    }
}
fn content(data: &WidgetData) -> WidgetContent {
    WidgetContent {
        title: data.title.clone(),
        value: data.value.clone(),
        secondary_value: data.secondary_value.clone(),
        detail: data.detail.clone(),
        progress: data.progress,
        valid: data.valid,
    }
}
pub fn capacity(bytes: u64) -> String {
    let (divisor, unit) = if bytes >= 1_u64 << 40 {
        ((1_u64 << 40) as f64, "TiB")
    } else {
        ((1_u64 << 30) as f64, "GiB")
    };
    format!("{:.1} {unit}", bytes as f64 / divisor)
}
pub fn disk(source: &WidgetSource, title: &str, value: Option<(u64, u64)>) -> WidgetData {
    let mut data = metric("disk", Some(source), title);
    if let Some((total, free)) = value.filter(|(total, free)| *total > 0 && free <= total) {
        data.valid = true;
        let formatted = capacity(free);
        let (number, unit) = formatted.split_once(' ').unwrap();
        data.value = number.into();
        data.detail = Some(format!("{unit} /{}", capacity(total)));
        data.progress = Some(free as f64 / total as f64);
        let mut used = data.clone();
        let formatted = capacity(total - free);
        let (number, unit) = formatted.split_once(' ').unwrap();
        used.value = number.into();
        used.detail = Some(format!("{unit} /{}", capacity(total)));
        used.progress = Some(1.0 - data.progress.unwrap());
        let mut percent = used.clone();
        percent.value = format!("{:.0}%", used.progress.unwrap() * 100.0);
        percent.detail = Some(format!("{}/{}", capacity(total - free), capacity(total)));
        data.presentations.insert("used".into(), content(&used));
        data.presentations
            .insert("percent".into(), content(&percent));
    } else {
        for id in ["used", "percent"] {
            data.presentations.insert(id.into(), content(&data));
        }
    }
    data
}
pub fn speed(bytes: f64) -> String {
    let (value, unit) = if bytes >= 1_073_741_824.0 {
        (bytes / 1_073_741_824.0, "GiB/s")
    } else if bytes >= 1_048_576.0 {
        (bytes / 1_048_576.0, "MiB/s")
    } else if bytes >= 1024.0 {
        (bytes / 1024.0, "KiB/s")
    } else {
        return format!("{bytes:.0} B/s");
    };
    format!("{value:.1} {unit}")
}
#[derive(Default)]
pub struct NetworkBaseline {
    previous: Option<(Instant, u64, u64)>,
}
impl NetworkBaseline {
    pub fn sample(&mut self, now: Instant, counters: Option<(u64, u64)>) -> Option<(f64, f64)> {
        let previous = self.previous.take();
        let (received, sent) = counters?;
        self.previous = Some((now, received, sent));
        let (before, old_received, old_sent) = previous?;
        let elapsed = now.checked_duration_since(before)?.as_secs_f64();
        if elapsed <= 0.0 || elapsed > 5.0 {
            return None;
        }
        Some((
            received.checked_sub(old_received)? as f64 / elapsed,
            sent.checked_sub(old_sent)? as f64 / elapsed,
        ))
    }
}
pub fn network(source: &WidgetSource, value: Option<(f64, f64)>) -> WidgetData {
    let mut data = metric("network", Some(source), &source.name);
    if let Some((down, up)) =
        value.filter(|(d, u)| d.is_finite() && u.is_finite() && *d >= 0.0 && *u >= 0.0)
    {
        data.value = speed(down);
        data.secondary_value = Some(speed(up));
        data.valid = true;
    }
    for (id, secondary) in [("download", false), ("upload", true)] {
        let mut single = data.clone();
        if secondary {
            single.value = data.secondary_value.clone().unwrap_or_else(|| "—".into());
        }
        single.secondary_value = None;
        let direction = if secondary { "上传" } else { "下载" };
        if let Some((number, unit)) = single.value.split_once(' ') {
            single.detail = Some(format!("{direction} {unit}"));
            single.value = number.into();
        } else {
            single.detail = Some(direction.into());
        }
        data.presentations.insert(id.into(), content(&single));
    }
    data
}
pub struct BatteryReading {
    pub present: bool,
    pub percent: Option<u8>,
    pub charging: bool,
    pub ac: Option<bool>,
    pub seconds: Option<u32>,
}
pub fn battery(value: Option<BatteryReading>) -> WidgetData {
    let mut data = metric("battery", None, "电池");
    let mut remaining = data.clone();
    remaining.title = "剩余时间".into();
    if let Some(BatteryReading {
        present: true,
        percent,
        charging,
        ac,
        seconds,
    }) = value
    {
        data.detail = Some(
            if charging {
                "充电中"
            } else if ac == Some(true) {
                "已接电源"
            } else if ac == Some(false) {
                "使用电池"
            } else {
                "电源状态未知"
            }
            .into(),
        );
        if let Some(percent) = percent.filter(|p| *p <= 100) {
            data.value = format!("{percent}%");
            data.progress = Some(percent as f64 / 100.0);
            data.valid = true;
        }
        if let Some(seconds) = seconds.filter(|s| *s != u32::MAX) {
            remaining.value = format!("{}:{:02}", seconds / 3600, seconds / 60 % 60);
            remaining.detail = Some("小时:分钟".into());
            remaining.valid = true;
        }
    }
    data.presentations
        .insert("remaining".into(), content(&remaining));
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn capacity_formats_and_invalid_values() {
        let s = source("disk", "volume-a", "C:".into());
        let d = disk(&s, "C:", Some((1_u64 << 40, 1_u64 << 39)));
        assert_eq!(d.value, "512.0");
        assert_eq!(d.detail.as_deref(), Some("GiB /1.0 TiB"));
        assert_eq!(d.presentations["percent"].value, "50%");
        assert!(!disk(&s, "C:", Some((1, 2))).valid);
        assert_eq!(source("disk", "volume-a", "D:".into()).id, s.id);
    }
    #[test]
    fn network_zero_disconnect_reset_and_elapsed_time() {
        let mut b = NetworkBaseline::default();
        let t = Instant::now();
        assert_eq!(b.sample(t, Some((100, 200))), None);
        assert_eq!(
            b.sample(t + Duration::from_secs(2), Some((2148, 200))),
            Some((1024.0, 0.0))
        );
        assert_eq!(b.sample(t + Duration::from_secs(3), Some((1, 2))), None);
        assert_eq!(
            b.sample(t + Duration::from_secs(4), Some((1, 2))),
            Some((0.0, 0.0))
        );
        assert_eq!(b.sample(t + Duration::from_secs(5), None), None);
        assert_eq!(b.sample(t + Duration::from_secs(6), Some((3, 4))), None);
        assert_eq!(b.sample(t + Duration::from_secs(20), Some((5, 6))), None);
        assert_eq!(speed(1024.0), "1.0 KiB/s");
    }
    #[test]
    fn battery_absent_unknown_and_remaining() {
        assert!(
            !battery(Some(BatteryReading {
                present: false,
                percent: None,
                charging: false,
                ac: None,
                seconds: None
            }))
            .valid
        );
        let d = battery(Some(BatteryReading {
            present: true,
            percent: Some(42),
            charging: false,
            ac: Some(false),
            seconds: Some(5400),
        }));
        assert_eq!(d.value, "42%");
        assert_eq!(d.presentations["remaining"].value, "1:30");
        assert!(
            !battery(Some(BatteryReading {
                present: true,
                percent: Some(50),
                charging: true,
                ac: Some(true),
                seconds: None
            }))
            .presentations["remaining"]
                .valid
        );
    }
}
