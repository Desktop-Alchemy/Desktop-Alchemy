//! Executable example: publishes installation-owned icons without touching a device.
use base64::Engine;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn send(value: Value) {
    println!("{value}");
    let _ = io::stdout().flush();
}
fn publish(id: &str, name: &str) {
    let png = include_bytes!("../../resources/key-icons/transparent/light/play.png");
    send(
        json!({"jsonrpc":"2.0","id":id,"method":"icons.publish","params":{"id":"play","name":name,"data":base64::engine::general_purpose::STANDARD.encode(png)}}),
    );
}
fn main() {
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match value["method"].as_str() {
            Some("initialize") => {
                send(json!({"jsonrpc":"2.0","id":value["id"],"result":{"apiVersion":3}}));
                publish("publish", "播放（运行时）");
            }
            Some("ping") => send(json!({"jsonrpc":"2.0","id":value["id"],"result":{"ok":true}})),
            Some("shutdown") => {
                send(json!({"jsonrpc":"2.0","id":value["id"],"result":{"ok":true}}));
                break;
            }
            _ if !value["error"].is_null() => eprintln!("图标接口调用失败：{}", value["error"]),
            _ if value["id"] == "publish" => {
                send(json!({"jsonrpc":"2.0","id":"list","method":"icons.list","params":{}}))
            }
            _ if value["id"] == "list" => send(
                json!({"jsonrpc":"2.0","id":"remove","method":"icons.remove","params":{"id":"play"}}),
            ),
            _ if value["id"] == "remove" => publish("final", "播放（可保留）"),
            _ => {}
        }
    }
}
