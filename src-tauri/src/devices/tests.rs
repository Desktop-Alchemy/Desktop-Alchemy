use super::*;
use crate::{core::ActivationSource, storage};

struct Fixture {
    manager: Arc<DeviceManager>,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("alchemy-devices-{}", uuid::Uuid::new_v4()));
        storage::initialize_workspace(&root).unwrap();
        let host = Core::new(root.clone()).unwrap();
        let manager = DeviceManager::new(host).unwrap();
        Self { manager, root }
    }
    async fn pair(&self) -> (Arc<Core>, Arc<Core>) {
        let a = m18::sdk::device_descriptor(0x6603, 0x1012, "one".into(), "V3.M18".into(), "a");
        let b = m18::sdk::device_descriptor(0x6603, 0x1012, "two".into(), "V3.M18".into(), "b");
        self.manager.discovered(vec![a.clone(), b.clone()]).unwrap();
        for (device, path) in [(a.clone(), "a"), (b.clone(), "b")] {
            crate::devices::testing::handle(
                &self.manager.host,
                crate::devices::testing::Event::Connected {
                    device,
                    path: path.into(),
                },
            )
            .await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let a = self.manager.device(&a.id).unwrap();
        let b = self.manager.device(&b.id).unwrap();
        (a, b)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn both_devices_receive_input_without_a_global_selection() {
    let f = Fixture::new();
    let (a, b) = f.pair().await;
    assert!(f.manager.host.snapshot().selected_device.is_none());
    let mut pages_a = a.snapshot().config.pages;
    let next_a = crate::devices::default_page("A next");
    let target_a = next_a.id.clone();
    pages_a[0].bindings[0].action = KeyAction::Page {
        page_id: target_a.clone(),
    };
    pages_a.push(next_a);
    a.update_pages(pages_a).unwrap();
    let mut pages_b = b.snapshot().config.pages;
    let next_b = crate::devices::default_page("B next");
    let target_b = next_b.id.clone();
    pages_b[0].bindings[0].action = KeyAction::Page {
        page_id: target_b.clone(),
    };
    pages_b.push(next_b);
    b.update_pages(pages_b).unwrap();
    for path in ["a", "b"] {
        crate::devices::testing::handle(
            &f.manager.host,
            crate::devices::testing::Event::Key {
                path: path.into(),
                key: 1,
                pressed: true,
            },
        )
        .await;
    }
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while a.snapshot().active_page != target_a || b.snapshot().active_page != target_b {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(a.snapshot().pressed, vec![1]);
    assert_eq!(b.snapshot().pressed, vec![1]);
    let selected = a.snapshot().selected_device;
    assert!(a
        .inner
        .lock()
        .unwrap()
        .queue
        .iter()
        .all(|w| Some(w.device_id.as_str()) == selected.as_deref()));
}

#[tokio::test]
async fn disconnect_and_old_prepared_work_are_isolated_even_if_path_is_reused() {
    let f = Fixture::new();
    let (a, b) = f.pair().await;
    a.inner.lock().unwrap().queue.clear();
    b.inner.lock().unwrap().queue.clear();
    a.enqueue(HardwareCommand::Firmware, None).unwrap();
    let old = a.pop_batch().remove(0);
    let b_receipt = b.enqueue(HardwareCommand::Firmware, None).unwrap();
    crate::devices::testing::handle(
        &f.manager.host,
        crate::devices::testing::Event::Disconnected { path: "a".into() },
    )
    .await;
    assert!(f.manager.device(&old.device_id).is_err());
    assert_eq!(
        b.snapshot()
            .receipts
            .iter()
            .find(|r| r.id == b_receipt)
            .unwrap()
            .status,
        "queued"
    );
    let descriptor =
        m18::sdk::device_descriptor(0x6603, 0x1012, "one".into(), "V3.M18".into(), "a");
    crate::devices::testing::handle(
        &f.manager.host,
        crate::devices::testing::Event::Connected {
            device: descriptor,
            path: "a".into(),
        },
    )
    .await;
    assert!(!a.work_is_current(&old));
    assert!(f
        .manager
        .snapshot()
        .unwrap()
        .devices
        .iter()
        .any(|d| d.device_id.as_deref() == Some(&old.device_id)));
}

#[tokio::test]
async fn profile_writes_and_scene_pauses_do_not_overwrite_another_device() {
    let f = Fixture::new();
    let (a, b) = f.pair().await;
    let scenes = SceneSettings {
        enabled: true,
        ..Default::default()
    };
    a.save_scenes(scenes.clone()).unwrap();
    b.save_scenes(scenes).unwrap();
    a.activate_from(&a.snapshot().active_page, ActivationSource::Manual)
        .await
        .unwrap();
    assert!(a.snapshot().config.scene_settings.paused);
    assert!(!b.snapshot().config.scene_settings.paused);
    let left = a.clone();
    let right = b.clone();
    let (left, right) = tokio::join!(
        tokio::task::spawn_blocking(move || {
            let mut pages = left.snapshot().config.pages;
            pages[0].name = "A saved".into();
            left.update_pages(pages)
        }),
        tokio::task::spawn_blocking(move || {
            let mut pages = right.snapshot().config.pages;
            pages[0].name = "B saved".into();
            right.update_pages(pages)
        })
    );
    left.unwrap().unwrap();
    right.unwrap().unwrap();
    let saved = storage::load_workspace(&f.root).unwrap();
    assert!(saved
        .profiles
        .iter()
        .any(|p| p.config.pages[0].name == "A saved"));
    assert!(saved
        .profiles
        .iter()
        .any(|p| p.config.pages[0].name == "B saved"));
}

struct LightAdapter;
impl DeviceAdapter for LightAdapter {
    fn id(&self) -> &'static str {
        "test.light"
    }
    fn layout(&self) -> DeviceLayout {
        DeviceLayout {
            led_count: 2,
            ..Default::default()
        }
    }
    fn default_config(&self) -> AppConfig {
        AppConfig {
            pages: vec![],
            active_page: String::new(),
            device_settings: DeviceSettings {
                led_colors: vec![[0; 3]; 2],
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn run(self: Arc<Self>, _: alchemy_device_api::AdapterHost) -> AdapterFuture {
        Box::pin(async { Ok(()) })
    }
}
#[tokio::test]
async fn a_device_without_keys_or_screens_needs_no_page() {
    let f = Fixture::new();
    let root = f.root.join("light");
    storage::initialize_workspace(&root).unwrap();
    storage::edit_workspace(&root, |workspace| {
        workspace.profiles.clear();
        Ok(())
    })
    .unwrap();
    let host = Core::new(root).unwrap();
    let mut adapters: BTreeMap<String, Arc<dyn DeviceAdapter>> = BTreeMap::new();
    adapters.insert("test.light".into(), Arc::new(LightAdapter));
    let manager = DeviceManager::with_adapters(host, adapters).unwrap();
    manager
        .discovered(vec![DeviceDescriptor {
            adapter_id: "test.light".into(),
            layout: LightAdapter.layout().into(),
            id: "light".into(),
            model: "Test light".into(),
            serial: "light".into(),
            firmware: "1".into(),
            vendor_id: 0,
            product_id: 0,
            capabilities: Default::default(),
        }])
        .unwrap();
    let snapshot = manager.snapshot().unwrap();
    assert_eq!(snapshot.devices.len(), 1);
    assert!(snapshot.devices[0].state.config.pages.is_empty());
    let core = manager.profile(&snapshot.devices[0].profile_id).unwrap();
    assert_eq!(core.layout.led_count, 2);
    assert!(core
        .validate_command(&HardwareCommand::KeyData {
            key: 1,
            data: String::new()
        })
        .is_err());
    assert!(core
        .configure_led_settings(LightAdapter.default_config().device_settings)
        .is_ok());
}

#[test]
fn unknown_module_fields_survive_workspace_roundtrip() {
    let root = std::env::temp_dir().join(format!("alchemy-roundtrip-{}", uuid::Uuid::new_v4()));
    storage::initialize_workspace(&root).unwrap();
    let mut value = serde_json::to_value(storage::load_workspace(&root).unwrap()).unwrap();
    let mut profile = serde_json::to_value(DeviceProfile::new(
        "future.device".into(),
        "future".into(),
        None,
        AppConfig::empty(),
    ))
    .unwrap();
    profile["schema_version"] = 99.into();
    profile["future"] = serde_json::json!({"nested":[1,2,3]});
    profile["config"]["future_field"] = serde_json::json!({"value":7});
    value["profiles"] = serde_json::json!([profile]);
    let workspace = storage::decode_workspace(&serde_json::to_vec(&value).unwrap()).unwrap();
    storage::save_workspace(&root, &workspace).unwrap();
    let roundtrip = serde_json::to_value(storage::load_workspace(&root).unwrap()).unwrap();
    assert_eq!(roundtrip, value);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn widget_frames_and_source_cancellation_are_device_scoped() {
    let f = Fixture::new();
    let (a, b) = f.pair().await;
    let manifest: PluginManifest =
        serde_json::from_slice(include_bytes!("../../../plugins/performance/plugin.json")).unwrap();
    let widget = manifest.widgets.iter().find(|w| w.id == "cpu").unwrap();
    let binding = WidgetBinding {
        plugin_id: manifest.id.clone(),
        widget_id: widget.id.clone(),
        style: widget.default_style.clone(),
        presentation_id: None,
        source_id: None,
        source_name: None,
    };
    f.manager
        .host
        .inner
        .lock()
        .unwrap()
        .snapshot
        .plugins
        .push(PluginInfo { ui_message: None,
            manifest: manifest.clone(),
            path: String::new(),
            status: "running".into(),
            error: None,
            built_in: true,
        });
    f.manager.sync_plugins();
    for core in [&a, &b] {
        let mut inner = core.inner.lock().unwrap();
        inner.queue.clear();
        inner.widgets.page_ready = true;
        inner.snapshot.config.pages[0].keys[0].widget = Some(binding.clone());
    }
    f.manager
        .host
        .publish_widget(
            &manifest.id,
            WidgetData {
                widget_id: "cpu".into(),
                source_id: None,
                title: "CPU".into(),
                value: "42%".into(),
                secondary_value: None,
                detail: None,
                progress: Some(0.42),
                valid: true,
                presentations: Default::default(),
            },
        )
        .unwrap();
    f.manager.host.refresh_widgets().unwrap();
    let a_work = a.pop_batch().remove(0);
    let b_work = b.pop_batch().remove(0);
    assert_ne!(a_work.device_id, b_work.device_id);
    assert!(a_work.widget_frame.is_some() && b_work.widget_frame.is_some());
    a.cancel_owner(&manifest.id);
    assert!(!a.work_is_current(&a_work));
    assert!(b.work_is_current(&b_work));
    f.manager.host.cancel_owner(&manifest.id);
    assert!(!b.work_is_current(&b_work));
}

#[tokio::test]
async fn scheduler_is_fair_keeps_pages_atomic_and_routes_only_its_adapter() {
    let f = Fixture::new();
    let (a, b) = f.pair().await;
    a.inner.lock().unwrap().queue.clear();
    b.inner.lock().unwrap().queue.clear();
    a.activate_from(&a.snapshot().active_page, ActivationSource::Refresh)
        .await
        .unwrap();
    b.enqueue(HardwareCommand::Firmware, None).unwrap();
    let mut cursor = 0;
    assert!(f
        .manager
        .next_batch("different.backend", &mut cursor)
        .is_none());
    let (_, first) = f.manager.next_batch(m18::ID, &mut cursor).unwrap();
    let (_, second) = f.manager.next_batch(m18::ID, &mut cursor).unwrap();
    assert_ne!(first[0].device_id, second[0].device_id);
    let page = if first[0].page_batch.is_some() {
        first
    } else {
        second
    };
    assert!(page.len() > 15);
    assert!(page
        .iter()
        .all(|w| w.device_id == page[0].device_id && w.page_batch == page[0].page_batch));
}

#[test]
fn workspace_rejects_plugin_path_traversal_and_duplicate_global_ids() {
    let mut workspace = WorkspaceConfig {
        format: crate::storage::WORKSPACE_FORMAT.into(),
        extensions: Default::default(),
        version: 1,
        global: GlobalConfig::default(),
        profiles: vec![],
    };
    workspace.global.managed_plugin_ids.push("../escape".into());
    assert!(storage::validate_workspace(&workspace).is_err());
    workspace.global.managed_plugin_ids = vec!["example.plugin".into(), "EXAMPLE.PLUGIN".into()];
    assert!(storage::validate_workspace(&workspace).is_err());
}
