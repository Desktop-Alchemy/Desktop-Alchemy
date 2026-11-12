use desktop_alchemy_core::plugins::PluginManager;
use std::time::Duration;
async fn fault(mode: &str, expected: &str, timeout: u64) {
    let dir = std::env::temp_dir().join(format!("m18-fault-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_m18-test-plugin"),
        dir.join("fixture.exe"),
    )
    .unwrap();
    std::fs::write(dir.join("mode.txt"), mode).unwrap();
    std::fs::write(dir.join("plugin.json"),serde_json::json!({"id":"fault.fixture","name":"test","version":"1","api_version":3,"executable":"fixture.exe","permissions":[]}).to_string()).unwrap();
    let c = crate::support::device_core(dir.join("data"));
    let p = PluginManager::new(c.clone());
    let id = p.register(&dir.join("plugin.json"), false).unwrap();
    p.start(&id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(timeout), async {
        loop {
            if c.snapshot().plugins[0].status == "error" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    let error = c.snapshot().plugins[0].error.clone().unwrap();
    assert!(error.contains(expected), "unexpected error: {error}");
}
#[tokio::test]
async fn handshake_timeout() {
    fault("hang", "握手超时", 8).await;
}
#[tokio::test]
async fn crash_before_handshake() {
    fault("crash", "退出", 5).await;
}
#[tokio::test]
async fn wrong_protocol() {
    fault("wrong_version", "版本不符", 5).await;
}

#[tokio::test]
async fn heartbeat_timeout() {
    fault("heartbeat", "心跳请求超时", 24).await;
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return false;
        }
        let running =
            WaitForSingleObject(handle, 0) != windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        CloseHandle(handle);
        running
    }
}

#[cfg(windows)]
#[tokio::test]
async fn disable_waits_for_real_process_tree_and_cancels_handshake() {
    for mode in ["hang", "ignore_shutdown", "tree"] {
        let root = std::env::temp_dir().join(format!("m18-stop-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::copy(
            env!("CARGO_BIN_EXE_m18-test-plugin"),
            root.join("fixture.exe"),
        )
        .unwrap();
        std::fs::write(root.join("mode.txt"), mode).unwrap();
        std::fs::write(root.join("plugin.json"), serde_json::json!({"id":"test.stop","name":"Stop","version":"1","api_version":3,"executable":"fixture.exe","permissions":[]}).to_string()).unwrap();
        let core = crate::support::device_core(root.join("data"));
        let manager = PluginManager::new(core.clone());
        manager.register(&root.join("plugin.json"), false).unwrap();
        manager.start("test.stop").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root.join("parent.pid").exists()
                || (mode == "tree" && !root.join("child.tick").exists())
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let parent: u32 = std::fs::read_to_string(root.join("parent.pid"))
            .unwrap()
            .parse()
            .unwrap();
        let child = std::fs::read_to_string(root.join("child.pid"))
            .ok()
            .map(|s| s.parse::<u32>().unwrap());
        assert!(alive(parent));
        let before = std::time::Instant::now();
        manager.set_enabled("test.stop", false).await.unwrap();
        assert!(before.elapsed() < Duration::from_secs(4));
        assert!(!alive(parent));
        if let Some(child) = child {
            assert!(!alive(child));
            let tick = std::fs::read(root.join("child.tick")).unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert_eq!(tick, std::fs::read(root.join("child.tick")).unwrap());
        }
        assert_eq!(core.snapshot().plugins[0].status, "stopped");
        assert!(manager.start("test.stop").await.is_err());
        assert!(desktop_alchemy_core::storage::load(&core.root)
            .unwrap()
            .disabled_plugins
            .contains(&"test.stop".into()));
        manager.stop_all().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(windows)]
#[tokio::test]
async fn failed_disable_save_still_terminates_and_can_be_retried() {
    let root = std::env::temp_dir().join(format!("m18-stop-save-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_m18-test-plugin"),
        root.join("fixture.exe"),
    )
    .unwrap();
    std::fs::write(root.join("mode.txt"), "ignore_shutdown").unwrap();
    std::fs::write(root.join("plugin.json"), serde_json::json!({"id":"test.save","name":"Save","version":"1","api_version":3,"executable":"fixture.exe","permissions":[]}).to_string()).unwrap();
    let core = crate::support::device_core(root.join("data"));
    let manager = PluginManager::new(core.clone());
    manager.register(&root.join("plugin.json"), false).unwrap();
    manager.start("test.save").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while core.snapshot().plugins[0].status != "running" {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let pid = manager.running_pid("test.save").await.unwrap();
    std::fs::create_dir(core.root.join("config.json.tmp")).unwrap();
    assert!(manager.set_enabled("test.save", false).await.is_err());
    assert!(!alive(pid));
    assert_eq!(core.snapshot().plugins[0].status, "error");
    assert!(manager.start("test.save").await.is_err());
    std::fs::remove_dir(core.root.join("config.json.tmp")).unwrap();
    manager.set_enabled("test.save", false).await.unwrap();
    assert!(desktop_alchemy_core::storage::load(&core.root)
        .unwrap()
        .disabled_plugins
        .contains(&"test.save".into()));
    for _ in 0..3 {
        let (enable, enable_again, disable) = tokio::join!(
            manager.set_enabled("test.save", true),
            manager.set_enabled("test.save", true),
            manager.set_enabled("test.save", false),
        );
        enable.unwrap();
        enable_again.unwrap();
        disable.unwrap();
        assert!(manager.running_pid("test.save").await.is_none());
        let last_pid: u32 = std::fs::read_to_string(root.join("parent.pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(!alive(last_pid));
        assert!(core
            .snapshot()
            .config
            .disabled_plugins
            .contains(&"test.save".into()));
    }
    std::fs::remove_dir_all(root).unwrap();
}
