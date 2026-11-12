use super::*;
use crate::model::{AssetLibrary, CloseBehavior};
use std::sync::Mutex;

async fn ok(client: &RunningService<RoleClient, ()>, name: &'static str, args: Value) -> Value {
    let result = call(client, name, args).await;
    assert_ne!(result.is_error, Some(true), "{name}: {result:?}");
    result.structured_content.unwrap()
}
async fn finish_job(client: &RunningService<RoleClient, ()>, operation_id: &Value) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let result = ok(
                client,
                "alchemy_get_operation",
                json!({"operation_id":operation_id}),
            )
            .await;
            if result["status"] != "running" {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
async fn job(client: &RunningService<RoleClient, ()>, id: &str, operation: Value) -> Value {
    let started = ok(
        client,
        "alchemy_start_operation",
        json!({"request_id":id,"operation":operation}),
    )
    .await;
    finish_job(client, &started["operation_id"]).await
}
fn data() -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png())
    )
}

#[tokio::test]
async fn library_management_is_persistent_revision_checked_and_preserves_originals() {
    let h = Harness::new(false).await;
    let client = h.client().await;
    let initial = ok(&client, "alchemy_get_library", json!({})).await;
    let folder=ok(&client,"alchemy_edit_library",json!({"expected_revision":initial["revision"],"edit":{"type":"create_folder","parent_id":null,"name":"AI assets"}})).await;
    let id = folder["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "AI assets")
        .unwrap()["id"]
        .clone();
    let imported=ok(&client,"alchemy_import_library",json!({"expected_revision":folder["revision"],"folder_id":id,"items":[{"name":"sample.png","data":data()},{"name":"bad.png","data":"invalid"}]})).await;
    assert_eq!(imported["results"][0]["saved"], true);
    assert_eq!(imported["results"][1]["saved"], false);
    let original = imported["results"][0]["asset_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let preview = call(
        &client,
        "alchemy_preview_image",
        json!({"image":{"type":"asset","asset_id":original}}),
    )
    .await;
    assert!(
        preview
            .content
            .iter()
            .any(|content| content.as_image().is_some()),
        "The AI client must receive an image content block"
    );

    let library = ok(&client, "alchemy_get_library", json!({"query":"sample"})).await;
    let item = library["data"]["items"][0]["id"].clone();
    let (a, b) = tokio::join!(
        call(
            &client,
            "alchemy_edit_library",
            json!({"expected_revision":library["revision"],"edit":{"type":"rename_item","id":item,"name":"a.png"}})
        ),
        call(
            &client,
            "alchemy_edit_library",
            json!({"expected_revision":library["revision"],"edit":{"type":"rename_item","id":item,"name":"b.png"}})
        )
    );
    assert_ne!(
        a.is_error, b.is_error,
        "Only one edit from the same revision may commit"
    );
    let stale = if a.is_error == Some(true) { a } else { b };
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_operation"
    );
    let library = ok(&client, "alchemy_get_library", json!({})).await;
    let next=ok(&client,"alchemy_edit_library",json!({"expected_revision":library["revision"],"edit":{"type":"move","folder_ids":[],"item_ids":[item],"parent_id":null}})).await;
    let mut pages = h.core.snapshot().config.pages;
    pages[0].keys[0].asset_id = Some(original.clone());
    h.core.update_pages(pages).unwrap();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let replaced=ok(&client,"alchemy_import_library",json!({"expected_revision":next["revision"],"folder_id":null,"items":[{"name":"replacement.png","replace_item_id":item,"data":base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())}]})).await;
    assert_ne!(replaced["results"][0]["asset_id"], original);
    assert_eq!(
        h.core.snapshot().config.pages[0].keys[0]
            .asset_id
            .as_deref(),
        Some(original.as_str())
    );
    let next = ok(&client, "alchemy_get_library", json!({})).await;
    // An atomic persistence failure must not publish an in-memory rename.
    std::fs::create_dir(h.core.root.join("library.json.tmp")).unwrap();
    assert_eq!(call(&client,"alchemy_edit_library",json!({"expected_revision":next["revision"],"edit":{"type":"rename_item","id":item,"name":"must-not-commit"}})).await.is_error,Some(true));
    std::fs::remove_dir(h.core.root.join("library.json.tmp")).unwrap();
    assert!(!h
        .core
        .library
        .snapshot()
        .unwrap()
        .items
        .iter()
        .any(|i| i.name == "must-not-commit"));
    ok(&client,"alchemy_edit_library",json!({"expected_revision":next["revision"],"edit":{"type":"delete","folder_ids":[id],"item_ids":[item]}})).await;
    assert!(storage::asset(&h.core.root, &original).unwrap().is_file());
    let saved: AssetLibrary =
        serde_json::from_slice(&std::fs::read(h.core.root.join("library.json")).unwrap()).unwrap();
    assert!(!saved.items.iter().any(|i| i.id == item.as_str().unwrap()));
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn presets_pages_scenes_and_offline_profiles_round_trip_without_launching() {
    let h = Harness::new(false).await;
    let client = h.client().await;
    let profile = h.core.profile_id.clone().unwrap();
    let apps = ok(&client, "alchemy_get_applications", json!({})).await;
    let apps=ok(&client,"alchemy_edit_applications",json!({"expected_revision":apps["revision"],"edit":{"type":"save","id":null,"name":"Fixture app","target":{"type":"program","path":"C:/missing/mcp-fixture.exe","arguments":"--isolated","working_directory":null}}})).await;
    let app = apps["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "Fixture app")
        .unwrap()["id"]
        .clone();
    let before = ok(
        &client,
        "alchemy_get_profile",
        json!({"profile_id":profile}),
    )
    .await;
    let active = before["runtime"]["active_page"].clone();
    ok(&client,"alchemy_edit_pages",json!({"profile_id":profile,"expected_revision":before["revisions"]["pages"],"edit":{"type":"create","name":"AI workspace"}})).await;
    let snapshot = ok(
        &client,
        "alchemy_get_profile",
        json!({"profile_id":profile}),
    )
    .await;
    let page = snapshot["saved"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "AI workspace")
        .unwrap()["id"]
        .clone();
    ok(&client,"alchemy_edit_pages",json!({"profile_id":profile,"expected_revision":snapshot["revisions"]["pages"],"edit":{"type":"apply_application","page_id":page,"control_id":"1","application_id":app}})).await;
    let snapshot = ok(
        &client,
        "alchemy_get_profile",
        json!({"profile_id":profile}),
    )
    .await;
    assert_eq!(snapshot["runtime"]["active_page"], active);
    let saved = snapshot["saved"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == page)
        .unwrap();
    assert_eq!(saved["keys"][0]["text"], "Fixture app");
    assert!(saved["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["control_id"] == "1" && b["action"]["type"] == "launch"));
    let mut visual = saved["keys"][0].clone();
    visual["content_mode"] = json!("none");
    visual["show_text"] = json!(false);
    visual["text_position"] = json!("bottom");
    let preview = ok(
        &client,
        "alchemy_preview_key",
        json!({"profile_id":profile,"visual":visual}),
    )
    .await;
    let png = base64::engine::general_purpose::STANDARD
        .decode(
            preview["data_url"]
                .as_str()
                .unwrap()
                .split_once(',')
                .unwrap()
                .1,
        )
        .unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgb8();
    assert!(image.pixels().all(|p| p == image.get_pixel(0, 0)));
    ok(&client, "alchemy_edit_pages", json!({"profile_id":profile,"expected_revision":snapshot["revisions"]["pages"],"edit":{"type":"set_keys","page_id":page,"keys":[visual]}})).await;
    let snapshot = ok(
        &client,
        "alchemy_get_profile",
        json!({"profile_id":profile}),
    )
    .await;
    let saved_visual = &snapshot["saved"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == page)
        .unwrap()["keys"][0];
    assert_eq!(saved_visual, &visual);
    // A stale desktop autosave uses the same owner check as MCP and retains saved data.
    assert!(h
        .core
        .update_pages_checked(
            serde_json::from_value(before["saved"]["pages"].clone()).unwrap(),
            before["revisions"]["pages"].as_str()
        )
        .is_err());
    let mut scenes = snapshot["saved"]["scene_settings"].clone();
    scenes["paused"] = json!(true);
    ok(&client,"alchemy_edit_scenes",json!({"profile_id":profile,"expected_revision":snapshot["revisions"]["scenes"],"settings":scenes})).await;
    h.manager.configure(false, h.port).await.unwrap();
    let durable = storage::load_workspace(&h.core.root)
        .unwrap()
        .profiles
        .into_iter()
        .find(|p| p.id == profile)
        .unwrap();
    assert!(durable
        .config
        .pages
        .iter()
        .any(|p| p.name == "AI workspace"));
    assert!(durable.config.scene_settings.paused);
    assert_eq!(h.core.snapshot().sdk_pid, None);
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn preferences_and_m18_settings_keep_persistence_distinct_from_acceptance() {
    let h = Harness::new(true).await;
    let client = h.client().await;
    let prefs = ok(&client, "alchemy_get_app_settings", json!({})).await;
    let mut settings = prefs["data"].clone();
    settings["language"] = json!("en");
    settings["close_behavior"] = json!("exit");
    ok(
        &client,
        "alchemy_update_app_settings",
        json!({"expected_revision":prefs["revision"],"settings":settings}),
    )
    .await;
    assert_eq!(
        call(
            &client,
            "alchemy_update_app_settings",
            json!({"expected_revision":prefs["revision"],"settings":prefs["data"]})
        )
        .await
        .is_error,
        Some(true)
    );
    ok(
        &client,
        "alchemy_update_theme",
        json!({"expected_revision":prefs["theme_revision"],"theme":"dark"}),
    )
    .await;
    let current = ok(
        &client,
        "alchemy_get_device_settings",
        json!({"device_id":"simulated"}),
    )
    .await;
    assert_eq!(current["source"], "saved_configuration");
    assert_eq!(current["hardware_readback_supported"], false);
    let mut settings = current["data"].clone();
    settings["screen_brightness"] = json!(42);
    settings["led_mode"] = json!("breathing");
    settings["led_speed"] = json!(7);
    let result=ok(&client,"alchemy_update_device_settings",json!({"device_id":"simulated","edit":{"type":"m18","expected_revision":current["revision"],"settings":settings}})).await;
    assert_eq!(result["saved"], true);
    assert_eq!(result["verification"], "not_observed");
    h.manager.configure(false, h.port).await.unwrap();
    assert_eq!(
        h.core.snapshot().config.device_settings.screen_brightness,
        42
    );
    assert_eq!(h.core.snapshot().config.device_settings.led_speed, 7);
    assert_eq!(
        crate::desktop::Preferences::load(&h.core.root)
            .settings()
            .unwrap()
            .close_behavior,
        CloseBehavior::Exit
    );
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn backup_jobs_are_deduplicated_and_restore_is_staged_with_fingerprint() {
    let h = Harness::new(false).await;
    let client = h.client().await;
    let archive = h.core.root.with_extension("m18backup");
    let operation = json!({"type":"backup_export","path":archive});
    let exported = job(&client, "export-once", operation.clone()).await;
    assert_eq!(exported["status"], "completed", "{exported}");
    let duplicate = ok(
        &client,
        "alchemy_start_operation",
        json!({"request_id":"export-once","operation":operation}),
    )
    .await;
    assert_eq!(duplicate["operation_id"], exported["operation_id"]);
    assert_eq!(
        call(
            &client,
            "alchemy_start_operation",
            json!({"request_id":"export-once","operation":{"type":"backup_inspect","path":archive}})
        )
        .await
        .is_error,
        Some(true)
    );
    let inspected = job(
        &client,
        "inspect",
        json!({"type":"backup_inspect","path":archive}),
    )
    .await;
    assert_eq!(inspected["status"], "completed", "{inspected}");
    let rejected = job(
        &client,
        "bad-restore",
        json!({"type":"backup_restore","path":archive,"fingerprint":"wrong"}),
    )
    .await;
    assert_eq!(rejected["status"], "failed");
    assert!(!h.core.root.join("backup-pending").exists());
    let restored=job(&client,"restore",json!({"type":"backup_restore","path":archive,"fingerprint":inspected["result"]["fingerprint"]})).await;
    assert_eq!(restored["status"], "completed", "{restored}");
    assert_eq!(restored["result"]["staged"], true);
    assert!(h.core.root.join("backup-pending").is_dir());
    let reloaded = Operations::load(&h.core.root);
    assert_eq!(
        reloaded
            .get(exported["operation_id"].as_str().unwrap())
            .unwrap()["status"],
        "completed"
    );
    let notice = crate::backup::apply_pending(&h.core.root).unwrap().unwrap();
    assert!(notice.restored);
    std::fs::remove_file(archive).unwrap();
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn malformed_management_requests_do_not_modify_resources_or_execute_operations() {
    let h = Harness::new(false).await;
    let before = std::fs::read(h.core.root.join("config.json")).unwrap();
    for (name, args) in [
        (
            "alchemy_edit_library",
            json!({"expected_revision":"x","edit":{"type":"create_folder","name":"x","parent_id":null,"unexpected":true}}),
        ),
        (
            "alchemy_edit_pages",
            json!({"profile_id":"unknown","expected_revision":"x","edit":{"type":"create","name":"x"}}),
        ),
        (
            "alchemy_update_device_settings",
            json!({"device_id":"simulated","edit":{"type":"raw_hid","bytes":[1]}}),
        ),
        (
            "alchemy_start_operation",
            json!({"request_id":"x","operation":{"type":"shell","command":"anything"}}),
        ),
    ] {
        let response = h
            .raw("tools/call", json!({"name":name,"arguments":args}))
            .await;
        assert!(
            response.get("error").is_some() || response["result"]["isError"] == true,
            "{response}"
        );
    }
    assert_eq!(
        before,
        std::fs::read(h.core.root.join("config.json")).unwrap()
    );
    assert!(!h.core.root.join("mcp-operations.json").exists());
    h.finish().await;
}

#[tokio::test]
async fn plugin_inspection_installation_and_removal_share_the_package_owner() {
    use std::io::Write;
    let h = Harness::new(false).await;
    let client = h.client().await;
    let path = h.core.root.join("fixture.m18plugin");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name,bytes) in [("plugin.json",br#"{"id":"test.mcp-package","name":"Fixture","version":"1","api_version":3,"executable":"fixture.exe","permissions":[]}"#.as_slice()),("fixture.exe",b"not executed".as_slice())] {
        archive.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap();
    let inspected = job(
        &client,
        "inspect-plugin",
        json!({"type":"plugin_inspect","path":path}),
    )
    .await;
    assert_eq!(inspected["status"], "completed", "{inspected}");
    let wrong = job(
        &client,
        "wrong-plugin",
        json!({"type":"plugin_install","path":path,"fingerprint":"wrong"}),
    )
    .await;
    assert_eq!(wrong["status"], "failed");
    let installed=job(&client,"install-plugin",json!({"type":"plugin_install","path":path,"fingerprint":inspected["result"]["fingerprint"]})).await;
    assert_eq!(installed["status"], "completed", "{installed}");
    assert!(h
        .core
        .snapshot()
        .config
        .disabled_plugins
        .iter()
        .any(|id| id == "test.mcp-package"));
    assert!(h
        .core
        .root
        .join("plugins/test.mcp-package/fixture.exe")
        .is_file());
    assert_eq!(
        h.manager
            .services
            .plugins
            .running_pid("test.mcp-package")
            .await,
        None
    );
    let removed = job(
        &client,
        "remove-plugin",
        json!({"type":"plugin_remove","id":"test.mcp-package"}),
    )
    .await;
    assert_eq!(removed["status"], "completed", "{removed}");
    assert!(!h.core.root.join("plugins/test.mcp-package").exists());
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn damaged_operation_history_is_preserved_and_cannot_replay_jobs() {
    let h = Harness::new(false).await;
    std::fs::write(h.core.root.join("mcp-operations.json"), b"{broken").unwrap();
    let operations = Operations::load(&h.core.root);
    let result = operations.submit(
        h.manager.services.clone(),
        JobRequest {
            request_id: "unique".into(),
            operation: Operation::Launch {
                target: crate::model::LaunchTarget::Program {
                    path: "C:/not-launched.exe".into(),
                    arguments: String::new(),
                    working_directory: None,
                },
            },
        },
        Arc::new(RwLock::new(true)).read_owned().await,
    );
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(h.core.root.join("mcp-operations.json")).unwrap(),
        b"{broken"
    );
    h.finish().await;
}

#[derive(Default)]
struct RestartFixture(std::sync::atomic::AtomicUsize);
impl crate::management::DesktopBridge for RestartFixture {
    fn preferences_changed(&self, _: &crate::model::AppSettings) {}
    fn resource_changed(&self, _: &str) {}
    fn request_restart(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
#[tokio::test]
async fn restore_requests_restart_only_after_a_complete_http_response() {
    let bridge = Arc::new(RestartFixture::default());
    let h = Harness::with_bridge(false, Some(bridge.clone())).await;
    let client = h.client().await;
    let archive = h.core.root.with_extension("m18backup");
    h.manager
        .services
        .export_backup(archive.to_str().unwrap())
        .unwrap();
    let summary = crate::backup::inspect(&h.core.root, &archive).unwrap();
    let started = h
        .manager
        .operations
        .submit(
            h.manager.services.clone(),
            JobRequest {
                request_id: "restore-with-response".into(),
                operation: Operation::BackupRestore {
                    path: archive.to_string_lossy().into(),
                    fingerprint: summary.fingerprint,
                },
            },
            Arc::new(RwLock::new(true)).read_owned().await,
        )
        .unwrap();
    // Observe job completion locally, deliberately without another HTTP response.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !h.manager.operations.restart_pending() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(bridge.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    let response = ok(
        &client,
        "alchemy_get_operation",
        json!({"operation_id":started["operation_id"]}),
    )
    .await;
    assert_eq!(response["status"], "completed");
    tokio::time::timeout(Duration::from_secs(3), async {
        while bridge.0.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(bridge.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    std::fs::remove_file(archive).unwrap();
    client.cancel().await.unwrap();
    h.finish().await;
}

#[cfg(feature = "device-ms-audio-dock")]
struct SettingsFixture {
    volume: Mutex<u8>,
    fail_readback: std::sync::atomic::AtomicBool,
}
#[cfg(feature = "device-ms-audio-dock")]
impl crate::devices::DeviceAdapter for SettingsFixture {
    fn id(&self) -> &'static str {
        crate::devices::ms_audio_dock::ID
    }
    fn layout(&self) -> crate::model::DeviceLayout {
        crate::devices::ms_audio_dock::Adapter::default().layout()
    }
    fn default_config(&self) -> crate::model::AppConfig {
        crate::devices::ms_audio_dock::Adapter::default().default_config()
    }
    fn operations(&self) -> &'static [&'static str] {
        &["read", "update"]
    }
    fn run(
        self: Arc<Self>,
        host: alchemy_device_api::AdapterHost,
    ) -> alchemy_device_api::AdapterFuture {
        Box::pin(async move {
            host.cancel.cancelled().await;
            Ok(())
        })
    }
    fn request<'a>(
        &'a self,
        host: alchemy_device_api::AdapterHost,
        connection: alchemy_device_api::Connection,
        operation: String,
        payload: Value,
    ) -> alchemy_device_api::ServiceFuture<'a> {
        Box::pin(async move {
            anyhow::ensure!(host.api.connection_current(&connection), "Stale connection");
            if operation == "update" {
                let update: crate::devices::ms_audio_dock::types::AudioDockSettingsUpdate =
                    serde_json::from_value(payload)?;
                match update {crate::devices::ms_audio_dock::types::AudioDockSettingsUpdate::PromptVolume{value}=>{anyhow::ensure!(value<=100,"Invalid volume");*self.volume.lock().unwrap()=value;},_=>anyhow::bail!("Fixture only implements prompt volume")}
                anyhow::ensure!(
                    !self.fail_readback.load(std::sync::atomic::Ordering::SeqCst),
                    "Simulated readback failure after write"
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            anyhow::ensure!(host.api.connection_current(&connection), "Stale connection");
            Ok(
                json!({"device_id":connection.device_id,"settings":{"prompt_volume":*self.volume.lock().unwrap()}}),
            )
        })
    }
}

#[tokio::test]
#[cfg(feature = "device-ms-audio-dock")]
async fn dock_management_reads_and_updates_through_module_service_and_rejects_late_results() {
    let h = Harness::new(false).await;
    let adapter = Arc::new(SettingsFixture {
        volume: Mutex::new(35),
        fail_readback: std::sync::atomic::AtomicBool::new(false),
    });
    let devices = crate::devices::DeviceManager::with_adapters(
        h.core.clone(),
        [(
            crate::devices::ms_audio_dock::ID.into(),
            adapter.clone() as Arc<dyn crate::devices::DeviceAdapter>,
        )]
        .into(),
    )
    .unwrap();
    let descriptor = DeviceDescriptor {
        adapter_id: crate::devices::ms_audio_dock::ID.into(),
        layout: crate::devices::DeviceAdapter::layout(&*adapter).into(),
        id: "fixture-dock".into(),
        model: "Fixture Dock".into(),
        serial: String::new(),
        firmware: String::new(),
        vendor_id: 0x045e,
        product_id: 0x084d,
        capabilities: Default::default(),
    };
    devices
        .discovered_for(crate::devices::ms_audio_dock::ID, vec![descriptor.clone()])
        .unwrap();
    let core = devices.device_profile("fixture-dock").unwrap();
    core.attach_input_device(descriptor, "fixture-dock-path".into());
    let before = std::fs::read(h.core.root.join("config.json")).unwrap();
    let client = h.client().await;
    let read = ok(
        &client,
        "alchemy_get_device_settings",
        json!({"device_id":"fixture-dock"}),
    )
    .await;
    assert_eq!(read["source"], "hardware_readback");
    assert_eq!(read["data"]["settings"]["prompt_volume"], 35);
    let update=ok(&client,"alchemy_update_device_settings",json!({"device_id":"fixture-dock","edit":{"type":"audio_dock","update":{"type":"prompt_volume","value":56}}})).await;
    assert_eq!(update["verification"], "readback_confirmed");
    adapter
        .fail_readback
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let failed=call(&client,"alchemy_update_device_settings",json!({"device_id":"fixture-dock","edit":{"type":"audio_dock","update":{"type":"prompt_volume","value":67}}})).await;
    assert_eq!(failed.is_error, Some(true));
    assert_eq!(
        *adapter.volume.lock().unwrap(),
        67,
        "An uncertain write must not be automatically retried or reported as verified"
    );
    let (late, _) = tokio::join!(
        call(
            &client,
            "alchemy_get_device_settings",
            json!({"device_id":"fixture-dock"})
        ),
        async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            core.clear_connection();
        }
    );
    assert_eq!(late.is_error, Some(true));
    assert_eq!(
        before,
        std::fs::read(h.core.root.join("config.json")).unwrap()
    );
    client.cancel().await.unwrap();
    h.finish().await;
}
