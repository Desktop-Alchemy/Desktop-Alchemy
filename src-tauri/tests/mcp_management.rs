//! Real local HTTP -> management -> Shell fixture. No user application or device.
use super::*;
use serde_json::{json, Value};

async fn call(
    manager: &desktop_alchemy_core::mcp::McpManager,
    url: &str,
    name: &str,
    arguments: Value,
) -> Value {
    let response:Value=reqwest::Client::new().post(url).bearer_auth(manager.token().await.unwrap())
        .header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}}))
        .send().await.unwrap().json().await.unwrap();
    assert_ne!(response["result"]["isError"], true, "{response}");
    response["result"]["structuredContent"].clone()
}
#[tokio::test]
async fn mcp_launch_job_runs_only_the_fixture_and_deduplicates_retries() {
    let (root, exe) = fixture();
    let output = root.join("mcp-output.json");
    let core = crate::support::device_core(root.join("data"));
    let manager = desktop_alchemy_core::mcp::McpManager::new(core);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    manager.configure(true, port).await.unwrap();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let arguments = json!({"request_id":"isolated-launch","operation":{"type":"launch","target":LaunchTarget::Program{path:exe.to_string_lossy().into(),arguments:format!("\"{}\" \"MCP 参数\"",output.display()),working_directory:Some(root.join("工作目录").to_string_lossy().into())}}});
    let started = call(&manager, &url, "alchemy_start_operation", arguments.clone()).await;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let state = call(
                &manager,
                &url,
                "alchemy_get_operation",
                json!({"operation_id":started["operation_id"]}),
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
    let value = report(&output);
    assert_eq!(value["arguments"], json!(["MCP 参数"]));
    assert_eq!(value["directory"], json!(root.join("工作目录")));
    std::fs::remove_file(&output).unwrap();
    let duplicate = call(&manager, &url, "alchemy_start_operation", arguments).await;
    assert_eq!(duplicate["operation_id"], started["operation_id"]);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!output.exists(), "Retry launched the fixture a second time");
    manager.shutdown().await.unwrap();
    cleanup(root);
}
