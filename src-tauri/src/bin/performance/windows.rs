use super::{metrics, Collected};
use anyhow::{bail, Result};
use std::{collections::BTreeMap, ptr, time::Instant};
use windows_sys::Win32::{
    Foundation::*,
    NetworkManagement::IpHelper::*,
    Storage::FileSystem::*,
    System::{
        Power::*,
        WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE},
    },
};

fn text(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())])
}
struct Volume(HANDLE);
impl Drop for Volume {
    fn drop(&mut self) {
        unsafe {
            FindVolumeClose(self.0);
        }
    }
}

pub fn disks() -> Result<Collected> {
    let mut result = Collected::default();
    let mut volume = vec![0_u16; 1024];
    let handle = unsafe { FindFirstVolumeW(volume.as_mut_ptr(), volume.len() as u32) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().into());
    }
    let handle = Volume(handle);
    loop {
        let mut paths = vec![0_u16; 32768];
        let mut size = 0;
        let kind = unsafe { GetDriveTypeW(volume.as_ptr()) };
        if [DRIVE_FIXED, DRIVE_REMOVABLE].contains(&kind)
            && unsafe {
                GetVolumePathNamesForVolumeNameW(
                    volume.as_ptr(),
                    paths.as_mut_ptr(),
                    paths.len() as u32,
                    &mut size,
                )
            } != 0
        {
            let mount = text(&paths);
            if !mount.is_empty() {
                let mut label = vec![0_u16; 261];
                unsafe {
                    GetVolumeInformationW(
                        volume.as_ptr(),
                        label.as_mut_ptr(),
                        label.len() as u32,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        0,
                    );
                }
                let title = mount.trim_end_matches('\\');
                let mut source = metrics::source(
                    "disk",
                    &text(&volume).to_ascii_lowercase(),
                    format!("{title} · {}", text(&label)),
                );
                let (mut available, mut total, mut free) = (0, 0, 0);
                let ok = unsafe {
                    GetDiskFreeSpaceExW(volume.as_ptr(), &mut available, &mut total, &mut free)
                } != 0;
                if !ok {
                    source.available = false;
                    source.error = Some("卷暂时不可读取".into());
                }
                result.data.push(metrics::disk(
                    &source,
                    title,
                    ok.then_some((total, available)),
                ));
                result.sources.push(source);
            }
        }
        if result.sources.len() >= 64 {
            break;
        }
        if unsafe { FindNextVolumeW(handle.0, volume.as_mut_ptr(), volume.len() as u32) } == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_NO_MORE_FILES {
                bail!("卷枚举失败：{error}");
            }
            break;
        }
    }
    Ok(result)
}
pub fn battery() -> desktop_alchemy_core::model::WidgetData {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        let mut data = metrics::battery(None);
        data.detail = Some("电池状态读取失败".into());
        return data;
    }
    let present = status.BatteryFlag != 255 && status.BatteryFlag & 128 == 0;
    let value = metrics::BatteryReading {
        present,
        percent: (status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent),
        charging: status.BatteryFlag != 255 && status.BatteryFlag & 8 != 0,
        ac: match status.ACLineStatus {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        },
        seconds: (status.BatteryLifeTime != u32::MAX).then_some(status.BatteryLifeTime),
    };
    let mut data = metrics::battery(Some(value));
    if !present {
        data.detail = Some(
            if status.BatteryFlag == 255 {
                "电池状态未知"
            } else {
                "无系统电池"
            }
            .into(),
        );
    }
    data
}
struct Interfaces(*mut MIB_IF_TABLE2);
impl Drop for Interfaces {
    fn drop(&mut self) {
        unsafe {
            FreeMibTable(self.0.cast());
        }
    }
}
pub fn network(baselines: &mut BTreeMap<String, metrics::NetworkBaseline>) -> Result<Collected> {
    let mut table = ptr::null_mut();
    let error = unsafe { GetIfTable2(&mut table) };
    if error != 0 {
        baselines.clear();
        bail!("网卡枚举失败：{error}");
    }
    let table = Interfaces(table);
    let mut result = Collected::default();
    let rows = unsafe {
        std::slice::from_raw_parts((*table.0).Table.as_ptr(), (*table.0).NumEntries as usize)
    };
    let now = Instant::now();
    // FilterInterface is an NDIS/WFP filter layer, not a separately selectable
    // network adapter. Keep physical, tunnel and virtual adapters themselves.
    for row in rows
        .iter()
        .filter(|r| r.Type != 24 && r.InterfaceAndOperStatusFlags._bitfield & 2 == 0)
        .take(128)
    {
        let guid = &row.InterfaceGuid;
        let native = format!(
            "{:08x}-{:04x}-{:04x}-{:02x?}",
            guid.data1, guid.data2, guid.data3, guid.data4
        );
        let alias = text(&row.Alias);
        let description = text(&row.Description);
        let mut source = metrics::source(
            "network",
            &native,
            if alias == description {
                alias
            } else {
                format!("{alias} · {description}")
            },
        );
        source.available = row.OperStatus == 1;
        if !source.available {
            source.error = Some("网卡已断开".into());
        }
        let value = baselines.entry(source.id.clone()).or_default().sample(
            now,
            source.available.then_some((row.InOctets, row.OutOctets)),
        );
        let mut data = metrics::network(&source, value);
        // The full name remains in the selector; the key uses the interface alias.
        data.title = text(&row.Alias)
            .chars()
            .filter(|c| !c.is_control())
            .take(32)
            .collect();
        result.data.push(data);
        result.sources.push(source);
    }
    baselines.retain(|id, _| result.sources.iter().any(|s| &s.id == id));
    Ok(result)
}
