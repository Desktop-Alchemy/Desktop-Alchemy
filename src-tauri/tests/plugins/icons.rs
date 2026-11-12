use desktop_alchemy_core::{
    backup::{self, BackupData},
    model::*,
    plugin_package,
    plugins::PluginManager,
};
use std::path::{Path, PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("m18-icon-lifecycle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(path.join("source/icons")).unwrap();
        Self(path)
    }
    fn package(&self, version: &str, color: [u8; 4]) -> PathBuf {
        image::RgbaImage::from_pixel(8, 8, image::Rgba(color))
            .save(self.0.join("source/icons/play.png"))
            .unwrap();
        let manifest = serde_json::json!({"id":"example.icons","name":"图标包","version":version,"api_version":3,"permissions":[],"icons":[{"id":"play","name":"播放","path":"icons/play.png"}]});
        std::fs::write(self.0.join("source/plugin.json"), manifest.to_string()).unwrap();
        let target = self.0.join(format!("icons-{version}.m18plugin"));
        plugin_package::pack(&self.0.join("source"), &target).unwrap();
        target
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn install(manager: &std::sync::Arc<PluginManager>, package: &Path) {
    let summary = manager.inspect_package(package).unwrap();
    manager
        .install_package(package.to_owned(), summary.fingerprint)
        .await
        .unwrap();
}
#[tokio::test]
async fn icon_only_install_update_rollback_uninstall_reinstall_and_backup_refs() {
    let f = Fixture::new();
    let package = f.package("1", [35, 69, 103, 128]);
    let core = crate::support::device_core(f.0.join("data"));
    let manager = PluginManager::new(core.clone());
    install(&manager, &package).await;
    assert!(manager.start("example.icons").await.is_err());
    assert!(manager.set_enabled("example.icons", false).await.is_err());
    manager.stop("example.icons").await.unwrap();
    assert!(!core
        .snapshot()
        .config
        .disabled_plugins
        .contains(&"example.icons".into()));
    let reference = PluginIconRef {
        plugin_id: "example.icons".into(),
        icon_id: "play".into(),
    };
    let png = core.icons.resolve(&reference).unwrap().unwrap();
    core.icons
        .publish("example.icons", "play", "覆盖", &png)
        .unwrap();
    let mut pages = core.snapshot().config.pages;
    pages[0].keys[0].plugin_icon = Some(reference.clone());
    pages[0].keys[0].text = String::new();
    pages[0].keys[0].background = "#224466".into();
    core.update_pages(pages).unwrap();
    let assets_before = std::fs::read_dir(core.root.join("assets")).unwrap().count();
    let original = core
        .key_png(&core.snapshot().config.pages[0].keys[0])
        .unwrap();
    assert_eq!(
        assets_before,
        std::fs::read_dir(core.root.join("assets")).unwrap().count()
    );
    let restarted = crate::support::device_core(core.root.clone());
    let _restart_manager = PluginManager::new(restarted.clone());
    assert_eq!(restarted.icons.list("example.icons")[0].name, "覆盖");
    std::fs::create_dir(core.root.join("config.json.tmp")).unwrap();
    assert!(manager.remove("example.icons").await.is_err());
    assert!(core.root.join("plugins/example.icons/plugin.json").exists());
    assert!(core
        .root
        .join("plugin-icons/example.icons/index.json")
        .exists());
    assert!(core.icons.resolve(&reference).unwrap().is_some());
    std::fs::remove_dir(core.root.join("config.json.tmp")).unwrap();
    let package2 = f.package("2", [240, 20, 80, 255]);
    install(&manager, &package2).await;
    assert_eq!(core.icons.list("example.icons")[0].name, "覆盖");
    core.icons.remove("example.icons", "play").unwrap();
    assert_ne!(
        core.key_png(&core.snapshot().config.pages[0].keys[0])
            .unwrap(),
        original
    );
    manager.remove("example.icons").await.unwrap();
    assert!(!core.root.join("plugins/example.icons").exists());
    assert!(!core.root.join("plugin-icons/example.icons").exists());
    assert!(core
        .library_catalog()
        .unwrap()
        .folders
        .iter()
        .all(|folder| folder.plugin_id.is_none()));
    let saved = core.snapshot().config;
    assert_eq!(saved.pages[0].keys[0].plugin_icon, Some(reference.clone()));
    let image = image::load_from_memory(&core.key_png(&saved.pages[0].keys[0]).unwrap())
        .unwrap()
        .to_rgb8();
    assert_eq!(image.get_pixel(0, 0).0, [34, 68, 102]);
    let backup = f.0.join("settings.m18backup");
    backup::export(
        &core.root,
        &backup,
        &BackupData {
            config: desktop_alchemy_core::storage::load_workspace(&core.root).unwrap(),
            library: core.library.snapshot().unwrap(),
            applications: ApplicationLibrary {
                version: 2,
                items: vec![],
            },
            settings: AppSettings {
                model_appearances: Default::default(),
                close_behavior: CloseBehavior::Tray,
                ..Default::default()
            },
        },
    )
    .unwrap();
    backup::inspect(&core.root, &backup).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(backup).unwrap()).unwrap();
    use std::io::Read;
    let mut config = String::new();
    zip.by_name("config.json")
        .unwrap()
        .read_to_string(&mut config)
        .unwrap();
    assert!(config.contains("example.icons"));
    assert!(zip
        .file_names()
        .all(|name| !name.starts_with("plugin-icons/")));
    install(&manager, &package).await;
    assert!(core.icons.resolve(&reference).unwrap().is_some());
    assert_eq!(
        core.snapshot().config.pages[0].keys[0].plugin_icon,
        Some(reference)
    );
}

#[cfg(feature = "example-plugin")]
#[tokio::test]
async fn real_process_publishes_lists_removes_and_keeps_icons_while_disabled() {
    let f = Fixture::new();
    std::fs::copy(
        env!("CARGO_BIN_EXE_m18-icon-publisher"),
        f.0.join("source/publisher.exe"),
    )
    .unwrap();
    let manifest = serde_json::json!({"id":"example.publisher","name":"发布示例","version":"1","api_version":3,"executable":"publisher.exe","permissions":["icons.write"]});
    std::fs::write(f.0.join("source/plugin.json"), manifest.to_string()).unwrap();
    let package = f.0.join("publisher.m18plugin");
    plugin_package::pack(&f.0.join("source"), &package).unwrap();
    let core = crate::support::device_core(f.0.join("data"));
    let manager = PluginManager::new(core.clone());
    install(&manager, &package).await;
    manager
        .set_enabled("example.publisher", true)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            if core
                .icons
                .list("example.publisher")
                .iter()
                .any(|icon| icon.name == "播放（可保留）")
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    manager
        .set_enabled("example.publisher", false)
        .await
        .unwrap();
    assert_eq!(core.icons.list("example.publisher").len(), 1);
    let reopened = crate::support::device_core(core.root.clone());
    let _manager = PluginManager::new(reopened.clone());
    assert_eq!(reopened.icons.list("example.publisher").len(), 1);
    manager.remove("example.publisher").await.unwrap();
    assert!(core.icons.list("example.publisher").is_empty());
}
