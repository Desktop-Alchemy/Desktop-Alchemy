use super::*;
#[path = "management_tests.rs"]
mod management;
use crate::devices::default_capabilities as capabilities;
use crate::model::DeviceDescriptor;
use base64::Engine;
use rmcp::ClientServiceExt;
use rmcp::{
    model::{
        CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation, ProtocolVersion,
    },
    service::{ClientLifecycleMode, RunningService},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    RoleClient, ServiceExt,
};

struct Harness {
    manager: Arc<McpManager>,
    core: Arc<Core>,
    port: u16,
}
impl Harness {
    async fn new(connected: bool) -> Self {
        Self::with_bridge(connected, None).await
    }
    async fn with_bridge(
        connected: bool,
        bridge: Option<Arc<dyn crate::management::DesktopBridge>>,
    ) -> Self {
        let core = Core::new_test_device(
            std::env::temp_dir().join(format!("m18-mcp-test-{}", uuid::Uuid::new_v4())),
        )
        .unwrap();
        if connected {
            let mut i = core.inner.lock().unwrap();
            i.snapshot.connection = "ready".into();
            i.snapshot.selected_device = Some("simulated".into());
            i.paths.insert("simulated".into(), "test-path".into());
            i.snapshot.devices.push(DeviceDescriptor {
                adapter_id: crate::devices::default_adapter_id().into(),
                layout: crate::devices::default_layout().into(),
                id: "simulated".into(),
                model: "M18".into(),
                serial: "test".into(),
                firmware: "V3.M18".into(),
                vendor_id: 0x6603,
                product_id: 0x1012,
                capabilities: capabilities("V3.M18"),
            });
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut services = Services::headless(core.clone());
        Arc::get_mut(&mut services).unwrap().desktop = bridge;
        let manager = McpManager::with_services(services);
        assert_eq!(
            manager.configure(true, port).await.unwrap().status,
            "running"
        );
        Self {
            manager,
            core,
            port,
        }
    }
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }
    async fn client(&self) -> RunningService<RoleClient, ()> {
        ().serve(StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(self.url())
                .auth_header(self.manager.token().await.unwrap()),
        ))
        .await
        .unwrap()
    }
    async fn raw(&self, method: &str, arguments: Value) -> Value {
        reqwest::Client::new()
            .post(self.url())
            .bearer_auth(self.manager.token().await.unwrap())
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-11-25")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":arguments}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    async fn finish(self) {
        self.manager.shutdown().await.unwrap();
        assert!(tokio::net::TcpStream::connect(("127.0.0.1", self.port))
            .await
            .is_err());
        std::fs::remove_dir_all(&self.core.root).unwrap();
    }
}
async fn call(
    client: &RunningService<RoleClient, ()>,
    name: &'static str,
    args: Value,
) -> CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap()
}
fn png() -> Vec<u8> {
    let mut data = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
        .write_to(&mut data, image::ImageFormat::Png)
        .unwrap();
    data.into_inner()
}

#[tokio::test]
async fn official_client_round_trip_and_legacy_http() {
    let h = Harness::new(true).await;
    let client = h.client().await;
    let tools = client.list_all_tools().await.unwrap();
    for name in [
        "alchemy_get_status",
        "alchemy_import_asset",
        "alchemy_device_command",
        "alchemy_get_command_receipt",
        "alchemy_edit_pages",
        "alchemy_import_library",
        "alchemy_update_device_settings",
        "alchemy_start_operation",
    ] {
        assert!(tools.iter().any(|tool| tool.name == name), "Missing {name}");
    }
    let command_schema = tools
        .iter()
        .find(|t| t.name == "alchemy_device_command")
        .unwrap()
        .input_schema
        .as_ref();
    let schema = serde_json::to_string(command_schema).unwrap();
    assert!(schema.contains("key_image"));
    assert!(!schema.contains("\"persistent\":"));
    let status = call(&client, "alchemy_get_status", json!({})).await;
    assert_eq!(
        status.structured_content.as_ref().unwrap()["devices"][0]["device_id"],
        "simulated"
    );
    assert!(!serde_json::to_string(&status)
        .unwrap()
        .contains(&h.manager.token().await.unwrap()));
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png())
    );
    let imported = call(&client, "alchemy_import_asset", json!({"data":data})).await;
    let id = imported.structured_content.unwrap()["asset_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let local = h.core.root.join("input.png");
    std::fs::write(&local, png()).unwrap();
    // Core canonicalizes to a Win32 extended path internally; MCP accepts ordinary paths.
    let ordinary = local
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let from_path = call(&client, "alchemy_import_asset", json!({"path":ordinary})).await;
    assert_eq!(from_path.is_error, Some(false), "{from_path:?}");
    assert_eq!(from_path.structured_content.unwrap()["asset_id"], id);
    let before = std::fs::read(h.core.root.join("config.json")).unwrap();
    let submitted = call(
        &client,
        "alchemy_device_command",
        json!({"device_id":"simulated","command":{"type":"key_image","key":1,"asset_id":id}}),
    )
    .await;
    let receipt = submitted.structured_content.unwrap()["receipt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let polled = call(
        &client,
        "alchemy_get_command_receipt",
        json!({"device_id":"simulated","receipt_id":receipt}),
    )
    .await;
    assert_eq!(polled.structured_content.unwrap()["status"], "queued");
    assert_eq!(
        h.core
            .inner
            .lock()
            .unwrap()
            .queue
            .back()
            .unwrap()
            .source
            .as_deref(),
        Some(OWNER)
    );
    h.core.receipt(&receipt, "sent", "simulation");
    let sent = call(
        &client,
        "alchemy_get_command_receipt",
        json!({"device_id":"simulated","receipt_id":receipt}),
    )
    .await;
    assert_eq!(sent.structured_content.unwrap()["status"], "sent");
    let legacy = h
        .raw(
            "tools/call",
            json!({"name":"alchemy_get_status","arguments":{}}),
        )
        .await;
    assert_eq!(
        legacy["result"]["structuredContent"]["devices"][0]["connection"],
        "ready"
    );
    assert_eq!(
        before,
        std::fs::read(h.core.root.join("config.json")).unwrap()
    );
    assert_eq!(h.core.snapshot().sdk_pid, None);
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn http_auth_origin_host_and_body_limits() {
    let h = Harness::new(false).await;
    let http = reqwest::Client::new();
    let token = h.manager.token().await.unwrap();
    assert_eq!(http.get(h.url()).send().await.unwrap().status(), 401);
    assert_eq!(
        http.post(h.url())
            .bearer_auth("wrong")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    for (header, value) in [
        ("Host", "evil.example"),
        ("Origin", "https://evil.example"),
        ("Origin", "null"),
    ] {
        let response = http
            .post(h.url())
            .bearer_auth(&token)
            .header(header, value)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403, "{header}: {value}");
    }
    let response = http.post(h.url()).bearer_auth(&token)
        .header("Origin", format!("http://127.0.0.1:{}", h.port))
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let too_large = http
        .post(h.url())
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(vec![b' '; MAX_BODY + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(too_large.status(), 413);
    h.finish().await;
}

#[tokio::test]
async fn invalid_inputs_never_queue_hardware_work() {
    let h = Harness::new(true).await;
    let client = h.client().await;
    for command in [
        json!({"type":"screen_brightness","value":101}),
        json!({"type":"clear_key","key":16}),
        json!({"type":"led_colors","colors":[[0,0,0]]}),
        json!({"type":"key_image","key":1,"asset_id":"../outside.png"}),
        json!({"type":"led_color","color":"invalid"}),
    ] {
        let result = call(
            &client,
            "alchemy_device_command",
            json!({"device_id":"simulated","command":command}),
        )
        .await;
        assert_eq!(result.is_error, Some(true));
    }
    for command in [
        json!({"type":"background","asset_id":"anything","persistent":true}),
        json!({"type":"background_data","data":"","persistent":true}),
        json!({"type":"unknown"}),
        json!({"type":"led_brightness","value":256}),
    ] {
        let response = h
            .raw(
                "tools/call",
                json!({"name":"alchemy_device_command","arguments":{"device_id":"simulated","command":command}}),
            )
            .await;
        assert!(response.get("error").is_some() || response["result"]["isError"] == true);
    }
    for input in [
        json!({}),
        json!({"path":"x","data":"y"}),
        json!({"data":"not base64"}),
        json!({"path":"https://example.com/a.png"}),
        json!({"path":"\\\\server\\image.png"}),
        json!({"path":"\\\\?\\C:\\image.png"}),
        json!({"path":"relative.png"}),
        json!({"path":h.core.root}),
    ] {
        assert_eq!(
            call(&client, "alchemy_import_asset", input).await.is_error,
            Some(true)
        );
    }
    assert_eq!(
        call(
            &client,
            "alchemy_get_command_receipt",
            json!({"device_id":"simulated","receipt_id":"expired"})
        )
        .await
        .is_error,
        Some(true)
    );
    assert!(h.core.inner.lock().unwrap().queue.is_empty());
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn disable_cancels_only_own_pending_work_and_restores_gif_only_control() {
    let h = Harness::new(true).await;
    let client = h.client().await;
    let own = call(
        &client,
        "alchemy_device_command",
        json!({"device_id":"simulated","command":{"type":"stop_gif"}}),
    )
    .await;
    let own_id = own.structured_content.unwrap()["receipt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let other_id = h.core.enqueue(HardwareCommand::Firmware, None).unwrap();
    let before = std::fs::read(h.core.root.join("config.json")).unwrap();
    let disabled = h.manager.configure(false, h.port).await.unwrap();
    assert_eq!(disabled.status, "stopped");
    let s = h.core.snapshot();
    assert_eq!(
        s.receipts.iter().find(|r| r.id == own_id).unwrap().status,
        "cancelled"
    );
    assert_eq!(
        s.receipts.iter().find(|r| r.id == other_id).unwrap().status,
        "queued"
    );
    assert!(h.core.inner.lock().unwrap().queue.iter().any(|w| matches!(
        w.command,
        HardwareCommand::Background {
            persistent: false,
            ..
        }
    )));
    assert!(!h
        .core
        .inner
        .lock()
        .unwrap()
        .overrides
        .values()
        .any(|(owner, _)| owner == OWNER));
    assert_eq!(
        before,
        std::fs::read(h.core.root.join("config.json")).unwrap()
    );
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn configuration_restart_token_rotation_and_port_conflict() {
    let h = Harness::new(false).await;
    let old_token = h.manager.token().await.unwrap();
    h.manager.reset_token().await.unwrap();
    assert_ne!(old_token, h.manager.token().await.unwrap());
    assert_eq!(
        reqwest::Client::new()
            .post(h.url())
            .bearer_auth(&old_token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    h.manager.shutdown().await.unwrap();
    let loaded = McpManager::new(h.core.clone());
    loaded.start_saved().await;
    assert_eq!(loaded.status().await.status, "running");
    assert_eq!(
        loaded.token().await.unwrap(),
        h.manager.token().await.unwrap()
    );
    loaded.shutdown().await.unwrap();
    let conflict = TcpListener::bind(("127.0.0.1", h.port)).await.unwrap();
    let result = h.manager.configure(true, h.port).await.unwrap();
    assert_eq!(result.status, "error");
    assert!(result.error.unwrap().contains("端口"));
    drop(conflict);
    assert_eq!(
        h.manager.configure(true, h.port).await.unwrap().status,
        "running"
    );
    assert_eq!(
        h.manager.configure(false, h.port).await.unwrap().status,
        "stopped"
    );
    h.finish().await;
}

#[tokio::test]
async fn offline_unsupported_full_queue_and_disabled_handler() {
    let h = Harness::new(false).await;
    let client = h.client().await;
    assert_eq!(
        call(
            &client,
            "alchemy_device_command",
            json!({"device_id":"simulated","command":{"type":"firmware"}})
        )
        .await
        .is_error,
        Some(true)
    );
    let stopped = AlchemyServer::new(h.core.clone(), Arc::new(RwLock::new(false)));
    assert_eq!(stopped.alchemy_get_status().await.is_error, Some(true));
    client.cancel().await.unwrap();
    h.finish().await;
    let h = Harness::new(true).await;
    let client = h.client().await;
    h.core.inner.lock().unwrap().snapshot.devices[0]
        .capabilities
        .get_mut("key_gif")
        .unwrap()
        .available = false;
    assert_eq!(
        call(
            &client,
            "alchemy_device_command",
            json!({"device_id":"simulated","command":{"type":"start_gif"}})
        )
        .await
        .is_error,
        Some(true)
    );
    for _ in 0..4096 {
        h.core.enqueue(HardwareCommand::Firmware, None).unwrap();
    }
    let full = call(
        &client,
        "alchemy_device_command",
        json!({"device_id":"simulated","command":{"type":"firmware"}}),
    )
    .await;
    assert!(full.structured_content.unwrap()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("队列已满"));
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn discover_lifecycle_2026_works_without_server_sessions() {
    let h = Harness::new(false).await;
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(h.url())
            .auth_header(h.manager.token().await.unwrap()),
    );
    let config = ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("test", "1"),
    );
    let client = config
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    assert!(client
        .list_all_tools()
        .await
        .unwrap()
        .iter()
        .any(|tool| tool.name == "alchemy_get_workspace"));
    let result = client
        .call_tool(
            CallToolRequestParams::new("alchemy_get_status").with_arguments(Default::default()),
        )
        .await
        .unwrap();
    assert_eq!(
        result.structured_content.unwrap()["devices"][0]["connection"],
        "disconnected"
    );
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
async fn corrupted_config_does_not_block_core_or_overwrite_original() {
    let core = Core::new_test_device(
        std::env::temp_dir().join(format!("m18-mcp-test-{}", uuid::Uuid::new_v4())),
    )
    .unwrap();
    let data = b"{broken";
    std::fs::write(core.root.join("mcp.json"), data).unwrap();
    let manager = McpManager::new(core.clone());
    manager.start_saved().await;
    assert_eq!(manager.status().await.status, "error");
    assert_eq!(std::fs::read(core.root.join("mcp.json")).unwrap(), data);
    std::fs::remove_dir_all(&core.root).unwrap();
}

#[tokio::test]
async fn connection_close_requests_return_complete_json() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let h = Harness::new(true).await;
    let client = reqwest::Client::builder()
        .http1_only()
        // Every request here must open a fresh short connection.
        .pool_max_idle_per_host(0)
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let token = h.manager.token().await.unwrap();
    for id in 1..=5 {
        let data = serde_json::to_string(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"alchemy_get_status","arguments":{}}})).unwrap();
        let request = format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2025-11-25\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}", h.port, token, data.len(), data);
        let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", h.port))
            .await
            .unwrap();
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut data = vec![];
        let result = tokio::time::timeout(Duration::from_secs(10), socket.read_to_end(&mut data))
            .await
            .unwrap_or_else(|_|panic!("Connection: close response did not finish within ten seconds; received {} bytes: {}",data.len(),String::from_utf8_lossy(&data[..data.len().min(200)])));
        result.unwrap();
        assert!(data.starts_with(b"HTTP/1.1 200"));
    }
    for id in 1..=100 {
        let params = if id % 2 == 0 {
            json!({"name":"alchemy_device_command","arguments":{"device_id":"simulated","command":{"type":"firmware"}}})
        } else {
            json!({"name":"alchemy_get_status","arguments":{}})
        };
        let response = client
            .post(h.url())
            .bearer_auth(&token)
            .header("Connection", "close")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-11-25")
            .json(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":params}))
            .send()
            .await
            .unwrap();
        let result: Value = response.json().await.unwrap();
        assert_eq!(result["id"], id);
        assert_eq!(result["result"]["isError"], false);
    }
    h.finish().await;
}

#[tokio::test]
#[cfg(feature = "device-m18")]
async fn multi_device_http_requires_target_and_never_follows_ui_selection() {
    let h = Harness::new(false).await;
    let devices = crate::devices::DeviceManager::new(h.core.clone()).unwrap();
    let a = crate::devices::m18::sdk::device_descriptor(
        0x6603,
        0x1012,
        "a".into(),
        "V3.M18".into(),
        "a-path",
    );
    let b = crate::devices::m18::sdk::device_descriptor(
        0x6603,
        0x1012,
        "b".into(),
        "V3.M18".into(),
        "b-path",
    );
    devices.discovered(vec![a.clone()]).unwrap();
    devices.discovered(vec![a.clone(), b.clone()]).unwrap();
    for (device, path) in [(a.clone(), "a-path"), (b.clone(), "b-path")] {
        crate::devices::testing::handle(
            &h.core,
            crate::devices::testing::Event::Connected {
                device,
                path: path.into(),
            },
        )
        .await;
    }
    let client = h.client().await;
    let retry = call(
        &client,
        "alchemy_manage_device",
        json!({"edit":{"type":"retry","device_id":a.id}}),
    )
    .await;
    let retry = retry.structured_content.unwrap();
    assert_eq!(retry["requested"], true);
    assert_eq!(retry["connection_verified"], false);
    assert_eq!(
        call(
            &client,
            "alchemy_device_command",
            json!({"command":{"type":"firmware"}})
        )
        .await
        .is_error,
        Some(true)
    );
    assert_eq!(
        call(
            &client,
            "alchemy_device_command",
            json!({"device_id":"unknown","command":{"type":"firmware"}})
        )
        .await
        .is_error,
        Some(true)
    );
    let sent = call(
        &client,
        "alchemy_device_command",
        json!({"device_id":a.id,"command":{"type":"screen_brightness","value":55}}),
    )
    .await;
    assert_eq!(sent.is_error, Some(false));
    let receipt = sent.structured_content.unwrap()["receipt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(devices
        .device(&a.id)
        .unwrap()
        .snapshot()
        .receipts
        .iter()
        .any(|r| r.id == receipt));
    assert!(!devices
        .device(&b.id)
        .unwrap()
        .snapshot()
        .receipts
        .iter()
        .any(|r| r.id == receipt));
    assert_eq!(
        call(
            &client,
            "alchemy_get_command_receipt",
            json!({"device_id":b.id,"receipt_id":receipt})
        )
        .await
        .is_error,
        Some(true)
    );
    crate::devices::testing::handle(
        &h.core,
        crate::devices::testing::Event::Disconnected {
            path: "b-path".into(),
        },
    )
    .await;
    assert_eq!(
        call(
            &client,
            "alchemy_device_command",
            json!({"device_id":b.id,"command":{"type":"firmware"}})
        )
        .await
        .is_error,
        Some(true)
    );
    assert_eq!(
        call(
            &client,
            "alchemy_device_command",
            json!({"device_id":a.id,"command":{"type":"key_image","key":16,"asset_id":"bad"}})
        )
        .await
        .is_error,
        Some(true)
    );
    let status = call(&client, "alchemy_get_status", json!({})).await;
    assert_eq!(
        status.structured_content.unwrap()["devices"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    client.cancel().await.unwrap();
    h.finish().await;
}

#[tokio::test]
#[cfg(feature = "device-ms-audio-dock")]
async fn input_only_dock_cannot_launch_or_receive_display_commands_over_http() {
    use crate::devices::{
        ms_audio_dock::{Adapter, ID},
        DeviceAdapter,
    };
    let h = Harness::new(false).await;
    let devices = crate::devices::DeviceManager::new(h.core.clone()).unwrap();
    let descriptor = DeviceDescriptor {
        adapter_id: ID.into(),
        layout: Adapter::default().layout().into(),
        id: "http-dock".into(),
        model: "Audio Dock".into(),
        serial: String::new(),
        firmware: String::new(),
        vendor_id: 0x045e,
        product_id: 0x084d,
        capabilities: Default::default(),
    };
    devices
        .discovered_for(ID, vec![descriptor.clone()])
        .unwrap();
    let core = devices.device_profile("http-dock").unwrap();
    core.attach_input_device(descriptor, "dock-path".into());
    let client = h.client().await;
    for command in [
        json!({"type":"refresh"}),
        json!({"type":"launch","target":{"type":"uri","uri":"msteams:"}}),
        json!({"type":"key_data","key":1,"data":"invalid"}),
    ] {
        assert_eq!(
            call(
                &client,
                "alchemy_device_command",
                json!({"device_id":"http-dock","command":command})
            )
            .await
            .is_error,
            Some(true)
        );
    }
    assert!(core.snapshot().receipts.is_empty());
    client.cancel().await.unwrap();
    h.finish().await;
}
