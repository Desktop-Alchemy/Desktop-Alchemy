use desktop_alchemy_core::{core::Core, model::*, plugins::PluginManager};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::task::JoinHandle;

struct Fixture {
    root: PathBuf,
    core: Arc<Core>,
    manager: Arc<PluginManager>,
    supervisor: Option<JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("m18-performance-process-{}", uuid::Uuid::new_v4()));
        let directory = root.join("plugins/performance");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(
            env!("CARGO_BIN_EXE_m18-performance-plugin"),
            directory.join("m18-performance-plugin.exe"),
        )
        .unwrap();
        std::fs::write(
            directory.join("plugin.json"),
            include_str!("../../../plugins/performance/plugin.json"),
        )
        .unwrap();
        let core = crate::support::device_core(root.join("data"));
        let manager = PluginManager::new(core.clone());
        manager.register_performance(&root).unwrap();
        Self {
            root,
            core,
            manager,
            supervisor: None,
        }
    }
    fn bind(&self) {
        let manifest = self.core.snapshot().plugins[0].manifest.clone();
        let mut pages = self.core.snapshot().config.pages;
        for (key, definition) in pages[0]
            .keys
            .iter_mut()
            .zip(manifest.widgets.into_iter().take(2))
        {
            key.widget = Some(WidgetBinding {
                source_id: None,
                source_name: None,
                presentation_id: None,
                plugin_id: manifest.id.clone(),
                widget_id: definition.id,
                style: definition.default_style,
            });
        }
        self.core.update_pages(pages).unwrap();
    }
    fn supervise(&mut self) {
        self.supervisor = Some(tokio::spawn(self.manager.clone().supervise()));
    }
    async fn wait(&self, predicate: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !predicate() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Timed out: {:?}", self.core.snapshot().plugins));
    }
    async fn stop(&mut self) {
        self.core.stop.send_replace(true);
        self.manager.stop_all().await;
        self.wait(|| {
            self.core
                .snapshot()
                .plugins
                .iter()
                .all(|p| ["stopped", "error"].contains(&p.status.as_str()))
        })
        .await;
        if let Some(task) = self.supervisor.take() {
            task.await.unwrap();
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.core.stop.send_replace(true);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
#[cfg(feature = "diagnostics")]
async fn sensor_fixture_changes_sources_and_blocked_collection_keeps_heartbeat_and_stops() {
    for mode in ["sensors_mock", "sensors_blocked", "sensors_crash"] {
        let mut f = Fixture::new();
        let directory = f.root.join("plugins/performance/sensors/publish");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(
            env!("CARGO_BIN_EXE_m18-test-plugin"),
            directory.join("m18-sensors.exe"),
        )
        .unwrap();
        std::fs::write(directory.join("mode.txt"), mode).unwrap();
        f.bind();
        f.supervise();
        f.wait(|| directory.join("sensor.pid").exists()).await;
        let pid: u32 = std::fs::read_to_string(directory.join("sensor.pid"))
            .unwrap()
            .parse()
            .unwrap();
        if mode == "sensors_mock" {
            f.wait(|| {
                f.core
                    .widget_samples()
                    .iter()
                    .any(|s| s.data.source_id.as_deref() == Some("probe-a") && s.data.valid)
            })
            .await;
            f.wait(|| {
                !f.core
                    .widget_catalog("dev.masterain.performance")
                    .sources
                    .iter()
                    .any(|s| s.id == "probe-a")
            })
            .await;
            assert!(!f
                .core
                .widget_samples()
                .iter()
                .any(|s| s.data.source_id.as_deref() == Some("probe-a")));
        } else if mode == "sensors_crash" {
            f.wait(|| {
                f.core
                    .widget_catalog("dev.masterain.performance")
                    .hardware_status
                    == "error"
            })
            .await;
        } else {
            tokio::time::sleep(Duration::from_secs(11)).await;
            assert_eq!(f.core.snapshot().plugins[0].status, "running");
            assert!(f
                .core
                .widget_samples()
                .iter()
                .any(|s| s.data.widget_id == "cpu" && s.data.valid));
        }
        f.manager
            .set_enabled("dev.masterain.performance", false)
            .await
            .unwrap();
        assert_eq!(f.core.snapshot().plugins[0].status, "stopped");
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
            let handle = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            );
            if !handle.is_null() {
                assert_eq!(WaitForSingleObject(handle, 0), 0);
                CloseHandle(handle);
            }
        }
        f.stop().await;
    }
}
#[tokio::test]
async fn discovery_leases_release_without_bindings_and_disabled_plugins_stay_stopped() {
    let mut f = Fixture::new();
    f.supervise();
    f.manager
        .discover_widgets("dev.masterain.performance", "editor-a")
        .await
        .unwrap();
    f.wait(|| f.core.snapshot().plugins[0].status == "running")
        .await;
    #[cfg(windows)]
    {
        f.wait(|| {
            f.core
                .widget_samples()
                .iter()
                .any(|s| s.data.widget_id == "battery")
        })
        .await;
        f.wait(|| {
            !f.core
                .widget_catalog("dev.masterain.performance")
                .sources
                .is_empty()
        })
        .await;
        let catalog = f.core.widget_catalog("dev.masterain.performance");
        for source in &catalog.sources {
            assert!(["disk", "network"].contains(&source.widget_id.as_str()));
            assert!(source.id.len() <= 100);
        }
        let disabled_adapter = catalog
            .sources
            .iter()
            .find(|s| s.widget_id == "network" && !s.available);
        if let Some(adapter) = disabled_adapter {
            f.wait(|| {
                f.core
                    .widget_samples()
                    .iter()
                    .any(|s| s.data.source_id.as_ref() == Some(&adapter.id))
            })
            .await;
            assert!(
                !f.core
                    .widget_samples()
                    .iter()
                    .find(|s| s.data.source_id.as_ref() == Some(&adapter.id))
                    .unwrap()
                    .data
                    .valid
            );
        }
    }
    f.manager
        .discover_widgets("dev.masterain.performance", "editor-b")
        .await
        .unwrap();
    f.manager
        .release_discovery("dev.masterain.performance", "editor-a")
        .await;
    assert_eq!(f.core.snapshot().plugins[0].status, "running");
    f.manager
        .release_discovery("dev.masterain.performance", "editor-b")
        .await;
    assert_eq!(
        f.core.snapshot().plugins[0].status,
        "stopped",
        "{:?}",
        f.core.snapshot().plugins
    );
    assert!(f
        .core
        .widget_catalog("dev.masterain.performance")
        .sources
        .is_empty());
    f.manager
        .set_enabled("dev.masterain.performance", false)
        .await
        .unwrap();
    assert!(f
        .manager
        .discover_widgets("dev.masterain.performance", "editor-a")
        .await
        .is_err());
    assert!(f.manager.enable_hardware_sensors().await.is_err());
    assert!(f
        .manager
        .running_pid("dev.masterain.performance")
        .await
        .is_none());
    f.stop().await;
}

#[tokio::test]
async fn bindings_restore_on_restart_disable_persists_and_last_unbind_stops() {
    let mut f = Fixture::new();
    f.bind();
    f.supervise();
    f.wait(|| {
        f.core
            .widget_samples()
            .iter()
            .filter(|s| ["cpu", "memory"].contains(&s.data.widget_id.as_str()))
            .count()
            == 2
    })
    .await;
    f.stop().await;
    f.core = crate::support::device_core(f.root.join("data"));
    f.manager = PluginManager::new(f.core.clone());
    f.manager.register_performance(&f.root).unwrap();
    f.supervise();
    f.wait(|| {
        f.core
            .widget_samples()
            .iter()
            .filter(|s| ["cpu", "memory"].contains(&s.data.widget_id.as_str()))
            .count()
            == 2
    })
    .await;
    f.manager
        .set_enabled("dev.masterain.performance", false)
        .await
        .unwrap();
    f.wait(|| f.core.snapshot().plugins[0].status == "stopped")
        .await;
    assert!(f.core.widget_samples().is_empty());
    f.stop().await;
    f.core = crate::support::device_core(f.root.join("data"));
    f.manager = PluginManager::new(f.core.clone());
    f.manager.register_performance(&f.root).unwrap();
    f.supervise();
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(f.core.snapshot().plugins[0].status, "stopped");
    assert!(f
        .core
        .snapshot()
        .config
        .disabled_plugins
        .contains(&"dev.masterain.performance".into()));
    f.manager
        .set_enabled("dev.masterain.performance", true)
        .await
        .unwrap();
    f.wait(|| {
        f.core
            .widget_samples()
            .iter()
            .filter(|s| ["cpu", "memory"].contains(&s.data.widget_id.as_str()))
            .count()
            == 2
    })
    .await;
    let mut pages = f.core.snapshot().config.pages;
    for key in &mut pages[0].keys {
        key.widget = None;
    }
    f.core.update_pages(pages).unwrap();
    f.wait(|| f.core.snapshot().plugins[0].status == "stopped")
        .await;
    f.stop().await;
}

#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn failed_metric_publications_are_preserved_and_crashes_do_not_loop() {
    for mode in ["widgets_invalid", "crash_after_initialize"] {
        let mut f = Fixture::new();
        let directory = f.root.join("fixture");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(
            env!("CARGO_BIN_EXE_m18-test-plugin"),
            directory.join("fixture.exe"),
        )
        .unwrap();
        std::fs::write(directory.join("mode.txt"), mode).unwrap();
        let mut manifest = f.core.snapshot().plugins[0].manifest.clone();
        manifest.id = "test.metrics".into();
        manifest.executable = Some("fixture.exe".into());
        std::fs::write(
            directory.join("plugin.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        f.manager
            .register(&directory.join("plugin.json"), false)
            .unwrap();
        let mut pages = f.core.snapshot().config.pages;
        pages[0].keys[0].widget = Some(WidgetBinding {
            source_id: None,
            source_name: None,
            presentation_id: None,
            plugin_id: manifest.id.clone(),
            widget_id: "cpu".into(),
            style: manifest.widgets[0].default_style.clone(),
        });
        f.core.update_pages(pages).unwrap();
        f.supervise();
        if mode == "widgets_invalid" {
            f.wait(|| {
                f.core
                    .widget_samples()
                    .iter()
                    .filter(|s| ["cpu", "memory"].contains(&s.data.widget_id.as_str()))
                    .count()
                    == 2
            })
            .await;
            assert!(f
                .core
                .widget_samples()
                .iter()
                .all(|s| !s.data.valid && s.data.progress.is_none()));
        } else {
            f.wait(|| f.core.snapshot().plugins[1].status == "error")
                .await;
            let generation = f
                .core
                .inner
                .lock()
                .unwrap()
                .source_epochs
                .get("test.metrics")
                .copied();
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert_eq!(
                f.core
                    .inner
                    .lock()
                    .unwrap()
                    .source_epochs
                    .get("test.metrics")
                    .copied(),
                generation
            );
            assert_eq!(f.core.snapshot().plugins[1].status, "error");
        }
        f.stop().await;
    }
}
