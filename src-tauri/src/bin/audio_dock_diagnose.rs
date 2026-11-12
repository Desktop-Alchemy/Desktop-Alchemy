//! Explicit hardware acceptance, using the same DeviceManager/Core settings path as IPC.
#[cfg(windows)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use alchemy_audio_dock::types::*;
    use desktop_alchemy_core::{
        core::Core,
        devices::{ms_audio_dock::Adapter, DeviceAdapter, DeviceManager},
        raw_input::RawInputService,
    };
    let root = std::env::temp_dir().join(format!("alchemy-dock-settings-{}", uuid::Uuid::new_v4()));
    let manager = DeviceManager::new(Core::new(root.clone())?)?;
    manager.start(None, false);
    let service = RawInputService::start(manager.clone(), Adapter::default().raw_input())?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let result=async {
        let devices=manager.snapshot()?.discovered;
        anyhow::ensure!(devices.len()==1,"需要恰好一台 Audio Dock");
        let id=&devices[0].id;
        let core=manager.device_profile(id)?;
        println!("input connection: {} {:?}",core.snapshot().connection,core.snapshot().error);
        let initial=dock_settings(&manager,id,None).await?;
        let output=std::env::args().skip(1).collect::<Vec<_>>();
        if let Some(path)=output.iter().find_map(|s| s.strip_prefix("--output=")) {std::fs::write(path,serde_json::to_vec_pretty(&initial)?)?;}
        // Never print serial numbers to routine output.
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"connection":initial.connection,"settings":initial.settings,"hardware":initial.info.hardware_version,"firmware":initial.info.firmware_version,"color":initial.info.color_code,"serial_read":initial.info.serial.is_some(),"error":initial.error}))?);
        if output.iter().any(|s|s=="--exercise") {
            anyhow::ensure!(initial.connection==AudioDockConnection::Ready,"初始读取不完整，禁止写入");
            let s=&initial.settings;
            let steps=vec![
                (AudioDockSettingsUpdate::PromptVolume{value:if s.prompt_volume==Some(39){40}else{39}},AudioDockSettingsUpdate::PromptVolume{value:s.prompt_volume.unwrap()}),
                (AudioDockSettingsUpdate::PromptTone{enabled:!s.prompt_tone.unwrap()},AudioDockSettingsUpdate::PromptTone{enabled:s.prompt_tone.unwrap()}),
                (AudioDockSettingsUpdate::PushToTalk{enabled:!s.push_to_talk.unwrap()},AudioDockSettingsUpdate::PushToTalk{enabled:s.push_to_talk.unwrap()}),
                (AudioDockSettingsUpdate::NoiseSuppression{mode:if s.noise_suppression==Some(AudioDockNoiseSuppression::Standard){AudioDockNoiseSuppression::Low}else{AudioDockNoiseSuppression::Standard}},AudioDockSettingsUpdate::NoiseSuppression{mode:s.noise_suppression.clone().unwrap()}),
                (AudioDockSettingsUpdate::SelectEqualizer{id:if s.active_equalizer==Some(0){1}else{0}},AudioDockSettingsUpdate::SelectEqualizer{id:s.active_equalizer.unwrap()}),
            ];
            for (change,restore) in steps {
                println!("testing: {:?}",change);
                let changed=dock_settings(&manager,id,Some(change)).await;
                let restored=dock_settings(&manager,id,Some(restore)).await;
                restored?;changed?;
            }
            println!("exercise: all scalar settings and preset selection read back and restored");
        }
        if output.iter().any(|s|s=="--exercise-custom") {
            let active=initial.settings.active_equalizer.ok_or_else(||anyhow::anyhow!("当前预设未知"))?;
            // Existing user-selected custom slot only. Never create persistent test presets.
            let preset=initial.settings.equalizer_presets.iter().find(|p|p.id==active && !p.factory).ok_or_else(||anyhow::anyhow!("自定义测试需当前已选中自定义预设"))?;
            let mut gains=preset.gains;gains[0]=if gains[0]<12 {gains[0]+1}else{gains[0]-1};
            let changed=dock_settings(&manager,id,Some(AudioDockSettingsUpdate::SaveEqualizer{id:Some(active),name:preset.name.clone(),gains})).await;
            let restored=dock_settings(&manager,id,Some(AudioDockSettingsUpdate::SaveEqualizer{id:Some(active),name:preset.name.clone(),gains:preset.gains})).await;
            restored?;changed?;println!("custom: written, read back, activated and restored");
        }
        if output.iter().any(|s|s=="--exercise-presets") {
            anyhow::ensure!(initial.connection==AudioDockConnection::Ready,"初始读取不完整，禁止写入");
            let name=format!("DA-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
            let original=initial.settings.active_equalizer.ok_or_else(||anyhow::anyhow!("当前预设未知"))?;
            let trial=dock_settings(&manager,id,Some(AudioDockSettingsUpdate::SaveEqualizer{id:None,name:name.clone(),gains:[1,0,-1,0,2]})).await;
            // Read even after uncertain creation. Only remove our unique new preset
            // in a slot which was free initially; never remove an existing preset.
            let now=dock_settings(&manager,id,None).await?;
            let created=now.settings.equalizer_presets.iter().find(|p|p.name==name && initial.settings.free_equalizer_slots.contains(&p.id)).cloned();
            let removed=if let Some(p)=created {
                dock_settings(&manager,id,Some(AudioDockSettingsUpdate::DeleteEqualizer{id:p.id,name:p.name})).await.map(|_|())
            }else{Err(anyhow::anyhow!("未找到本次创建的预设"))};
            let restored=dock_settings(&manager,id,Some(AudioDockSettingsUpdate::SelectEqualizer{id:original})).await;
            restored?;removed?;trial?;
            println!("presets: created, gains read back, activated, deleted and original preset restored");
        }
        let final_state=dock_settings(&manager,id,None).await?;
        anyhow::ensure!(initial.settings==final_state.settings,"最终设置与初始快照不一致");
        Ok::<(),anyhow::Error>(())
    }.await;
    service.stop();
    manager.shutdown().await;
    std::fs::remove_dir_all(root)?;
    result
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Audio Dock settings require Windows");
}

#[cfg(windows)]
async fn dock_settings(
    manager: &std::sync::Arc<desktop_alchemy_core::devices::DeviceManager>,
    id: &str,
    update: Option<alchemy_audio_dock::types::AudioDockSettingsUpdate>,
) -> anyhow::Result<alchemy_audio_dock::types::AudioDockState> {
    let operation = if update.is_some() { "update" } else { "read" };
    Ok(serde_json::from_value(
        manager
            .module_request(id, operation.into(), serde_json::to_value(update)?)
            .await?,
    )?)
}
