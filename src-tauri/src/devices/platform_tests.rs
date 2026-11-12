use super::*;
use alchemy_device_api::{
    AdapterHost, Connection, ControlBinding, InputControl, InputKind, InputTrigger, InputValue,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("alchemy-platform-{}", uuid::Uuid::new_v4())))
    }
    fn manager(&self) -> Arc<DeviceManager> {
        DeviceManager::with_adapters(Core::new(self.0.clone()).unwrap(), adapters()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Adapter(&'static str);
impl DeviceAdapter for Adapter {
    fn id(&self) -> &'static str {
        self.0
    }
    fn layout(&self) -> DeviceLayout {
        DeviceLayout {
            input_keys: (1..=32).collect(),
            display_keys: (1..=32).collect(),
            columns: 8,
            key_width: 16,
            key_height: 16,
            ..Default::default()
        }
    }
    fn definition(&self) -> DeviceDefinition {
        let mut d = DeviceDefinition {
            adapter_id: self.0.into(),
            name: "Fixture".into(),
            layout: self.layout(),
            supports_pages: true,
            input_names: Default::default(),
            inputs: vec![],
            displays: vec![],
        }
        .normalized();
        d.inputs.extend([
            InputControl {
                id: "knob".into(),
                name: "Knob".into(),
                kind: InputKind::Rotary,
            },
            InputControl {
                id: "slider".into(),
                name: "Slider".into(),
                kind: InputKind::Analog {
                    min: 0.0,
                    max: 100.0,
                    unit: "%".into(),
                },
            },
        ]);
        d
    }
    fn default_config(&self) -> AppConfig {
        let page = alchemy_device_api::default_page("Home", &self.definition());
        AppConfig {
            active_page: page.id.clone(),
            pages: vec![page],
            ..AppConfig::empty()
        }
    }
    fn validate(&self, config: &AppConfig) -> Result<()> {
        if config.device_settings.screen_brightness > 20 {
            bail!("adapter limit")
        }
        Ok(())
    }
    fn run(self: Arc<Self>, host: AdapterHost) -> AdapterFuture {
        Box::pin(async move {
            host.cancel.cancelled().await;
            Ok(())
        })
    }
}
fn adapters() -> BTreeMap<String, Arc<dyn DeviceAdapter>> {
    ["test.a", "test.b"]
        .into_iter()
        .map(|id| (id.into(), Arc::new(Adapter(id)) as Arc<dyn DeviceAdapter>))
        .collect()
}
fn descriptor(adapter: &str, id: &str) -> DeviceDescriptor {
    DeviceDescriptor {
        adapter_id: adapter.into(),
        id: id.into(),
        model: "Fixture".into(),
        serial: id.into(),
        firmware: "1".into(),
        vendor_id: 0,
        product_id: 0,
        layout: Box::new(Adapter("test.a").layout()),
        capabilities: ["key_images", "key_gif", "device_control", "test.custom"]
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    Capability {
                        available: true,
                        verification: Verification::Unverified,
                        reason: String::new(),
                    },
                )
            })
            .collect(),
    }
}
fn connect(
    manager: &Arc<DeviceManager>,
    adapter: &str,
    local: &str,
) -> (Arc<Core>, AdapterHost, Connection) {
    let host = manager.adapter_host(adapter);
    let d = descriptor(adapter, local);
    host.api.discover(vec![d.clone()]).unwrap();
    let token = host
        .api
        .connected(d, format!("path-{adapter}-{local}"))
        .unwrap();
    (manager.device(&token.device_id).unwrap(), host, token)
}

#[test]
fn synchronous_startup_and_native_callbacks_use_the_host_runtime() {
    assert!(tokio::runtime::Handle::try_current().is_err());
    let fixture = Fixture::new();
    let manager = fixture.manager();
    manager.start(None, true);
    assert_eq!(manager.tasks.lock().unwrap().len(), 2);
    let (core, host, token) = connect(&manager, "test.a", "unit");
    let mut pages = core.snapshot().config.pages;
    let mut next = pages[0].clone();
    next.id = "next".into();
    pages[0].bindings.push(ControlBinding {
        control_id: "knob".into(),
        enabled: true,
        trigger: InputTrigger::Clockwise,
        action: KeyAction::Next,
    });
    pages.push(next);
    core.update_pages(pages).unwrap();
    host.api.input(
        &token,
        InputValue::Rotary {
            control_id: "knob".into(),
            delta: 1,
        },
    );
    tauri::async_runtime::block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while core.snapshot().active_page != "next" {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        manager.shutdown().await;
    });
    assert!(manager.tasks.lock().unwrap().is_empty());
}

#[tokio::test]
async fn every_save_validates_adapter_and_failed_commit_keeps_memory_and_disk() {
    let fixture = Fixture::new();
    let manager = fixture.manager();
    let (core, _, _) = connect(&manager, "test.a", "unit");
    let before = std::fs::read(fixture.0.join("config.json")).unwrap();
    let mut next = core.snapshot().config;
    next.device_settings.screen_brightness = 99;
    assert!(core.update_config(next).is_err());
    assert_eq!(core.snapshot().config.device_settings.screen_brightness, 0);
    assert_eq!(
        std::fs::read(fixture.0.join("config.json")).unwrap(),
        before
    );
    std::fs::create_dir(fixture.0.join("config.json.tmp")).unwrap();
    assert!(core
        .mark("test.custom", Verification::Verified, "observed")
        .is_err());
    assert!(matches!(
        core.snapshot().devices[0].capabilities["test.custom"].verification,
        Verification::Unverified
    ));
    std::fs::remove_dir(fixture.0.join("config.json.tmp")).unwrap();
    core.mark("test.custom", Verification::Verified, "observed")
        .unwrap();
    assert!(core.snapshot().devices[0].capabilities["test.custom"].available);
    assert!(fixture.manager().snapshot().unwrap().devices[0]
        .config_error
        .is_none());
}

#[tokio::test]
async fn scoped_ids_and_rejected_discovery_leave_existing_bindings_intact() {
    let fixture = Fixture::new();
    let manager = fixture.manager();
    let (a, host_a, token_a) = connect(&manager, "test.a", "same");
    let (_, host_b, token_b) = connect(&manager, "test.b", "same");
    assert_ne!(token_a.device_id, token_b.device_id);
    let before = serde_json::to_value(manager.snapshot().unwrap().discovered).unwrap();
    assert!(host_a
        .api
        .discover(vec![
            descriptor("test.a", "duplicate"),
            descriptor("test.a", "duplicate")
        ])
        .is_err());
    assert!(host_a
        .api
        .discover(vec![descriptor("test.b", "wrong-owner")])
        .is_err());
    assert_eq!(
        serde_json::to_value(manager.snapshot().unwrap().discovered).unwrap(),
        before
    );
    host_a.api.disconnected(&token_a);
    let token_new = host_a
        .api
        .connected(descriptor("test.a", "same"), token_a.path.clone())
        .unwrap();
    assert_ne!(token_new.epoch, token_a.epoch);
    host_a.api.disconnected(&token_a);
    assert!(host_a.api.connection_current(&token_new));
    host_a.api.input(
        &token_a,
        InputValue::Button {
            control_id: "1".into(),
            pressed: true,
        },
    );
    assert!(a.snapshot().pressed.is_empty());
    assert!(host_b.api.connection_current(&token_b));
}

#[test]
fn unknown_and_newer_profiles_are_preserved_and_do_not_block_healthy_profiles() {
    let fixture = Fixture::new();
    storage::initialize_workspace(&fixture.0).unwrap();
    let mut good = DeviceProfile::new(
        "test.a".into(),
        "Good".into(),
        None,
        Adapter("test.a").default_config(),
    );
    good.module_data = serde_json::json!({"nested":{"future":[1,2,3]}});
    let mut newer = good.clone();
    newer.id = "newer".into();
    newer.schema_version = 99;
    let mut unknown = good.clone();
    unknown.id = "unknown".into();
    unknown.adapter_id = "future.module".into();
    storage::edit_workspace(&fixture.0, |w| {
        w.profiles = vec![good, newer, unknown];
        Ok(())
    })
    .unwrap();
    let mut original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("config.json")).unwrap()).unwrap();
    original["profiles"][2]["config"]["future"] = serde_json::json!({"payload":7});
    original["profiles"][2]["vendor_field"] = serde_json::json!({"nested":true});
    std::fs::write(
        fixture.0.join("config.json"),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    let manager = fixture.manager();
    let snapshot = manager.snapshot().unwrap();
    assert!(snapshot.devices[0].config_error.is_none());
    assert!(snapshot.devices[1].config_error.is_some());
    assert!(snapshot.devices[2].config_error.is_some());
    storage::edit_workspace(&fixture.0, |w| {
        w.global.theme = "dark".into();
        Ok(())
    })
    .unwrap();
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join("config.json")).unwrap()).unwrap();
    assert_eq!(after["profiles"], original["profiles"]);
}

#[tokio::test]
async fn large_page_is_one_batch_and_input_events_keep_values_and_hysteresis() {
    let fixture = Fixture::new();
    let manager = fixture.manager();
    let (core, host, token) = connect(&manager, "test.a", "unit");
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !core.inner.lock().unwrap().widgets.page_ready {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut cursor = 0;
    let mut batch = host.api.next_batch(&mut cursor).unwrap();
    if !batch.page {
        host.api.complete(&batch.id, Ok(()));
        batch = host.api.next_batch(&mut cursor).unwrap();
    }
    assert!(batch.page);
    assert!(batch.work.len() > 64);
    assert!(host.api.current(&batch.id));
    host.api.complete(&batch.id, Ok(()));
    let mut pages = core.snapshot().config.pages;
    let mut other = pages[0].clone();
    other.id = "other".into();
    other.name = "Other".into();
    let target = other.id.clone();
    pages[0].bindings.push(ControlBinding {
        control_id: "knob".into(),
        enabled: true,
        trigger: InputTrigger::Clockwise,
        action: KeyAction::Page {
            page_id: target.clone(),
        },
    });
    pages.push(other);
    core.update_pages(pages).unwrap();
    let mut events = core.events.subscribe();
    host.api.input(
        &token,
        InputValue::Rotary {
            control_id: "knob".into(),
            delta: 7,
        },
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if core.snapshot().active_page == target {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut rotary_events = 0;
    while let Ok(event) = events.try_recv() {
        if let Some(InputValue::Rotary { delta, .. }) = event.input {
            assert_eq!(delta, 7);
            rotary_events += 1;
        }
    }
    assert_eq!(rotary_events, 1);
    host.api.input(
        &token,
        InputValue::Analog {
            control_id: "slider".into(),
            value: f64::NAN,
        },
    );
    assert!(!core.snapshot().input_values.contains_key("slider"));
    host.api.input(
        &token,
        InputValue::Analog {
            control_id: "slider".into(),
            value: 80.0,
        },
    );
    assert!(matches!(
        core.snapshot().input_values["slider"],
        InputValue::Analog { value: 80.0, .. }
    ));
    let mut pages = core.snapshot().config.pages;
    pages
        .iter_mut()
        .find(|p| p.id == target)
        .unwrap()
        .bindings
        .push(ControlBinding {
            control_id: "slider".into(),
            enabled: true,
            trigger: InputTrigger::Above {
                threshold: 0.5,
                hysteresis: 0.02,
            },
            action: KeyAction::Page {
                page_id: target.clone(),
            },
        });
    core.update_pages(pages).unwrap();
    async fn settle(core: &Core) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while core.action_slots.available_permits() < 8 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    fn page_events(events: &mut tokio::sync::broadcast::Receiver<DeviceEvent>) -> usize {
        let mut count = 0;
        while let Ok(event) = events.try_recv() {
            if event.kind == "page.changed" {
                count += 1
            }
        }
        count
    }
    settle(&core).await;
    let mut events = core.events.subscribe();
    for (value, expected) in [
        (80.0, 0),
        (40.0, 0),
        (51.0, 1),
        (49.0, 0),
        (51.0, 0),
        (47.0, 0),
        (50.0, 1),
    ] {
        host.api.input(
            &token,
            InputValue::Analog {
                control_id: "slider".into(),
                value,
            },
        );
        settle(&core).await;
        assert_eq!(page_events(&mut events), expected, "value={value}");
    }
    host.api.disconnected(&token);
    let new_token = host
        .api
        .connected(descriptor("test.a", "unit"), token.path.clone())
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if events.recv().await.unwrap().kind == "page.changed" {
                break;
            }
        }
    })
    .await
    .unwrap();
    page_events(&mut events);
    host.api.input(
        &new_token,
        InputValue::Analog {
            control_id: "slider".into(),
            value: 80.0,
        },
    );
    settle(&core).await;
    assert_eq!(
        page_events(&mut events),
        0,
        "first sample after reconnect must not trigger"
    );
}

struct MigratingAdapter;
impl DeviceAdapter for MigratingAdapter {
    fn id(&self) -> &'static str {
        "test.migration"
    }
    fn layout(&self) -> DeviceLayout {
        DeviceLayout::default()
    }
    fn default_config(&self) -> AppConfig {
        AppConfig::empty()
    }
    fn config_version(&self) -> u32 {
        2
    }
    fn default_module_config(&self) -> serde_json::Value {
        serde_json::json!({"value":2})
    }
    fn migrate_module(&self, from: u32, mut data: serde_json::Value) -> Result<serde_json::Value> {
        if from != 1 || data["fail"] == true {
            bail!("migration rejected")
        }
        data["value"] = 2.into();
        Ok(data)
    }
    fn run(self: Arc<Self>, host: AdapterHost) -> AdapterFuture {
        Box::pin(async move {
            host.cancel.cancelled().await;
            Ok(())
        })
    }
}
#[test]
fn module_migration_is_atomic_and_failed_or_malformed_profiles_remain_opaque() {
    let fixture = Fixture::new();
    storage::initialize_workspace(&fixture.0).unwrap();
    let good = DeviceProfile::new(
        "test.migration".into(),
        "Good".into(),
        None,
        AppConfig::empty(),
    );
    let mut failed = good.clone();
    failed.id = "failed".into();
    failed.module_data = serde_json::json!({"fail":true});
    storage::edit_workspace(&fixture.0, |w| {
        w.profiles = vec![good, failed];
        Ok(())
    })
    .unwrap();
    let before = serde_json::to_value(storage::load_workspace(&fixture.0).unwrap()).unwrap();
    std::fs::create_dir(fixture.0.join("config.json.tmp")).unwrap();
    let failed_save = DeviceManager::with_adapters(
        Core::new(fixture.0.clone()).unwrap(),
        BTreeMap::from([(
            "test.migration".into(),
            Arc::new(MigratingAdapter) as Arc<dyn DeviceAdapter>,
        )]),
    )
    .unwrap();
    assert!(failed_save.snapshot().unwrap().devices[0]
        .config_error
        .is_some());
    assert_eq!(
        serde_json::to_value(storage::load_workspace(&fixture.0).unwrap()).unwrap(),
        before
    );
    drop(failed_save);
    std::fs::remove_dir(fixture.0.join("config.json.tmp")).unwrap();
    let registry = BTreeMap::from([(
        "test.migration".into(),
        Arc::new(MigratingAdapter) as Arc<dyn DeviceAdapter>,
    )]);
    let manager =
        DeviceManager::with_adapters(Core::new(fixture.0.clone()).unwrap(), registry).unwrap();
    let snapshot = manager.snapshot().unwrap();
    assert!(snapshot.devices[0].config_error.is_none());
    assert!(snapshot.devices[1].config_error.is_some());
    let after = serde_json::to_value(storage::load_workspace(&fixture.0).unwrap()).unwrap();
    assert_eq!(after["profiles"][0]["schema_version"], 2);
    assert_eq!(after["profiles"][0]["module_data"]["value"], 2);
    assert_eq!(after["profiles"][1], before["profiles"][1]);
    let mut malformed = after;
    malformed["profiles"][1] = serde_json::json!({"id":"broken","adapter_id":"test.migration","name":"Broken","device_id":7,"config":{"future_action":{"type":"unknown"}},"vendor":{"keep":[1,2]}});
    let decoded = storage::decode_workspace(&serde_json::to_vec(&malformed).unwrap()).unwrap();
    assert!(decoded.profiles[1].config_error.is_some());
    assert_eq!(serde_json::to_value(decoded).unwrap(), malformed);
}

#[tokio::test]
async fn module_configuration_uses_the_same_failure_and_session_guards() {
    let fixture = Fixture::new();
    let manager = fixture.manager();
    let (_, host, token) = connect(&manager, "test.a", "unit");
    assert!(host
        .api
        .save_module_config(&token, serde_json::json!(7))
        .is_err());
    let before = host.api.module_config(&token).unwrap();
    std::fs::create_dir(fixture.0.join("config.json.tmp")).unwrap();
    assert!(host
        .api
        .save_module_config(&token, serde_json::json!({"value":1}))
        .is_err());
    assert_eq!(host.api.module_config(&token).unwrap(), before);
    std::fs::remove_dir(fixture.0.join("config.json.tmp")).unwrap();
    host.api
        .save_module_config(&token, serde_json::json!({"value":1}))
        .unwrap();
    assert_eq!(host.api.module_config(&token).unwrap()["value"], 1);
    host.api.disconnected(&token);
    assert!(host
        .api
        .save_module_config(&token, serde_json::json!({"value":2}))
        .is_err());
}

#[test]
fn displays_can_exist_without_numeric_button_inputs() {
    let definition = DeviceDefinition {
        adapter_id: "test.display".into(),
        name: "Display with dial".into(),
        supports_pages: true,
        inputs: vec![InputControl {
            id: "dial".into(),
            name: "Dial".into(),
            kind: InputKind::Rotary,
        }],
        displays: vec![alchemy_device_api::DisplayRegion {
            id: "status".into(),
            key: 1,
            width: 128,
            height: 64,
            column: 0,
            row: 0,
        }],
        layout: DeviceLayout {
            display_keys: vec![1],
            columns: 1,
            ..Default::default()
        },
        input_names: Default::default(),
    };
    let page = alchemy_device_api::default_page("Home", &definition);
    let mut config = AppConfig {
        active_page: page.id.clone(),
        pages: vec![page],
        ..AppConfig::empty()
    };
    config.pages[0].bindings.push(ControlBinding {
        control_id: "dial".into(),
        enabled: true,
        trigger: InputTrigger::Clockwise,
        action: KeyAction::Next,
    });
    storage::validate_device_config(&config, &definition).unwrap();
    assert_eq!(config.pages[0].keys.len(), 1);
}
