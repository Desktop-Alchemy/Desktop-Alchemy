use desktop_alchemy_core::{management::Services, mcp::McpManager, plugin_package};
use serde_json::{json, Value};
use std::time::Duration;

async fn call(manager: &McpManager, url: &str, name: &str, args: Value) -> Value {
    let result:Value=reqwest::Client::new().post(url).bearer_auth(manager.token().await.unwrap())
        .header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}))
        .send().await.unwrap().json().await.unwrap();
    assert_ne!(result["result"]["isError"], true, "{result}");
    result["result"]["structuredContent"].clone()
}
async fn job(manager: &McpManager, url: &str, id: &str, operation: Value) -> Value {
    let start = call(
        manager,
        url,
        "alchemy_start_operation",
        json!({"request_id":id,"operation":operation}),
    )
    .await;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let state = call(
                manager,
                url,
                "alchemy_get_operation",
                json!({"operation_id":start["operation_id"]}),
            )
            .await;
            if state["status"] != "running" {
                break state;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(result["status"], "completed", "{result}");
    result["result"].clone()
}
#[tokio::test]
async fn mcp_installs_enables_stops_and_removes_an_isolated_real_plugin() {
    let root = std::env::temp_dir().join(format!("alchemy-mcp-process-{}", uuid::Uuid::new_v4()));
    let source = root.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_m18-example-plugin"),
        source.join("m18-example-plugin.exe"),
    )
    .unwrap();
    let mut manifest: Value =
        serde_json::from_str(include_str!("../../../examples/counter/plugin.json")).unwrap();
    manifest["id"] = json!("test.mcp-process");
    std::fs::write(
        source.join("plugin.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let package = root.join("fixture.m18plugin");
    plugin_package::pack(&source, &package).unwrap();
    let core = crate::support::device_core(root.join("data"));
    let services = Services::headless(core.clone());
    let plugins = services.plugins.clone();
    let manager = McpManager::with_services(services);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    manager.configure(true, port).await.unwrap();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let summary = job(
        &manager,
        &url,
        "inspect",
        json!({"type":"plugin_inspect","path":package}),
    )
    .await;
    let installed = job(
        &manager,
        &url,
        "install",
        json!({"type":"plugin_install","path":package,"fingerprint":summary["fingerprint"]}),
    )
    .await;
    assert_eq!(installed["enabled"], false);
    assert_eq!(plugins.running_pid("test.mcp-process").await, None);
    job(
        &manager,
        &url,
        "enable",
        json!({"type":"plugin_enable","id":"test.mcp-process","enabled":true}),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(6), async {
        while !core
            .snapshot()
            .plugins
            .iter()
            .any(|p| p.manifest.id == "test.mcp-process" && p.status == "running")
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(plugins.running_pid("test.mcp-process").await.is_some());
    job(
        &manager,
        &url,
        "stop",
        json!({"type":"plugin_stop","id":"test.mcp-process"}),
    )
    .await;
    assert_eq!(plugins.running_pid("test.mcp-process").await, None);
    job(
        &manager,
        &url,
        "disable",
        json!({"type":"plugin_enable","id":"test.mcp-process","enabled":false}),
    )
    .await;
    assert!(core
        .snapshot()
        .config
        .disabled_plugins
        .contains(&"test.mcp-process".into()));
    job(
        &manager,
        &url,
        "remove",
        json!({"type":"plugin_remove","id":"test.mcp-process"}),
    )
    .await;
    assert!(!core.root.join("plugins/test.mcp-process").exists());
    manager.shutdown().await.unwrap();
    plugins.stop_all().await;
    std::fs::remove_dir_all(root).unwrap();
}
