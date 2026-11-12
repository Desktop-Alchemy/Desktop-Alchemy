use desktop_alchemy_core::{core::Core, model::*, render, storage};
use std::{path::PathBuf, time::Duration};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let sensors = args.iter().any(|s| s == "--performance-sensors");
    let performance = sensors || args.iter().any(|s| s == "--performance-widgets");
    let led_effects = args.iter().any(|s| s == "--led-effects");
    let seconds = args
        .windows(2)
        .find(|a| a[0] == "--seconds")
        .and_then(|a| a[1].parse::<u64>().ok())
        .unwrap_or(15);
    let root = args
        .windows(2)
        .find(|a| a[0] == "--data-dir")
        .map(|a| PathBuf::from(&a[1]))
        .unwrap_or_else(|| {
            std::env::temp_dir().join(if performance {
                format!("m18-performance-{}", uuid::Uuid::new_v4())
            } else {
                "m18-diagnostic".into()
            })
        });
    if args.iter().any(|s| s == "--render-widgets") {
        return render_widget_examples(&root);
    }
    if args.iter().any(|s| s == "--managed-devices") {
        return managed_devices(root, args.iter().any(|s| s == "--exercise"), seconds).await;
    }
    if performance && root.join("config.json").exists() {
        anyhow::bail!("性能专项须使用新的独立数据目录，不能覆盖现有配置");
    }
    let host = Core::new(root)?;
    let devices = desktop_alchemy_core::devices::DeviceManager::new(host.clone())?;
    devices.start(None, true);
    let c = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let snapshot = devices.snapshot()?;
            let connected = snapshot
                .devices
                .iter()
                .filter(|d| {
                    d.adapter_id == desktop_alchemy_core::devices::m18::ID
                        && d.state.connection == "ready"
                })
                .collect::<Vec<_>>();
            if connected.len() > 1 {
                anyhow::bail!("请明确指定诊断设备")
            }
            if let Some(device) = connected.first() {
                return devices.device(device.device_id.as_deref().unwrap());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    let mut stopped = c.stop.subscribe();
    let task = tokio::spawn(async move {
        let _ = stopped.changed().await;
        devices.shutdown().await;
        let _ = host.stop.send(true);
    });
    if performance {
        let manager = desktop_alchemy_core::plugins::PluginManager::new(c.clone());
        let resources = args
            .windows(2)
            .find(|a| a[0] == "--plugin-resources")
            .map(|a| PathBuf::from(&a[1]))
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".."));
        let result = async {
            manager.register_performance(&resources)?;
            performance_widgets(c.clone(), manager.clone(), seconds, sensors).await
        }
        .await;
        c.stop.send_replace(true);
        manager.stop_all().await;
        let stopped = tokio::time::timeout(Duration::from_secs(5), task).await;
        if stopped.is_err() {
            anyhow::bail!("性能专项 SDK 退出超时");
        }
        return result;
    }
    if led_effects {
        let result = led_effect_tour(c.clone()).await;
        if let Err(e) = c.reset_led_for_shutdown().await {
            eprintln!("LED default restore: {e:#}");
        }
        if let Err(e) = &result {
            eprintln!("LED effect tour: {e:#}");
        }
        let _ = c.stop.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(4), task).await;
        println!("LED effect tour finished; it did not change the saved LED configuration.");
        return result;
    }
    let mut events = c.events.subscribe();
    let end = tokio::time::Instant::now() + Duration::from_secs(seconds);
    if args.iter().any(|s| s == "--exercise") {
        let core = c.clone();
        tokio::spawn(async move {
            if let Err(e) = exercise(core).await {
                eprintln!("Hardware exercise: {e:#}")
            }
        });
    }
    let mut health = tokio::time::interval(Duration::from_secs(10));
    let started = now();
    let mut sample_count = 0;
    if args.iter().any(|s| s == "--page-tour") {
        let core = c.clone();
        tokio::spawn(async move {
            let result: anyhow::Result<()> = async {
                tokio::time::timeout(Duration::from_secs(20), async {
                    while core.snapshot().connection != "ready" {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await?;
                tokio::time::sleep(Duration::from_secs(2)).await;
                let config = core.snapshot().config;
                for page in config.pages.iter().cycle().take(config.pages.len() * 2) {
                    println!("Page tour: {}", page.name);
                    core.activate(&page.id).await?;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                core.activate(&config.active_page).await?;
                Ok(())
            }
            .await;
            if let Err(e) = result {
                eprintln!("Page tour: {e:#}");
            }
        });
    }
    loop {
        tokio::select! {_=tokio::time::sleep_until(end)=>break,e=events.recv()=>if let Ok(e)=e{println!("{}",serde_json::to_string(&e)?);},_=health.tick()=>{use std::io::Write;let s=c.snapshot();let row=serde_json::json!({"startedAt":started,"at":now(),"connection":s.connection,"sdkPid":s.sdk_pid,"devices":s.devices.len(),"queue":c.inner.lock().unwrap().queue.len()});let mut f=std::fs::OpenOptions::new().create(true).append(true).open(c.root.join("session-health.jsonl"))?;writeln!(f,"{row}")?;sample_count+=1;if sample_count%6==0&&s.selected_device.is_some(){let _=c.enqueue(HardwareCommand::Firmware,None);let _=c.export_report();}}}
    }
    println!("{}", serde_json::to_string_pretty(&c.snapshot())?);
    println!("Report: {}", c.export_report()?.display());
    let _ = c.stop.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(4), task).await;
    Ok(())
}

fn render_widget_examples(root: &std::path::Path) -> anyhow::Result<()> {
    let manifest: PluginManifest =
        serde_json::from_str(include_str!("../../../plugins/performance/plugin.json"))?;
    std::fs::create_dir_all(root)?;
    for (definition, value, detail, progress) in [
        (&manifest.widgets[0], "28%", None, Some(0.28)),
        (
            &manifest.widgets[1],
            "62%",
            Some("19.8/32.0 GiB"),
            Some(0.62),
        ),
    ] {
        let binding = WidgetBinding {
            source_id: None,
            source_name: None,
            presentation_id: None,
            plugin_id: manifest.id.clone(),
            widget_id: definition.id.clone(),
            style: definition.default_style.clone(),
        };
        let data = WidgetData {
            source_id: None,
            secondary_value: None,
            presentations: Default::default(),
            widget_id: definition.id.clone(),
            title: definition.name.clone(),
            value: value.into(),
            detail: detail.map(str::to_owned),
            progress,
            valid: true,
        };
        std::fs::write(
            root.join(format!("{}.png", definition.id)),
            render::widget(&binding, &data)?,
        )?;
        let waiting = WidgetData {
            presentations: Default::default(),
            valid: false,
            ..data
        };
        std::fs::write(
            root.join(format!("{}-waiting.png", definition.id)),
            render::widget(&binding, &waiting)?,
        )?;
    }
    let binding = WidgetBinding {
        source_id: None,
        source_name: None,
        presentation_id: None,
        plugin_id: manifest.id.clone(),
        widget_id: "memory".into(),
        style: manifest.widgets[1].default_style.clone(),
    };
    for (id, title, value, progress) in [
        ("available", "剩余内存", "12.2", 0.38),
        ("used", "已用内存", "19.8", 0.62),
    ] {
        let data = WidgetData {
            source_id: None,
            secondary_value: None,
            presentations: Default::default(),
            widget_id: "memory".into(),
            title: title.into(),
            value: value.into(),
            detail: Some("/32.0 GiB".into()),
            progress: Some(progress),
            valid: true,
        };
        std::fs::write(
            root.join(format!("memory-{id}.png")),
            render::widget(&binding, &data)?,
        )?;
    }
    for (id, title, value, secondary, detail, progress) in [
        (
            "disk",
            "C:",
            "238.5",
            None,
            Some("GiB /1.8 TiB"),
            Some(0.13),
        ),
        (
            "network",
            "Ethernet",
            "12.4 MiB/s",
            Some("850 KiB/s"),
            None,
            None,
        ),
        ("battery", "电池", "76%", None, Some("充电中"), Some(0.76)),
        ("temperature", "CPU Package", "62.3°C", None, None, None),
        ("fan", "CPU Fan", "1250", None, Some("RPM"), None),
        ("power", "CPU Package", "65.2", None, Some("W"), None),
        ("voltage", "CPU Core", "1.25", None, Some("V"), None),
    ] {
        let definition = manifest.widgets.iter().find(|d| d.id == id).unwrap();
        let binding = WidgetBinding {
            plugin_id: manifest.id.clone(),
            widget_id: id.into(),
            style: definition.default_style.clone(),
            presentation_id: None,
            source_id: definition.select_source.then(|| "sample-source".into()),
            source_name: Some(title.into()),
        };
        let data = WidgetData {
            widget_id: id.into(),
            source_id: binding.source_id.clone(),
            title: title.into(),
            value: value.into(),
            secondary_value: secondary.map(str::to_owned),
            detail: detail.map(str::to_owned),
            progress,
            valid: true,
            presentations: Default::default(),
        };
        std::fs::write(
            root.join(format!("{id}.png")),
            render::widget(&binding, &data)?,
        )?;
    }
    Ok(())
}

async fn performance_widgets(
    c: std::sync::Arc<Core>,
    manager: std::sync::Arc<desktop_alchemy_core::plugins::PluginManager>,
    seconds: u64,
    sensors: bool,
) -> anyhow::Result<()> {
    use std::io::Write;
    let manifest = c
        .snapshot()
        .plugins
        .iter()
        .find(|p| p.manifest.id == "dev.masterain.performance")
        .unwrap()
        .manifest
        .clone();
    let mut pages = c.snapshot().config.pages;
    for (key, definition) in pages[0]
        .keys
        .iter_mut()
        .zip(manifest.widgets.iter().take(2))
    {
        key.widget = Some(WidgetBinding {
            source_id: None,
            source_name: None,
            presentation_id: None,
            plugin_id: manifest.id.clone(),
            widget_id: definition.id.clone(),
            style: definition.default_style.clone(),
        });
    }
    if sensors {
        manager.discover_widgets(&manifest.id, "diagnostic").await?;
        tokio::time::timeout(Duration::from_secs(15), async {
            while manager.core.widget_catalog(&manifest.id).sources.is_empty() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await?;
        let catalog = manager.core.widget_catalog(&manifest.id);
        let mut sources: Vec<_> = catalog
            .sources
            .iter()
            .filter(|s| s.widget_id == "disk")
            .collect();
        sources.sort_by_key(|s| &s.name);
        sources.extend(
            catalog
                .sources
                .iter()
                .filter(|s| s.widget_id == "network" && s.available)
                .take(1),
        );
        for kind in ["temperature", "fan", "power", "voltage"] {
            sources.extend(
                catalog
                    .sources
                    .iter()
                    .filter(|s| s.widget_id == kind && s.available)
                    .take(2),
            );
        }
        let mut bindings = vec![];
        for source in sources {
            let definition = manifest
                .widgets
                .iter()
                .find(|d| d.id == source.widget_id)
                .unwrap();
            bindings.push(WidgetBinding {
                plugin_id: manifest.id.clone(),
                widget_id: definition.id.clone(),
                style: definition.default_style.clone(),
                presentation_id: None,
                source_id: Some(source.id.clone()),
                source_name: Some(source.name.clone()),
            });
        }
        let battery = manifest.widgets.iter().find(|d| d.id == "battery").unwrap();
        bindings.insert(
            0,
            WidgetBinding {
                plugin_id: manifest.id.clone(),
                widget_id: battery.id.clone(),
                style: battery.default_style.clone(),
                presentation_id: None,
                source_id: None,
                source_name: None,
            },
        );
        for (key, binding) in pages[0].keys.iter_mut().skip(2).take(13).zip(bindings) {
            key.widget = Some(binding);
        }
        std::fs::write(
            c.root.join("sensor-catalog.json"),
            serde_json::to_vec_pretty(&catalog)?,
        )?;
    }
    let home = pages[0].id.clone();
    let other = desktop_alchemy_core::devices::default_page("无性能指标");
    let other_id = other.id.clone();
    pages.push(other);
    c.update_pages(pages)?;
    if sensors {
        manager.release_discovery(&manifest.id, "diagnostic").await;
    }
    let supervisor = tokio::spawn(manager.clone().supervise());
    tokio::time::timeout(Duration::from_secs(20), async {
        while c.snapshot().selected_device.is_none() || c.widget_samples().len() < 2 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;
    c.activate(&home).await?;
    // Exercise actual page boundaries and the persisted enable/disable path once.
    tokio::time::sleep(Duration::from_secs(3)).await;
    c.activate(&other_id).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    c.activate(&home).await?;
    manager.set_enabled(&manifest.id, false).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while c.snapshot().plugins[0].status != "stopped" {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await?;
    manager.set_enabled(&manifest.id, true).await?;
    let end = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut health = tokio::time::interval(Duration::from_secs(10));
    let mut file = std::fs::File::create(c.root.join("performance-health.jsonl"))?;
    while tokio::time::Instant::now() < end {
        health.tick().await;
        if c.snapshot().connection != "ready" {
            anyhow::bail!("性能专项期间设备连接异常");
        }
        let samples = c.widget_samples();
        let (queue, pending, submitted, failures) = {
            let i = c.inner.lock().unwrap();
            (
                i.queue.len(),
                i.widgets.pending.len(),
                i.widgets.submitted,
                i.widgets.failures,
            )
        };
        let files = |name| {
            std::fs::read_dir(c.root.join(name))
                .map(|r| r.count())
                .unwrap_or(0)
        };
        let row = serde_json::json!({"at":now(),"queue":queue,"pending":pending,"submitted":submitted,"failures":failures,"assets":files("assets"),"cache":files("cache"),"samples":samples});
        writeln!(file, "{row}")?;
        file.flush()?;
        if failures > 0 || pending > 15 {
            anyhow::bail!("动态帧提交失败或队列超限");
        }
    }
    manager.set_enabled(&manifest.id, false).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while c.snapshot().plugins[0].status != "stopped" {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await?;
    let submitted = c.inner.lock().unwrap().widgets.submitted;
    if submitted == 0 {
        anyhow::bail!("未记录到动态帧 SDK 接受");
    }
    c.stop.send_replace(true);
    supervisor.await?;
    println!(
        "{}",
        serde_json::json!({"ok":true,"submitted":submitted,"seconds":seconds,"evidence":c.root.join("performance-health.jsonl"),"note":"SDK 接受与连续运行检查，不包含屏幕目视验收、物理拔插或持久开机图写入"})
    );
    Ok(())
}

async fn led_effect_tour(c: std::sync::Arc<Core>) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(20), async {
        while c.snapshot().selected_device.is_none() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let original = c.snapshot().config.device_settings;
    let result = async {
        for (mode, label) in [
            (LedMode::Solid, "常亮"),
            (LedMode::Breathing, "呼吸"),
            (LedMode::ColorCycle, "颜色循环"),
            (LedMode::Rainbow, "彩虹"),
            (LedMode::Wave, "波浪"),
            (LedMode::Comet, "彗星"),
        ] {
            println!("LED test: {label}");
            let mut settings = original.clone();
            settings.led_mode = mode;
            settings.led_brightness = 255;
            settings.led_speed = 10;
            c.preview_led_settings(settings)?;
            tokio::time::sleep(Duration::from_secs(4)).await;
            if c.snapshot().connection != "ready" {
                anyhow::bail!("M18 在 LED 灯效测试期间断开")
            }
        }

        println!("LED test: 彩虹慢速");
        let mut slow = original.clone();
        slow.led_mode = LedMode::Rainbow;
        slow.led_brightness = 255;
        slow.led_speed = 1;
        c.preview_led_settings(slow)?;
        tokio::time::sleep(Duration::from_secs(4)).await;

        println!("LED test: 呼吸亮度 128");
        let mut dim = original.clone();
        dim.led_mode = LedMode::Breathing;
        dim.led_speed = 10;
        dim.led_brightness = 128;
        c.preview_led_settings(dim.clone())?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        println!("LED test: 呼吸亮度 0");
        dim.led_brightness = 0;
        c.preview_led_settings(dim)?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(restore) = c.preview_led_settings(original) {
        anyhow::bail!("LED 测试结束后恢复预览设置失败：{restore:#}")
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    result
}
async fn exercise(c: std::sync::Arc<Core>) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(20), async {
        while c.snapshot().selected_device.is_none() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let p = c.root.join("diagnostic.png");
    image::RgbImage::from_fn(64, 64, |x, y| {
        if (x / 8 + y / 8) % 2 == 0 {
            image::Rgb([89, 102, 243])
        } else {
            image::Rgb([255, 255, 255])
        }
    })
    .save(&p)?;
    let asset = storage::import(&c.root, &p)?;
    let data = storage::data_url(&p)?;
    let gif = c.root.join("diagnostic.gif");
    {
        let mut enc = image::codecs::gif::GifEncoder::new(std::fs::File::create(&gif)?);
        enc.set_repeat(image::codecs::gif::Repeat::Infinite)?;
        for color in [[89, 102, 243, 255], [30, 190, 140, 255]] {
            let frame = image::RgbaImage::from_pixel(64, 64, image::Rgba(color));
            enc.encode_frame(image::Frame::from_parts(
                frame,
                0,
                0,
                image::Delay::from_numer_denom_ms(400, 1),
            ))?;
        }
    }
    let gif_asset = storage::import(&c.root, &gif)?;
    let bg = storage::import(&c.root, &render::blank_background(&c.root)?)?;
    let commands = vec![
        HardwareCommand::ClearAll,
        HardwareCommand::ClearKey { key: 1 },
        HardwareCommand::KeyImage {
            key: 1,
            asset_id: asset.clone(),
        },
        HardwareCommand::KeyData {
            key: 2,
            data: data.clone(),
        },
        HardwareCommand::BulkImage {
            keys: vec![3, 4, 5],
            asset_id: asset.clone(),
        },
        HardwareCommand::Background {
            asset_id: bg.clone(),
            persistent: false,
        },
        HardwareCommand::BackgroundData {
            data: storage::data_url(&storage::asset(&c.root, &bg)?)?,
            persistent: false,
        },
        HardwareCommand::ScreenBrightness { value: 55 },
        HardwareCommand::LedBrightness { value: 128 },
        HardwareCommand::LedColor {
            color: "#5966F3".into(),
        },
        HardwareCommand::LedColors {
            colors: (0..24)
                .map(|n| {
                    if n % 2 == 0 {
                        [89, 102, 243]
                    } else {
                        [30, 190, 140]
                    }
                })
                .collect(),
        },
        HardwareCommand::ResetLed,
        HardwareCommand::KeyGif {
            key: 1,
            asset_id: gif_asset,
        },
        HardwareCommand::StartGif,
        HardwareCommand::GifStatus,
        HardwareCommand::StopGif,
        HardwareCommand::ClearGif { key: 1 },
        HardwareCommand::Refresh,
    ];
    use std::io::Write;
    for command in commands {
        let name = serde_json::to_value(&command)?["type"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let result = c.enqueue(command, None);
        let row = serde_json::json!({"at":now(),"command":name,"receipt":result.as_ref().ok(),"error":result.as_ref().err().map(ToString::to_string),"verification":"not_observed"});
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(c.root.join("hardware-exercise.jsonl"))?;
        writeln!(f, "{row}")?;
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
    let s = c.snapshot();
    c.activate(&s.config.active_page).await?;
    println!(
        "Hardware exercise submitted; numbered page restored. Physical effects remain unverified."
    );
    Ok(())
}

/// Exercise the same manager and scheduler as the client, in a fresh isolated directory.
async fn managed_devices(
    root: PathBuf,
    exercise_hardware: bool,
    seconds: u64,
) -> anyhow::Result<()> {
    if root.join("config.json").exists() {
        anyhow::bail!("多设备诊断须使用新的独立目录")
    }
    storage::initialize_workspace(&root)?;
    let host = Core::new(root)?;
    let manager = desktop_alchemy_core::devices::DeviceManager::new(host.clone())?;
    manager.start(None, true);
    let result=async {
        let core=tokio::time::timeout(Duration::from_secs(20),async {
            loop {
                let snapshot=manager.snapshot()?;
                if snapshot.discovered.iter().filter(|d|d.adapter_id==desktop_alchemy_core::devices::m18::ID).count()>1 { anyhow::bail!("诊断发现多台设备；请在客户端明确绑定，不能按枚举顺序测试") }
                if let Some(device)=snapshot.devices.iter().find(|d|d.adapter_id==desktop_alchemy_core::devices::m18::ID&&d.state.connection=="ready") {
                    return manager.device(device.device_id.as_deref().unwrap());
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }).await??;
        if exercise_hardware { exercise(core.clone()).await?; }
        else { tokio::time::sleep(Duration::from_secs(seconds)).await; }
        let report=host.export_report()?;
        let snapshot=core.snapshot();
        let failed=snapshot.receipts.iter().filter(|r|r.status=="failed").count();
        println!("{}",serde_json::to_string(&serde_json::json!({"ok":failed==0,"devices":manager.snapshot()?.devices.len(),"acknowledged":snapshot.receipts.iter().filter(|r|r.status=="acknowledged").count(),"failed":failed,"report":report,"verification":"sdk_only_not_observed"}))?);
        if failed>0 { anyhow::bail!("设备诊断存在失败回执") }
        Ok(())
    }.await;
    manager.shutdown().await;
    let _ = host.stop.send(true);
    tokio::time::timeout(Duration::from_secs(5), async {
        while host.snapshot().sdk_pid.is_some() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await?;
    result
}
