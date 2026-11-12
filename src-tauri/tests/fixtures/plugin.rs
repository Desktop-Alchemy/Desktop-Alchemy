// Deliberately broken subprocesses for protocol/lifecycle acceptance. Not bundled.
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn main() {
    if std::env::args().any(|a| a == "--parent") {
        sensor_fixture();
        return;
    }
    if std::env::args().any(|a| a == "--owned-child") {
        std::fs::write("child.pid", std::process::id().to_string()).unwrap();
        loop {
            std::fs::write("child.tick", format!("{:?}", std::time::SystemTime::now())).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    std::fs::write("parent.pid", std::process::id().to_string()).unwrap();
    let mode = std::fs::read_to_string("mode.txt").unwrap();
    if mode == "tree" {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.arg("--owned-child");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        // Deliberately orphaned: the host Job, not this fixture, must reap it.
        #[allow(clippy::zombie_processes)]
        let _child = command.spawn().unwrap();
    }
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let init: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    match mode.as_str() {
        "hang" => {
            std::thread::sleep(std::time::Duration::from_secs(60));
            return;
        }
        "crash" => std::process::exit(23),
        _ => {}
    }
    println!(
        "{}",
        json!({"jsonrpc":"2.0","id":init["id"],"result":{"apiVersion":if mode=="wrong_version"{99}else{3}}})
    );
    io::stdout().flush().unwrap();
    if mode == "crash_after_initialize" {
        std::process::exit(24);
    }
    if mode == "widgets_invalid" {
        for widget in ["cpu", "memory"] {
            println!(
                "{}",
                json!({"jsonrpc":"2.0","id":widget,"method":"widgets.publish","params":{"widget_id":widget,"title":widget,"value":"—","detail":null,"progress":null,"valid":false}})
            );
        }
        io::stdout().flush().unwrap();
    }
    for line in lines {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        if v["method"] == "shutdown" {
            if mode == "ignore_shutdown" || mode == "tree" {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
            return;
        }
        if v["method"] == "ping" && mode != "heartbeat" {
            println!(
                "{}",
                json!({"jsonrpc":"2.0","id":v["id"],"result":{"ok":true}})
            );
            io::stdout().flush().unwrap();
        }
    }
}

fn sensor_fixture() {
    let mode = std::fs::read_to_string("mode.txt").unwrap();
    std::fs::write("sensor.pid", std::process::id().to_string()).unwrap();
    if mode == "sensors_crash" {
        std::process::exit(31);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if tx.send(line.unwrap()).is_err() {
                return;
            }
        }
        let _ = tx.send("stop".into());
    });
    let mut count = 0;
    loop {
        if mode != "sensors_blocked" {
            let sources = if count % 2 == 0 {
                json!([{"widget_id":"temperature","id":"probe-a","name":"模拟 CPU / Package","available":true,"error":null}])
            } else {
                json!([])
            };
            let data = if count % 2 == 0 {
                json!([{"widget_id":"temperature","source_id":"probe-a","title":"CPU Package","value":"42.0°C","secondary_value":null,"detail":null,"progress":null,"valid":true,"presentations":{}}])
            } else {
                json!([])
            };
            println!(
                "{}",
                json!({"sources":sources,"data":data,"hardware_status":"ready","message":null})
            );
            io::stdout().flush().unwrap();
            count += 1;
        }
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(command) if command == "stop" => return,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            _ => {}
        }
    }
}
