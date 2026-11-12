//! Audio Dock configuration reports, recovered from Accessory Center 5.193.137.0.
//! No public raw-message entry point. Requests are feature 0x24; replies input 0x27.
use super::types::*;
use anyhow::{bail, ensure, Context, Result};

pub(super) trait Transport {
    fn exchange(&mut self, command: u8, payload: &[u8]) -> Result<Vec<u8>>;
}
pub(super) fn request(command: u8, payload: &[u8]) -> Result<[u8; 64]> {
    ensure!(payload.len() <= 61, alchemy_device_api::UiFailure::new("backend.e040", "设置请求过长"));
    let mut report = [0; 64];
    report[..3].copy_from_slice(&[0x24, command, payload.len() as u8]);
    report[3..3 + payload.len()].copy_from_slice(payload);
    Ok(report)
}
pub(super) fn response(command: u8, bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(bytes.len() >= 3 && bytes.len() <= 64, alchemy_device_api::UiFailure::new("backend.e041", "设备回复长度错误"));
    ensure!(
        bytes[0] == 0x27 && bytes[1] == command, alchemy_device_api::UiFailure::new("backend.e042", "设备回复与当前请求不匹配"));
    ensure!(bytes[2] == 0, "设备拒绝设置请求（状态 {:02X}）", bytes[2]);
    // GET_INPUT_REPORT returns an unpadded three-byte setter ACK; interrupt
    // captures carry the same ACK padded to 64 bytes, including a zero length.
    if bytes.len() == 3 {
        ensure!(
            matches!(
                command,
                0x34 | 0x36 | 0x3a | 0x60 | 0x40 | 0x42 | 0x44 | 0x4e
            ), alchemy_device_api::UiFailure::new("backend.e043", "设备字段回复缺少长度"));
        return Ok(vec![]);
    }
    let length = usize::from(bytes[3]);
    ensure!(
        length <= 60 && length + 4 <= bytes.len(), alchemy_device_api::UiFailure::new("backend.e044", "设备回复数据不完整"));
    Ok(bytes[4..4 + length].to_vec())
}
fn exact(data: Vec<u8>, length: usize) -> Result<Vec<u8>> {
    ensure!(data.len() == length, alchemy_device_api::UiFailure::new("backend.e045", "设备字段长度错误"));
    Ok(data)
}
fn boolean(data: Vec<u8>) -> Result<bool> {
    let data = exact(data, 1)?;
    match data[0] {
        0 => Ok(false),
        1 => Ok(true),
        _ => bail!(alchemy_device_api::UiFailure::new("backend.b330", "设备返回未知开关值")),
    }
}
fn text(data: Vec<u8>) -> Result<String> {
    let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
    let value = std::str::from_utf8(&data[..end]).context(alchemy_device_api::UiFailure::new("backend.b331", "设备文本编码错误"))?;
    ensure!(
        !value.is_empty() && !value.chars().any(char::is_control), alchemy_device_api::UiFailure::new("backend.e046", "设备文本无效"));
    Ok(value.into())
}
pub(super) fn preset(data: &[u8]) -> Result<AudioDockEqualizerPreset> {
    ensure!(data.len() >= 8 && data[0] < 16, alchemy_device_api::UiFailure::new("backend.e047", "均衡器预设数据无效"));
    let n = usize::from(data[1]);
    ensure!(
        n > 0 && n <= 19 && data.len() == n + 8 && data[n + 2] == 5, alchemy_device_api::UiFailure::new("backend.e048", "均衡器名称或频段长度错误"));
    let name = text(data[2..2 + n].to_vec())?;
    let mut gains = [0; 5];
    // Device order is high -> low. The public/UI order is low -> high.
    for (value, byte) in gains.iter_mut().zip(data[n + 3..].iter().rev()) {
        *value = *byte as i8;
    }
    ensure!(
        gains.iter().all(|g| (-12..=12).contains(g)), alchemy_device_api::UiFailure::new("backend.e049", "均衡器频段超出支持范围"));
    Ok(AudioDockEqualizerPreset {
        id: data[0],
        name,
        gains,
        factory: data[0] < 5,
    })
}
fn encode_preset(id: u8, name: &str, gains: &[i8; 5]) -> Result<Vec<u8>> {
    ensure!((5..16).contains(&id), alchemy_device_api::UiFailure::new("backend.e050", "不能覆盖工厂预设"));
    ensure!(
        !name.trim().is_empty() && name.len() <= 19 && !name.chars().any(char::is_control), alchemy_device_api::UiFailure::new("backend.e051", "预设名称须为 1–19 个 UTF-8 字节且不含控制字符"));
    ensure!(
        gains.iter().all(|g| (-12..=12).contains(g)), alchemy_device_api::UiFailure::new("backend.e052", "均衡器增益须为 −12 到 +12 dB"));
    let mut data = vec![id, name.len() as u8];
    data.extend_from_slice(name.as_bytes());
    data.push(5);
    data.extend(gains.iter().rev().map(|g| *g as u8));
    Ok(data)
}
pub(super) fn read(t: &mut impl Transport, device_id: &str) -> Result<AudioDockState> {
    let mut errors = vec![];
    // Protocol errors remain unknown; I/O errors abort in the transport on subsequent calls.
    macro_rules! field {
        ($label:literal, $expr:expr) => {
            match $expr {
                Ok(v) => Some(v),
                Err(e) => {
                    errors.push(format!("{}：{e:#}", $label));
                    None
                }
            }
        };
    }
    let tone = field!(
        "提示音",
        t.exchange(0x35, &[])
            .and_then(|v| exact(v, 8))
            .map(|v| v.iter().any(|b| *b != 0))
    );
    let volume = field!(
        "提示音音量",
        t.exchange(0x37, &[]).and_then(|v| {
            let v = exact(v, 1)?;
            ensure!(v[0] <= 100, alchemy_device_api::UiFailure::new("backend.e053", "未知音量范围"));
            Ok(v[0])
        })
    );
    let ptt = field!("按下即可发言", t.exchange(0x3b, &[]).and_then(boolean));
    let noise = field!(
        "麦克风降噪",
        t.exchange(0x61, &[]).and_then(|v| {
            let v = exact(v, 1)?;
            match v[0] {
                0 => Ok(AudioDockNoiseSuppression::Standard),
                1 => Ok(AudioDockNoiseSuppression::Low),
                _ => bail!(alchemy_device_api::UiFailure::new("backend.b332", "未知降噪模式")),
            }
        })
    );
    let active = field!(
        "当前均衡器",
        t.exchange(0x45, &[]).and_then(|v| {
            let v = exact(v, 2)?;
            ensure!(v[0] < 16 && v[1] == 0, alchemy_device_api::UiFailure::new("backend.e054", "未知均衡器索引"));
            Ok(v[0])
        })
    );
    let occupied = field!(
        "均衡器列表",
        t.exchange(0x43, &[]).and_then(|v| {
            let v = exact(v, 2)?;
            Ok(u16::from_le_bytes([v[0], v[1]]))
        })
    );
    let mut presets = vec![];
    let mut free = vec![];
    if let Some(mask) = occupied {
        for id in 0..16 {
            if mask & (1 << id) != 0 {
                if let Some(p) = field!(
                    "均衡器预设",
                    t.exchange(0x41, &[id]).and_then(|v| {
                        let p = preset(&v)?;
                        ensure!(p.id == id, alchemy_device_api::UiFailure::new("backend.e055", "均衡器预设索引不匹配"));
                        Ok(p)
                    })
                ) {
                    presets.push(p);
                }
            } else if id >= 5 {
                free.push(id);
            }
        }
    }
    let serial = field!("序列号", t.exchange(0x18, &[]).and_then(text));
    let color_code = field!("颜色代码", t.exchange(0x1c, &[]).and_then(text));
    let hardware_version = field!(
        "硬件版本",
        t.exchange(0x02, &[]).and_then(|v| {
            ensure!(v.len() >= 7, alchemy_device_api::UiFailure::new("backend.e056", "硬件信息不完整"));
            Ok(format!("{}.{}.{}", v[6] >> 4, v[6] & 15, v[5]))
        })
    );
    let firmware_version = field!(
        "固件版本",
        t.exchange(0x2f, &[]).and_then(|v| {
            let v = exact(v, 4)?;
            Ok(format!(
                "{}.{}.{}",
                v[3],
                u16::from_le_bytes([v[1], v[2]]),
                v[0]
            ))
        })
    );
    Ok(AudioDockState { ui_message: None,
        device_id: device_id.into(),
        connection: if errors.is_empty() {
            AudioDockConnection::Ready
        } else {
            AudioDockConnection::Error
        },
        settings: AudioDockSettings {
            prompt_tone: tone,
            prompt_volume: volume,
            push_to_talk: ptt,
            noise_suppression: noise,
            active_equalizer: active,
            equalizer_presets: presets,
            free_equalizer_slots: free,
        },
        info: AudioDockDeviceInfo {
            model: Some("Audio Dock".into()),
            serial,
            hardware_version,
            color_code,
            firmware_version,
        },
        error: (!errors.is_empty()).then(|| errors.join("；")),
    })
}
pub(super) fn update(
    t: &mut impl Transport,
    device_id: &str,
    update: &AudioDockSettingsUpdate,
) -> Result<AudioDockState> {
    let before = read(t, device_id)?;
    ensure!(
        before.connection == AudioDockConnection::Ready, alchemy_device_api::UiFailure::new("backend.e057", "读取设备当前设置失败，请先重新读取"));
    let s = &before.settings;
    let mut saved_id = None;
    match update {
        AudioDockSettingsUpdate::PromptTone { enabled } => {
            t.exchange(0x34, &[if *enabled { 255 } else { 0 }; 8])?;
        }
        AudioDockSettingsUpdate::PromptVolume { value } => {
            ensure!(*value <= 100, alchemy_device_api::UiFailure::new("backend.e058", "音量须为 0–100"));
            t.exchange(0x36, &[*value])?;
        }
        AudioDockSettingsUpdate::PushToTalk { enabled } => {
            t.exchange(0x3a, &[u8::from(*enabled)])?;
        }
        AudioDockSettingsUpdate::NoiseSuppression { mode } => {
            t.exchange(
                0x60,
                &[match mode {
                    AudioDockNoiseSuppression::Standard => 0,
                    AudioDockNoiseSuppression::Low => 1,
                }],
            )?;
        }
        AudioDockSettingsUpdate::SelectEqualizer { id } => {
            ensure!(
                s.equalizer_presets.iter().any(|p| p.id == *id), alchemy_device_api::UiFailure::new("backend.e059", "均衡器预设不存在"));
            t.exchange(0x44, &[*id, 0])?;
        }
        AudioDockSettingsUpdate::SaveEqualizer { id, name, gains } => {
            let id = match id {
                Some(id) => {
                    ensure!(
                        s.equalizer_presets
                            .iter()
                            .any(|p| p.id == *id && !p.factory && p.name == *name), alchemy_device_api::UiFailure::new("backend.e060", "覆盖目标已改变，请重新读取"));
                    *id
                }
                None => *s
                    .free_equalizer_slots
                    .first()
                    .context(alchemy_device_api::UiFailure::new("backend.b333", "没有可用的自定义预设槽位"))?,
            };
            ensure!(
                !s.equalizer_presets
                    .iter()
                    .any(|p| p.id != id && p.name == *name), alchemy_device_api::UiFailure::new("backend.e061", "同名预设已存在"));
            t.exchange(0x40, &encode_preset(id, name, gains)?)?;
            let saved = preset(&t.exchange(0x41, &[id])?)?;
            ensure!(
                saved.id == id && saved.name == *name && saved.gains == *gains, alchemy_device_api::UiFailure::new("backend.e062", "预设写入读回不一致，未切换预设"));
            t.exchange(0x44, &[id, 0])?;
            saved_id = Some(id);
        }
        AudioDockSettingsUpdate::DeleteEqualizer { id, name } => {
            ensure!(
                (5..16).contains(id)
                    && s.equalizer_presets
                        .iter()
                        .any(|p| p.id == *id && !p.factory && p.name == *name), alchemy_device_api::UiFailure::new("backend.e063", "删除目标已改变或为工厂预设，请重新读取"));
            if s.active_equalizer == Some(*id) {
                ensure!(
                    s.equalizer_presets.iter().any(|p| p.id == 0 && p.factory), alchemy_device_api::UiFailure::new("backend.e064", "未读取到 Flat 预设"));
                t.exchange(0x44, &[0, 0])?;
                ensure!(
                    exact(t.exchange(0x45, &[])?, 2)? == [0, 0], alchemy_device_api::UiFailure::new("backend.e065", "切换 Flat 失败，未删除预设"));
            }
            // Audio Dock uses one slot byte. Other Microsoft audio products use
            // a different 0x42 format; never reuse their enable/disable payload.
            t.exchange(0x42, &[*id])?;
        }
    }
    let mut after = read(t, device_id).context(alchemy_device_api::UiFailure::new("backend.b334", "设置已发送，但读回失败；请重新读取，勿重复提交"))?;
    ensure!(
        after.connection == AudioDockConnection::Ready, alchemy_device_api::UiFailure::new("backend.e066", "设置已发送，但读回不完整；请重新读取"));
    let a = &after.settings;
    let matches = match update {
        AudioDockSettingsUpdate::PromptTone { enabled } => a.prompt_tone == Some(*enabled),
        AudioDockSettingsUpdate::PromptVolume { value } => a.prompt_volume == Some(*value),
        AudioDockSettingsUpdate::PushToTalk { enabled } => a.push_to_talk == Some(*enabled),
        AudioDockSettingsUpdate::NoiseSuppression { mode } => {
            a.noise_suppression.as_ref() == Some(mode)
        }
        AudioDockSettingsUpdate::SelectEqualizer { id } => a.active_equalizer == Some(*id),
        AudioDockSettingsUpdate::SaveEqualizer { name, gains, .. } => {
            a.active_equalizer == saved_id
                && a.equalizer_presets
                    .iter()
                    .any(|p| Some(p.id) == saved_id && p.name == *name && p.gains == *gains)
        }
        AudioDockSettingsUpdate::DeleteEqualizer { id, .. } => {
            a.active_equalizer
                == if s.active_equalizer == Some(*id) {
                    Some(0)
                } else {
                    s.active_equalizer
                }
                && a.free_equalizer_slots.contains(id)
                && !a.equalizer_presets.iter().any(|p| p.id == *id)
                && s.equalizer_presets
                    .iter()
                    .filter(|p| p.id != *id)
                    .all(|p| a.equalizer_presets.contains(p))
        }
    };
    ensure!(matches, alchemy_device_api::UiFailure::new("backend.e067", "设置已发送，但设备读回值不一致；请重新读取"));
    if matches!(
        update,
        AudioDockSettingsUpdate::PromptTone { .. } | AudioDockSettingsUpdate::PromptVolume { .. }
    ) {
        // Official Dock PlayTone report: independent audition, no persistent mask change.
        // Settings are already verified; do not misreport a failed preview as a failed write.
        if let Err(e) = t.exchange(0x4e, &[0x10, 0, 0, 0, 0, 0, 0, 0]) {
            after.error = Some(format!("设置已应用，但试听失败：{e:#}；请重新读取后再操作"));
            after.ui_message = Some(alchemy_device_api::UiMessage::new("errors.auditionFailed"));
            after.connection = AudioDockConnection::Error;
        }
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    struct Dock {
        values: BTreeMap<u8, Vec<u8>>,
        presets: BTreeMap<u8, Vec<u8>>,
        writes: Vec<u8>,
        reject_write: bool,
        reject_preview: bool,
    }
    impl Dock {
        fn new() -> Self {
            Self {
                values: BTreeMap::from([
                    (0x35, vec![255; 8]),
                    (0x37, vec![39]),
                    (0x3b, vec![1]),
                    (0x61, vec![0]),
                    (0x45, vec![0, 0]),
                    (0x18, b"TEST-SERIAL".to_vec()),
                    (0x1c, b"black".to_vec()),
                    (0x02, vec![1, 1, 0x16, 7, 11, 9, 0x31, 0]),
                    (0x2f, vec![139, 47, 0, 1]),
                ]),
                presets: BTreeMap::from([(0, b"\x00\x04Flat\x05\x00\x00\x00\x00\x00".to_vec())]),
                writes: vec![],
                reject_write: false,
                reject_preview: false,
            }
        }
    }
    impl Transport for Dock {
        fn exchange(&mut self, command: u8, payload: &[u8]) -> Result<Vec<u8>> {
            match command {
                0x4e => {
                    self.writes.push(command);
                    ensure!(payload == [0x10, 0, 0, 0, 0, 0, 0, 0], "invalid audition");
                    ensure!(!self.reject_preview, "preview timeout");
                    Ok(vec![])
                }
                0x42 => {
                    self.writes.push(command);
                    ensure!(!self.reject_write, "lost response");
                    ensure!(payload.len() == 1 && payload[0] >= 5, "invalid delete");
                    self.presets.remove(&payload[0]).context("missing preset")?;
                    Ok(vec![])
                }
                0x43 => Ok(self
                    .presets
                    .keys()
                    .fold(0u16, |mask, id| mask | (1 << id))
                    .to_le_bytes()
                    .to_vec()),
                0x41 => Ok(self
                    .presets
                    .get(&payload[0])
                    .context("missing preset")?
                    .clone()),
                0x40 => {
                    self.writes.push(command);
                    ensure!(!self.reject_write, "lost response");
                    self.presets.insert(payload[0], payload.to_vec());
                    Ok(vec![])
                }
                0x34 | 0x36 | 0x3a | 0x60 | 0x44 => {
                    self.writes.push(command);
                    ensure!(!self.reject_write, "lost response");
                    let get = if command == 0x60 { 0x61 } else { command + 1 };
                    self.values.insert(get, payload.to_vec());
                    Ok(vec![])
                }
                _ => Ok(self.values.get(&command).context("unknown query")?.clone()),
            }
        }
    }
    #[test]
    fn capture_reports_decode_signed_bands_and_reject_bad_frames() {
        let p = preset(&[
            1, 7, b'C', b'l', b'a', b's', b's', b'i', b'c', 5, 0, 0, 0, 2, 4,
        ])
        .unwrap();
        assert_eq!(p.gains, [4, 2, 0, 0, 0]);
        let custom = encode_preset(5, "test", &[-12, -1, 0, 1, 12]).unwrap();
        assert_eq!(preset(&custom).unwrap().gains, [-12, -1, 0, 1, 12]);
        let good = [0x27, 0x37, 0, 1, 39];
        assert_eq!(response(0x37, &good).unwrap(), vec![39]);
        assert!(response(0x36, &[0x27, 0x36, 0]).unwrap().is_empty());
        for bad in [
            &good[..3],
            &[0x27, 0x37, 0, 2, 39][..],
            &[0x27, 0x36, 0, 1, 39][..],
            &[0x27, 0x37, 3, 1, 39][..],
        ] {
            assert!(response(0x37, bad).is_err());
        }
        assert!(preset(&[5, 19, 0]).is_err());
    }
    #[test]
    fn updates_read_back_and_do_not_cross_device_or_overwrite_factory_slots() {
        let mut a = Dock::new();
        let mut b = Dock::new();
        let state = read(&mut a, "a").unwrap();
        assert_eq!(state.info.hardware_version.as_deref(), Some("3.1.9"));
        assert_eq!(state.info.firmware_version.as_deref(), Some("1.47.139"));
        assert_eq!(
            update(
                &mut a,
                "a",
                &AudioDockSettingsUpdate::PromptVolume { value: 40 }
            )
            .unwrap()
            .settings
            .prompt_volume,
            Some(40)
        );
        assert_eq!(read(&mut b, "b").unwrap().settings.prompt_volume, Some(39));
        assert_eq!(a.writes, [0x36, 0x4e]);
        assert!(update(
            &mut a,
            "a",
            &AudioDockSettingsUpdate::SaveEqualizer {
                id: Some(0),
                name: "Flat".into(),
                gains: [0; 5]
            }
        )
        .is_err());
        let custom = AudioDockSettingsUpdate::SaveEqualizer {
            id: None,
            name: "custom".into(),
            gains: [1, 2, 3, 4, 5],
        };
        let state = update(&mut a, "a", &custom).unwrap();
        assert_eq!(state.settings.active_equalizer, Some(5));
        assert_eq!(state.settings.equalizer_presets[1].gains, [1, 2, 3, 4, 5]);
        let count = a.writes.len();
        assert!(update(&mut a, "a", &custom).is_err());
        assert_eq!(a.writes.len(), count);
        a.reject_write = true;
        assert!(update(
            &mut a,
            "a",
            &AudioDockSettingsUpdate::PromptVolume { value: 41 }
        )
        .is_err());
        assert_eq!(a.writes.len(), count + 1); // No retry after uncertain write.
    }
    #[test]
    fn custom_delete_checks_identity_and_switches_before_removing_active_slot() {
        let mut dock = Dock::new();
        update(
            &mut dock,
            "a",
            &AudioDockSettingsUpdate::SaveEqualizer {
                id: None,
                name: "custom".into(),
                gains: [1; 5],
            },
        )
        .unwrap();
        dock.writes.clear();
        for (id, name) in [(0, "Flat"), (5, "stale")] {
            assert!(update(
                &mut dock,
                "a",
                &AudioDockSettingsUpdate::DeleteEqualizer {
                    id,
                    name: name.into()
                }
            )
            .is_err());
        }
        assert!(dock.writes.is_empty());
        let state = update(
            &mut dock,
            "a",
            &AudioDockSettingsUpdate::DeleteEqualizer {
                id: 5,
                name: "custom".into(),
            },
        )
        .unwrap();
        assert_eq!(dock.writes, [0x44, 0x42]);
        assert_eq!(state.settings.active_equalizer, Some(0));
        assert!(state.settings.free_equalizer_slots.contains(&5));
        assert_eq!(state.settings.equalizer_presets.len(), 1);
    }
    #[test]
    fn prompt_feedback_runs_once_after_readback_and_reports_partial_success() {
        let mut dock = Dock::new();
        for enabled in [false, true] {
            update(
                &mut dock,
                "a",
                &AudioDockSettingsUpdate::PromptTone { enabled },
            )
            .unwrap();
        }
        assert_eq!(dock.writes, [0x34, 0x4e, 0x34, 0x4e]);
        dock.reject_preview = true;
        let state = update(
            &mut dock,
            "a",
            &AudioDockSettingsUpdate::PromptVolume { value: 40 },
        )
        .unwrap();
        assert_eq!(state.settings.prompt_volume, Some(40));
        assert_eq!(state.connection, AudioDockConnection::Error);
        assert!(state.error.unwrap().contains("设置已应用，但试听失败"));
    }
    #[test]
    fn unknown_fields_stay_unknown_and_incomplete_reads_block_writes() {
        let mut dock = Dock::new();
        dock.values.insert(0x61, vec![42]);
        let state = read(&mut dock, "a").unwrap();
        assert_eq!(state.settings.noise_suppression, None);
        assert_eq!(state.connection, AudioDockConnection::Error);
        assert!(update(
            &mut dock,
            "a",
            &AudioDockSettingsUpdate::PromptVolume { value: 0 }
        )
        .is_err());
        assert!(dock.writes.is_empty());
    }
}
