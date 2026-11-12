#![cfg(windows)]
#[path = "mcp_management.rs"]
mod mcp_management;
mod support;
use desktop_alchemy_core::model::LaunchTarget;
use desktop_alchemy_core::{launcher, model::KeyAction};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows::{
    core::{Interface, PCWSTR},
    Win32::{System::Com::*, UI::Shell::*},
};

fn wide(s: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn fixture() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("m18-启动测试 空格-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("工作目录")).unwrap();
    let exe = root.join("测试 程序.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_m18-test-launcher"), &exe).unwrap();
    (root, exe)
}
fn report(path: &Path) -> serde_json::Value {
    let start = Instant::now();
    loop {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                return value;
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "launch fixture did not write report"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn cleanup(root: PathBuf) {
    let start = Instant::now();
    while let Err(e) = std::fs::remove_dir_all(&root) {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "fixture cleanup: {e}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn shortcut(path: &Path, target: &Path, output: &Path, directory: &Path) {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            link.SetPath(PCWSTR(wide(target).as_ptr())).unwrap();
            let args: Vec<u16> = format!("\"{}\" 原参数", output.display())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            link.SetArguments(PCWSTR(args.as_ptr())).unwrap();
            link.SetWorkingDirectory(PCWSTR(wide(directory).as_ptr()))
                .unwrap();
            let persist: IPersistFile = link.cast().unwrap();
            persist.Save(PCWSTR(wide(path).as_ptr()), true).unwrap();
        }
        CoUninitialize();
    }
}
#[tokio::test]
async fn core_launch_preserves_unicode_quoting_and_default_directory() {
    let (root, exe) = fixture();
    let output = root.join("output.json");
    let core = crate::support::device_core(root.join("data"));
    core.execute_action(KeyAction::Launch {
        target: LaunchTarget::Program {
            path: exe.to_string_lossy().into(),
            arguments: format!("\"{}\" \"参数 空格\" &literal", output.display()),
            working_directory: None,
        },
    })
    .await
    .unwrap();
    let value = report(&output);
    assert_eq!(
        value["arguments"],
        serde_json::json!(["参数 空格", "&literal"])
    );
    assert_eq!(Path::new(value["directory"].as_str().unwrap()), root);
    let mut config = core.snapshot().config;
    config.pages[0].bindings[0].action = KeyAction::Launch {
        target: LaunchTarget::Program {
            path: root.join("已卸载.exe").to_string_lossy().into(),
            arguments: String::new(),
            working_directory: None,
        },
    };
    core.update_config(config).unwrap();
    assert!(desktop_alchemy_core::storage::load(&root.join("data")).is_ok());
    drop(core);
    cleanup(root);
}
#[test]
fn shortcut_appends_parameters_and_inherits_or_overrides_directory() {
    let (root, exe) = fixture();
    let output = root.join("output.json");
    let lnk = root.join("快捷方式.lnk");
    let dir = root.join("工作目录");
    shortcut(&lnk, &exe, &output, &dir);
    launcher::launch(lnk.to_str().unwrap(), "\"追加 参数\"", None).unwrap();
    let value = report(&output);
    assert_eq!(
        value["arguments"],
        serde_json::json!(["原参数", "追加 参数"])
    );
    assert_eq!(Path::new(value["directory"].as_str().unwrap()), dir);
    std::fs::remove_file(&output).unwrap();
    launcher::launch(
        lnk.to_str().unwrap(),
        "override",
        Some(root.to_str().unwrap()),
    )
    .unwrap();
    let value = report(&output);
    assert_eq!(
        value["arguments"],
        serde_json::json!(["原参数", "override"])
    );
    assert_eq!(Path::new(value["directory"].as_str().unwrap()), root);
    cleanup(root);
}
#[test]
fn resolves_shortcut_without_launching_and_keeps_launch_options() {
    let (root, exe) = fixture();
    let output = root.join("未启动.json");
    let lnk = root.join("快捷方式.LNK");
    let dir = root.join("工作目录");
    shortcut(&lnk, &exe, &output, &dir);
    let before = std::fs::read(&lnk).unwrap();
    let target = launcher::resolve_shortcut(lnk.to_str().unwrap()).unwrap();
    assert_eq!(Path::new(&target.path), exe);
    assert_eq!(target.arguments, format!("\"{}\" 原参数", output.display()));
    assert_eq!(Path::new(target.working_directory.as_deref().unwrap()), dir);
    assert!(!output.exists(), "resolving must not launch the target");
    assert_eq!(std::fs::read(&lnk).unwrap(), before);
    assert!(launcher::resolve_shortcut(exe.to_str().unwrap()).is_err());
    launcher::launch(
        &target.path,
        &target.arguments,
        target.working_directory.as_deref(),
    )
    .unwrap();
    assert_eq!(report(&output)["arguments"], serde_json::json!(["原参数"]));
    cleanup(root);
}
#[test]
fn rejects_missing_targets_non_program_shortcuts_and_os_launch_failure() {
    let (root, exe) = fixture();
    let output = root.join("output.json");
    assert!(launcher::launch(
        exe.to_str().unwrap(),
        "",
        Some(root.join("missing").to_str().unwrap())
    )
    .is_err());
    let broken = root.join("bad.exe");
    std::fs::write(&broken, "not an exe").unwrap();
    assert!(launcher::launch(broken.to_str().unwrap(), "", None).is_err());
    let lnk = root.join("document.lnk");
    let document = root.join("text.txt");
    std::fs::write(&document, "text").unwrap();
    shortcut(&lnk, &document, &output, &root);
    assert!(launcher::launch(lnk.to_str().unwrap(), "", None).is_err());
    assert!(launcher::resolve_shortcut(lnk.to_str().unwrap()).is_err());
    let missing_link = root.join("missing.lnk");
    shortcut(&missing_link, &root.join("missing.exe"), &output, &root);
    assert!(launcher::launch(missing_link.to_str().unwrap(), "", None).is_err());
    assert!(launcher::resolve_shortcut(missing_link.to_str().unwrap()).is_err());
    assert!(launcher::resolve_shortcut(root.join("absent.lnk").to_str().unwrap()).is_err());
    cleanup(root);
}

#[test]
fn registered_apps_include_desktop_and_store_and_preserve_shell_identity() {
    use desktop_alchemy_core::{
        application_catalog::ApplicationCatalog,
        applications::ApplicationStore,
        launch_service,
        model::{ApplicationEdit, LaunchTarget, RegisteredIdentity},
        storage,
    };
    let apps = launcher::registered_applications().unwrap();
    let mut identities = std::collections::HashSet::new();
    for app in &apps {
        assert!(!app.name.trim().is_empty());
        assert!(identities.insert(app.id.to_lowercase()));
        launch_service::validate(&app.target).unwrap();
    }
    assert!(
        apps.iter().any(|app| !app.id.contains('!')),
        "AppsFolder desktop registrations must be included"
    );
    let Some(app) = apps.first() else { return };
    let (root, _) = fixture();
    let data = root.join("registered-app-data");
    std::fs::create_dir_all(&data).unwrap();
    let mut config = desktop_alchemy_core::devices::default_config();
    config.pages[0].bindings[0].action = KeyAction::Launch {
        target: app.target.clone(),
    };
    let store = ApplicationStore::new(&data, &config);
    let item = store.snapshot().unwrap().items[0].clone();
    assert_eq!(item.target, app.target);
    let icon = apps
        .iter()
        .find_map(|app| launch_service::icon(&data, &app.target).ok())
        .expect("At least one registered app provides an icon");
    assert!(image::open(storage::asset(&data, &icon).unwrap())
        .unwrap()
        .to_rgba8()
        .pixels()
        .any(|p| p[3] > 0));
    assert!(store
        .edit(ApplicationEdit::Save {
            id: None,
            name: "重复目标".into(),
            target: app.target.clone()
        })
        .is_err());
    assert_eq!(ApplicationCatalog::list().unwrap().len(), apps.len());
    assert!(
        launch_service::execute(&LaunchTarget::RegisteredApplication {
            identity: RegisteredIdentity::AppsFolder {
                parsing_name: "DesktopAlchemy.Nonexistent.ExactIdentity".into()
            },
            arguments: String::new()
        })
        .is_err()
    );
    assert!(launcher::launch(
        "shell:AppsFolder\\M18.Nonexistent_0000000000000!App",
        "",
        None
    )
    .is_err());
    eprintln!(
        "enumerated {} complete AppsFolder registrations",
        apps.len()
    );
    cleanup(root);
}

#[test]
fn custom_protocol_passes_complete_encoded_uri_to_its_handler_and_missing_handler_fails() {
    use desktop_alchemy_core::{launch_service, model::LaunchTarget};
    use windows::Win32::System::Registry::*;
    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    struct Registration(String);
    impl Drop for Registration {
        fn drop(&mut self) {
            unsafe {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(utf16(&self.0).as_ptr()));
            }
        }
    }
    fn set(path: &str, name: &str, value: &str) {
        unsafe {
            let mut key = HKEY::default();
            RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(utf16(path).as_ptr()), &mut key)
                .ok()
                .unwrap();
            let bytes: Vec<u8> = utf16(value).iter().flat_map(|v| v.to_le_bytes()).collect();
            let result = RegSetValueExW(
                key,
                PCWSTR(utf16(name).as_ptr()),
                None,
                REG_SZ,
                Some(&bytes),
            );
            let _ = RegCloseKey(key);
            result.ok().unwrap();
        }
    }
    let (root, exe) = fixture();
    let output = root.join("uri-result.json");
    let scheme = format!("alchemy-test-{}", uuid::Uuid::new_v4());
    let path = format!("Software\\Classes\\{scheme}");
    let registration = Registration(path.clone());
    set(&path, "", "URL:Alchemy launch test");
    set(&path, "URL Protocol", "");
    set(
        &format!("{path}\\shell\\open\\command"),
        "",
        &format!("\"{}\" \"{}\" \"%1\"", exe.display(), output.display()),
    );
    let uri = format!("{scheme}://join/a%20b?name=%E4%B8%AD%E6%96%87&literal=%26%22#part");
    launch_service::execute(&LaunchTarget::Uri { uri: uri.clone() }).unwrap();
    assert_eq!(report(&output)["arguments"], serde_json::json!([uri]));
    drop(registration);
    assert!(launch_service::execute(&LaunchTarget::Uri {
        uri: format!("alchemy-missing-{}:", uuid::Uuid::new_v4())
    })
    .is_err());
    cleanup(root);
}

#[test]
fn registered_desktop_launch_uses_the_original_shell_item_and_retains_parameters() {
    use desktop_alchemy_core::{launch_service, model::LaunchTarget};
    struct StartMenuLink(PathBuf);
    impl Drop for StartMenuLink {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            unsafe {
                SHChangeNotify(
                    SHCNE_DELETE,
                    SHCNF_PATHW,
                    Some(wide(&self.0).as_ptr().cast()),
                    None,
                );
            }
        }
    }
    let (root, exe) = fixture();
    let output = root.join("registered-output.json");
    let name = format!("Alchemy launch fixture {}", uuid::Uuid::new_v4());
    let link = PathBuf::from(std::env::var_os("APPDATA").unwrap())
        .join("Microsoft/Windows/Start Menu/Programs")
        .join(format!("{name}.lnk"));
    let registration = StartMenuLink(link.clone());
    shortcut(&link, &exe, &output, &root.join("工作目录"));
    unsafe {
        SHChangeNotify(
            SHCNE_CREATE,
            SHCNF_PATHW,
            Some(wide(&link).as_ptr().cast()),
            None,
        );
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut target = loop {
        if let Some(entry) = launcher::registered_applications()
            .unwrap()
            .into_iter()
            .find(|entry| entry.name == name)
        {
            break entry.target;
        }
        assert!(
            Instant::now() < deadline,
            "Owned Start Menu fixture was not registered"
        );
        std::thread::sleep(Duration::from_millis(150));
    };
    let LaunchTarget::RegisteredApplication { arguments, .. } = &mut target else {
        panic!("not a registered target")
    };
    *arguments = "\"追加 参数\"".into();
    launch_service::execute(&target).unwrap();
    let result = report(&output);
    assert_eq!(result["arguments"][0], "原参数");
    assert_eq!(
        Path::new(result["directory"].as_str().unwrap()),
        root.join("工作目录")
    );
    assert!(
        matches!(&target,LaunchTarget::RegisteredApplication{arguments,..} if arguments=="\"追加 参数\"")
    );
    // AppsFolder providers own parameter interpretation. This Windows desktop
    // shortcut provider uses its saved arguments; direct Program targets test
    // reliable appending separately without substituting the registered identity.
    drop(registration);
    cleanup(root);
}
