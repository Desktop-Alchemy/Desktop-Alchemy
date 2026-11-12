//! Read-only input diagnostics: fresh disabled bindings, no M18 SDK ownership.
#[cfg(windows)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use desktop_alchemy_core::{
        core::Core,
        devices::{ms_audio_dock::Adapter, DeviceAdapter, DeviceManager},
        raw_input::RawInputService,
    };
    fn cpu() -> u64 {
        unsafe {
            use windows_sys::Win32::{
                Foundation::FILETIME,
                System::Threading::{GetCurrentProcess, GetProcessTimes},
            };
            let mut create = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            GetProcessTimes(
                GetCurrentProcess(),
                &mut create,
                &mut exit,
                &mut kernel,
                &mut user,
            );
            ((kernel.dwHighDateTime as u64) << 32 | kernel.dwLowDateTime as u64)
                + ((user.dwHighDateTime as u64) << 32 | user.dwLowDateTime as u64)
        }
    }
    let listen = std::env::args().any(|arg| arg == "--raw-input");
    let root =
        std::env::temp_dir().join(format!("alchemy-input-diagnose-{}", uuid::Uuid::new_v4()));
    let manager = DeviceManager::new(Core::new(root.clone())?)?;
    manager.start(None, false);
    let service = if listen {
        Some(RawInputService::start(
            manager.clone(),
            Adapter::default().raw_input(),
        )?)
    } else {
        None
    };
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let exercise = std::env::args().any(|arg| arg == "--launch-test");
    let output = root.join("physical-launch.json");
    if exercise {
        use alchemy_device_api::{ControlBinding, InputTrigger};
        use desktop_alchemy_core::model::{KeyAction, LaunchTarget};
        let devices = manager.runtimes_for("microsoft.audio-dock");
        anyhow::ensure!(
            listen && devices.len() == 1,
            "Launch test needs exactly one discovered Audio Dock"
        );
        let executable = std::env::current_exe()?.with_file_name("m18-test-launcher.exe");
        anyhow::ensure!(executable.is_file(), "Build m18-test-launcher first");
        devices[0].core.save_control_bindings(vec![ControlBinding {
            control_id: "1".into(),
            trigger: InputTrigger::Press,
            enabled: true,
            action: KeyAction::Launch {
                target: LaunchTarget::Program {
                    path: executable.to_string_lossy().into(),
                    arguments: format!("\"{}\" \"Teams 按键测试\"", output.display()),
                    working_directory: None,
                },
            },
        }])?;
        println!("Ready: temporary physical launch test enabled");
    }
    let before = cpu();
    let started = std::time::Instant::now();
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(if exercise { 180 } else { 60 });
    let mut events = manager.host.events.subscribe();
    loop {
        tokio::select! {_=tokio::time::sleep_until(deadline)=>break,event=events.recv()=>{if let Ok(event)=event{if event.kind.starts_with("button."){println!("input {} key={:?} pressed={:?}",event.kind,event.key,event.pressed);}}}}
    }
    let elapsed = started.elapsed().as_secs_f64();
    let cpu_seconds = (cpu() - before) as f64 / 10_000_000.;
    println!("raw_input={listen} elapsed={elapsed:.3}s cpu_seconds={cpu_seconds:.6} one_core_percent={:.4}",cpu_seconds/elapsed*100.);
    if let Some(service) = service {
        println!("stats={}", serde_json::to_string(&service.stats())?);
        for device in manager
            .snapshot()?
            .devices
            .into_iter()
            .filter(|device| device.adapter_id == "microsoft.audio-dock")
        {
            println!(
                "Audio Dock status={} bindings_enabled={}",
                device.state.connection,
                device
                    .state
                    .config
                    .bindings
                    .iter()
                    .any(|binding| binding.enabled)
            );
        }
        service.stop();
    }
    if exercise {
        println!("physical_launch={}", output.is_file());
        if output.is_file() {
            let report: serde_json::Value = serde_json::from_slice(&std::fs::read(&output)?)?;
            println!("launch_arguments={}", report["arguments"]);
        }
    }
    manager.shutdown().await;
    drop(manager);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Raw Input diagnostics require Windows");
}
