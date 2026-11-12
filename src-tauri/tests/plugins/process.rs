use desktop_alchemy_core::{
    core::Core,
    model::{now, DeviceEvent},
    plugins::PluginManager,
};
use std::time::Duration;
#[tokio::test]
async fn example_plugin_handshakes_switches_page_and_stops() {
    let dir = std::env::temp_dir().join(format!("m18-plugin-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_m18-example-plugin"),
        dir.join("m18-example-plugin.exe"),
    )
    .unwrap();
    std::fs::write(
        dir.join("plugin.json"),
        include_str!("../../../examples/counter/plugin.json"),
    )
    .unwrap();
    let core = crate::support::device_core(dir.join("data"));
    let mut config = core.snapshot().config;
    config
        .pages
        .push(desktop_alchemy_core::devices::default_page("第二页"));
    let target = config.pages[1].id.clone();
    core.update_config(config).unwrap();
    {
        let mut inner = core.inner.lock().unwrap();
        inner.snapshot.selected_device = Some("simulated".into());
        inner.paths.insert("simulated".into(), "test-path".into());
        inner
            .snapshot
            .devices
            .push(desktop_alchemy_core::model::DeviceDescriptor {
                adapter_id: desktop_alchemy_core::devices::default_adapter_id().into(),
                layout: desktop_alchemy_core::devices::default_layout().into(),
                id: "simulated".into(),
                model: "M18".into(),
                serial: "test".into(),
                firmware: "V3.M18".into(),
                vendor_id: 0x6603,
                product_id: 0x1012,
                capabilities: desktop_alchemy_core::devices::default_capabilities("V3.M18"),
            });
    }
    let plugins = PluginManager::new(core.clone());
    let package = dir.with_extension("m18plugin");
    desktop_alchemy_core::plugin_package::pack(&dir, &package).unwrap();
    let summary = plugins.inspect_package(&package).unwrap();
    let id = plugins
        .install_package(package, summary.fingerprint)
        .await
        .unwrap();
    assert!(plugins.start(&id).await.is_err());
    plugins.set_enabled(&id, true).await.unwrap();

    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if core.snapshot().plugins[0].status == "running" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    core.event(DeviceEvent {
        input: None,
        at: now(),
        kind: "button.down".into(),
        device_id: "simulated".into(),
        key: Some(2),
        pressed: Some(true),
        page_id: None,
    });
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if core.snapshot().config.active_page == target {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    plugins.stop(&id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if core.snapshot().plugins[0].status == "stopped" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(core.snapshot().config.active_page, target);
}

#[tokio::test]
async fn managed_install_replace_rollback_uninstall_and_restart() {
    use desktop_alchemy_core::{model::WidgetBinding, plugin_package, storage};
    let root = std::env::temp_dir().join(format!("m18-managed-{}", uuid::Uuid::new_v4()));
    let source = root.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_m18-example-plugin"),
        source.join("m18-example-plugin.exe"),
    )
    .unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/counter/plugin.json")).unwrap();
    let package = root.join("counter.m18plugin");
    std::fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    plugin_package::pack(&source, &package).unwrap();
    let core = crate::support::device_core(root.join("data"));
    let manager = PluginManager::new(core.clone());
    let summary = manager.inspect_package(&package).unwrap();
    let id = manager
        .install_package(package.clone(), summary.fingerprint.clone())
        .await
        .unwrap();
    assert!(core.snapshot().config.disabled_plugins.contains(&id));
    assert!(manager.running_pid(&id).await.is_none());
    // Preserve a missing widget binding as well as ordinary button content.
    let mut pages = core.snapshot().config.pages;
    pages[0].keys[0].widget = Some(WidgetBinding {
        source_id: None,
        source_name: None,
        plugin_id: id.clone(),
        widget_id: "future".into(),
        presentation_id: None,
        style: desktop_alchemy_core::model::WidgetStyle {
            foreground: "#FFFFFF".into(),
            background: "#000000".into(),
            accent: "#FFFFFF".into(),
            show_progress: true,
        },
    });
    core.update_pages(pages).unwrap();
    let before = serde_json::to_value(core.snapshot().config.pages).unwrap();
    manager.set_enabled(&id, true).await.unwrap();
    async fn running(manager: &PluginManager, core: &Core) {
        tokio::time::timeout(Duration::from_secs(6), async {
            while core.snapshot().plugins[0].status != "running"
                || manager.running_pid("dev.masterain.counter").await.is_none()
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
    running(&manager, &core).await;
    let old_pid = manager.running_pid(&id).await.unwrap();
    manifest["version"] = serde_json::json!("0.0.1"); // Downgrades also require the same reviewed replace path.
    std::fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    plugin_package::pack(&source, &package).unwrap();
    assert!(manager
        .install_package(package.clone(), summary.fingerprint)
        .await
        .is_err());
    assert_eq!(manager.running_pid(&id).await, Some(old_pid));
    let summary = manager.inspect_package(&package).unwrap();
    assert_eq!(summary.installed_version.as_deref(), Some("0.1.0"));
    std::fs::create_dir(core.root.join("config.json.tmp")).unwrap();
    assert!(manager
        .install_package(package.clone(), summary.fingerprint.clone())
        .await
        .is_err());
    assert_eq!(core.snapshot().plugins[0].manifest.version, "0.1.0");
    std::fs::remove_dir(core.root.join("config.json.tmp")).unwrap();
    running(&manager, &core).await;
    manager
        .install_package(package.clone(), summary.fingerprint.clone())
        .await
        .unwrap();
    running(&manager, &core).await;
    assert_ne!(manager.running_pid(&id).await, Some(old_pid));
    assert_eq!(core.snapshot().plugins[0].manifest.version, "0.0.1");
    assert!(!core.snapshot().config.disabled_plugins.contains(&id));
    assert_eq!(
        before,
        serde_json::to_value(core.snapshot().config.pages).unwrap()
    );
    manager.set_enabled(&id, false).await.unwrap();
    manager
        .install_package(package.clone(), summary.fingerprint)
        .await
        .unwrap();
    assert!(core.snapshot().config.disabled_plugins.contains(&id));
    manager.stop_all().await;
    let restarted = crate::support::device_core(core.root.clone());
    let reloaded = PluginManager::new(restarted.clone());
    assert_eq!(restarted.snapshot().plugins.len(), 1);
    assert!(reloaded.start(&id).await.is_err());
    reloaded.remove(&id).await.unwrap();
    assert!(!core.root.join("plugins").join(&id).exists());
    assert!(storage::load(&core.root)
        .unwrap()
        .managed_plugin_ids
        .is_empty());
    assert_eq!(
        before,
        serde_json::to_value(restarted.snapshot().config.pages).unwrap()
    );
    let summary = reloaded.inspect_package(&package).unwrap();
    reloaded
        .install_package(package, summary.fingerprint)
        .await
        .unwrap();
    std::fs::rename(core.root.join("plugins").join(&id), root.join("lost-files")).unwrap();
    let missing_core = crate::support::device_core(core.root.clone());
    let missing_manager = PluginManager::new(missing_core.clone());
    assert!(missing_core.snapshot().plugins.is_empty());
    missing_manager.remove(&id).await.unwrap();
    assert!(storage::load(&core.root)
        .unwrap()
        .managed_plugin_ids
        .is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
