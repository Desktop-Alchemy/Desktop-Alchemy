//! Built-in plugin: system collection is deliberately outside the device-owning process.
use desktop_alchemy_core::model::{WidgetContent, WidgetData};
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};
use std::time::Duration;

fn cpu_data(value: Option<f64>) -> WidgetData {
    let value = value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v.min(100.0));
    WidgetData {
        source_id: None,
        secondary_value: None,
        presentations: Default::default(),
        widget_id: "cpu".into(),
        title: "CPU".into(),
        value: value
            .map(|v| format!("{v:.0}%"))
            .unwrap_or_else(|| "—".into()),
        detail: None,
        progress: value.map(|v| v / 100.0),
        valid: value.is_some(),
    }
}
fn memory_data(value: Option<(u64, u64)>) -> WidgetData {
    let value = value.filter(|(total, available)| *total > 0 && available <= total);
    let fraction = value.map(|(total, available)| (total - available) as f64 / total as f64);
    let mut presentations = std::collections::BTreeMap::new();
    for (id, title, available_mode) in
        [("available", "剩余内存", true), ("used", "已用内存", false)]
    {
        let bytes = value.map(|(total, available)| {
            if available_mode {
                available
            } else {
                total - available
            }
        });
        presentations.insert(
            id.into(),
            WidgetContent {
                secondary_value: None,
                title: title.into(),
                value: bytes
                    .map(|v| format!("{:.1}", v as f64 / 1_073_741_824.0))
                    .unwrap_or_else(|| "—".into()),
                detail: value
                    .map(|(total, _)| format!("/{:.1} GiB", total as f64 / 1_073_741_824.0)),
                progress: value
                    .zip(bytes)
                    .map(|((total, _), bytes)| bytes as f64 / total as f64),
                valid: value.is_some(),
            },
        );
    }
    WidgetData {
        source_id: None,
        secondary_value: None,
        presentations,
        widget_id: "memory".into(),
        title: "内存".into(),
        value: fraction
            .map(|v| format!("{:.0}%", v * 100.0))
            .unwrap_or_else(|| "—".into()),
        detail: value.map(|(total, available)| {
            format!(
                "{:.1}/{:.1} GiB",
                (total - available) as f64 / 1_073_741_824.0,
                total as f64 / 1_073_741_824.0
            )
        }),
        progress: fraction,
        valid: value.is_some(),
    }
}

#[cfg(windows)]
mod system {
    use anyhow::{bail, Result};
    use std::ptr;
    use windows_sys::Win32::System::{Performance::*, SystemInformation::*};

    pub struct Cpu {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
    }
    impl Cpu {
        pub fn new() -> Result<Self> {
            let mut query = ptr::null_mut();
            let code = unsafe { PdhOpenQueryW(ptr::null(), 0, &mut query) };
            if code != 0 {
                bail!("PDH 查询失败：0x{code:08x}");
            }
            let mut cpu = Self {
                query,
                counter: ptr::null_mut(),
            };
            let path: Vec<_> = r"\Processor Information(_Total)\% Processor Time"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let code = unsafe { PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut cpu.counter) };
            if code != 0 {
                bail!("CPU 计数器不可用：0x{code:08x}");
            }
            let code = unsafe { PdhCollectQueryData(query) };
            if code != 0 {
                bail!("CPU 初始采样失败：0x{code:08x}");
            }
            Ok(cpu)
        }
        pub fn sample(&self) -> Result<f64> {
            let code = unsafe { PdhCollectQueryData(self.query) };
            if code != 0 {
                bail!("CPU 采样失败：0x{code:08x}");
            }
            let mut value = PDH_FMT_COUNTERVALUE::default();
            let code = unsafe {
                PdhGetFormattedCounterValue(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    ptr::null_mut(),
                    &mut value,
                )
            };
            if code != 0 || ![PDH_CSTATUS_VALID_DATA, PDH_CSTATUS_NEW_DATA].contains(&value.CStatus)
            {
                bail!("CPU 样本无效：0x{code:08x}/0x{:08x}", value.CStatus);
            }
            Ok(unsafe { value.Anonymous.doubleValue })
        }
    }
    impl Drop for Cpu {
        fn drop(&mut self) {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
    }
    pub fn memory() -> Result<(u64, u64)> {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok((status.ullTotalPhys, status.ullAvailPhys))
    }
}

#[derive(Clone, Default, serde::Deserialize)]
struct Collected {
    #[serde(default)]
    sources: Vec<desktop_alchemy_core::model::WidgetSource>,
    #[serde(default)]
    data: Vec<WidgetData>,
    #[serde(default)]
    hardware_status: Option<String>,
    #[serde(default)]
    message: Option<String>,
}
#[path = "performance/hardware.rs"]
mod hardware;
#[path = "performance/metrics.rs"]
mod metrics;
#[cfg(windows)]
#[path = "performance/windows.rs"]
mod native;

type Updates = tokio::sync::mpsc::Sender<(&'static str, Collected)>;
fn collect(tx: Updates, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    #[cfg(windows)]
    {
        let mut cpu = None;
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            let value = if let Some(query) = &cpu {
                match system::Cpu::sample(query) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        cpu = None;
                        None
                    }
                }
            } else {
                cpu = system::Cpu::new().ok();
                None
            };
            if tx
                .blocking_send((
                    "base",
                    Collected {
                        data: vec![cpu_data(value), memory_data(system::memory().ok())],
                        ..Default::default()
                    },
                ))
                .is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = stop;
        let _ = tx.blocking_send((
            "base",
            Collected {
                data: vec![cpu_data(None), memory_data(None)],
                ..Default::default()
            },
        ));
    }
}
#[cfg(windows)]
fn collect_native(tx: Updates, stop: std::sync::Arc<std::sync::atomic::AtomicBool>, network: bool) {
    let mut baselines = Default::default();
    let mut previous = Collected::default();
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        let result = if network {
            native::network(&mut baselines)
        } else {
            native::disks()
        };
        let collected = match result {
            Ok(c) => c,
            Err(e) => {
                let mut c = previous.clone();
                for s in &mut c.sources {
                    s.available = false;
                    s.error = Some(e.to_string().chars().take(512).collect());
                }
                for d in &mut c.data {
                    d.valid = false;
                    d.secondary_value = None;
                    for p in d.presentations.values_mut() {
                        p.valid = false;
                    }
                }
                c
            }
        };
        previous = collected.clone();
        if tx
            .blocking_send((if network { "network" } else { "disk" }, collected))
            .is_err()
        {
            break;
        }
        std::thread::sleep(Duration::from_secs(if network { 1 } else { 2 }));
    }
}
fn send(output: &std::sync::mpsc::SyncSender<Value>, value: Value) -> anyhow::Result<()> {
    output
        .try_send(value)
        .map_err(|error| anyhow::anyhow!("插件输出积压或连接已关闭：{error}"))
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // A stopped host may stop reading before shutdown reaches us. Never block the
    // protocol thread in WriteFile: the bounded writer has no device ownership.
    let (output, outgoing) = std::sync::mpsc::sync_channel::<Value>(32);
    std::thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        while let Ok(value) = outgoing.recv() {
            if writeln!(stdout, "{value}")
                .and_then(|_| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    });
    let (input_tx, mut input) = tokio::sync::mpsc::channel(32);
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        loop {
            let mut line = String::new();
            match reader.by_ref().take(1_048_577).read_line(&mut line) {
                Ok(0) | Err(_) => break,
                _ if line.len() > 1_048_576 || !line.ends_with('\n') => break,
                _ => {
                    if input_tx.blocking_send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let (updates_tx, mut updates) = tokio::sync::mpsc::channel(16);
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let (hardware_tx, hardware_rx) = tokio::sync::mpsc::channel(1);
    let mut hardware_task = None;
    let mut hardware_receiver = Some(hardware_rx);
    let mut groups = std::collections::BTreeMap::<&str, Collected>::new();
    let mut last_catalog = None;
    let mut initialized = false;
    let mut sequence = 0_u64;
    let protocol_result: anyhow::Result<()> = async {
    loop {
        tokio::select! {
            line = input.recv() => {
                let Some(line) = line else { break; };
                let msg: Value = serde_json::from_str(&line)?;
                match msg["method"].as_str() {
                    Some("initialize") if !initialized && msg["params"]["apiVersion"] == 3 => {
                        send(&output, json!({"jsonrpc":"2.0","id":msg["id"],"result":{"apiVersion":3}}))?;
                        initialized = true;
                        let tx=updates_tx.clone(); let stop=stopped.clone();
                        std::thread::spawn(move || collect(tx,stop));
                        #[cfg(windows)] for network in [false,true] {
                            let tx=updates_tx.clone(); let stop=stopped.clone();
                            std::thread::spawn(move || collect_native(tx,stop,network));
                        }
                        #[cfg(windows)] {
                            let tx=updates_tx.clone(); let stop=stopped.clone();
                            std::thread::spawn(move || {
                                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                                    if tx.blocking_send(("battery", Collected { data: vec![native::battery()], ..Default::default() })).is_err() { break; }
                                    std::thread::sleep(Duration::from_secs(2));
                                }
                            });
                        }
                        hardware_task=Some(tokio::spawn(hardware::run(updates_tx.clone(),hardware_receiver.take().unwrap(),stop_rx.clone())));
                    },
                    Some("sensors.enable") if initialized => { let _=hardware_tx.try_send(()); },
                    Some("ping") => send(&output, json!({"jsonrpc":"2.0","id":msg["id"],"result":{"ok":true}}))?,
                    Some("shutdown") => {
                        send(&output, json!({"jsonrpc":"2.0","id":msg["id"],"result":{"ok":true}}))?;
                        break;
                    },
                    _ => {},
                }
            },
            Some((group, values)) = updates.recv(), if initialized => {
                groups.insert(group,values.clone());
                let mut catalog=desktop_alchemy_core::model::WidgetCatalog::default();
                for c in groups.values() {
                    catalog.sources.extend(c.sources.clone());
                    if let Some(status)=&c.hardware_status {catalog.hardware_status=status.clone(); catalog.message=c.message.clone();}
                }
                if last_catalog.as_ref()!=Some(&catalog) {
                    send(&output, json!({"jsonrpc":"2.0","method":"widgets.sources.publish","params":catalog}))?;
                    last_catalog=Some(catalog);
                }
                sequence += 1;
                send(&output, json!({"jsonrpc":"2.0","id":format!("sample-{sequence}"),"method":"widgets.publish_batch","params":values.data}))?;
            }
        }
    }
    Ok(())
    }.await;
    stopped.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = stop_tx.send(true);
    if let Some(task) = hardware_task {
        task.await??;
    }
    protocol_result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_valid_and_unavailable_metrics() {
        assert_eq!(cpu_data(Some(27.6)).value, "28%");
        assert_eq!(cpu_data(Some(101.0)).progress, Some(1.0));
        assert!(!cpu_data(Some(f64::NAN)).valid);
        assert!(!cpu_data(None).valid);
        let data = memory_data(Some((32 * 1_073_741_824, 12 * 1_073_741_824)));
        assert_eq!(data.detail.as_deref(), Some("20.0/32.0 GiB"));
        assert_eq!(data.progress, Some(0.625));
        assert_eq!(data.presentations["available"].value, "12.0");
        assert_eq!(
            data.presentations["available"].detail.as_deref(),
            Some("/32.0 GiB")
        );
        assert_eq!(data.presentations["available"].progress, Some(0.375));
        assert_eq!(data.presentations["used"].value, "20.0");
        assert_eq!(data.presentations["used"].progress, Some(0.625));
        assert!(memory_data(None)
            .presentations
            .values()
            .all(|v| !v.valid && v.progress.is_none()));
        assert!(!memory_data(Some((0, 0))).valid);
        assert!(!memory_data(Some((1, 2))).valid);
    }
}
