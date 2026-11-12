use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn send(v: Value) {
    println!("{v}");
    let _ = io::stdout().flush();
}
fn request(id: &str, method: &str, params: Value) {
    send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
}
fn main() {
    let stdin = io::stdin();
    let mut counts = std::collections::BTreeMap::<String, u64>::new();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match v["method"].as_str() {
            Some("initialize") => {
                send(json!({"jsonrpc":"2.0","id":v["id"],"result":{"apiVersion":3}}));
                request(
                    "subscribe",
                    "events.subscribe",
                    json!({"kinds":["button.down"]}),
                );
            }
            Some("ping") => send(json!({"jsonrpc":"2.0","id":v["id"],"result":{"ok":true}})),
            Some("shutdown") => {
                send(json!({"jsonrpc":"2.0","id":v["id"],"result":{"ok":true}}));
                break;
            }
            Some("event") if v["params"]["kind"] == "button.down" => {
                let device_id = v["params"]["device_id"].as_str().unwrap_or("");
                match v["params"]["key"].as_u64() {
                    Some(1) => {
                        let count = counts.entry(device_id.into()).or_default();
                        *count += 1;
                        request(
                            "visual",
                            "display.key",
                            json!({"device_id":device_id,"visual":{"key":1,"text":format!("计数 {count}"),"foreground":"#FFFFFF","background":"#5966F3","asset_id":null}}),
                        );
                    }
                    Some(2) => request(
                        &format!("pages:{device_id}"),
                        "pages.list",
                        json!({"device_id":device_id}),
                    ),
                    _ => {}
                }
            }
            _ if v["id"].as_str().is_some_and(|id| id.starts_with("pages:")) => {
                if let Some(pages) = v["result"]["pages"].as_array() {
                    let current = v["result"]["active_page"].as_str().unwrap_or("");
                    let index = pages.iter().position(|p| p["id"] == current).unwrap_or(0);
                    if !pages.is_empty() {
                        request(
                            "activate",
                            "pages.activate",
                            json!({"device_id":v["result"]["device_id"],"page_id":pages[(index+1)%pages.len()]["id"]}),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
