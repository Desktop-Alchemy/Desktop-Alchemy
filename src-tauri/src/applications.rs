//! Versioned launch presets. The same targets are usable by every device module.
use crate::{launch_service, model::*, storage};
use anyhow::{bail, Context, Result};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Mutex,
};
pub struct ApplicationStore {
    root: PathBuf,
    state: Mutex<Result<ApplicationLibrary, String>>,
}
fn same_icon(left: &LaunchTarget, right: &LaunchTarget) -> bool {
    match (left, right) {
        (LaunchTarget::Program { path: a, .. }, LaunchTarget::Program { path: b, .. }) => {
            a.eq_ignore_ascii_case(b)
        }
        (
            LaunchTarget::RegisteredApplication { identity: a, .. },
            LaunchTarget::RegisteredApplication { identity: b, .. },
        ) => a == b,
        _ => false,
    }
}
fn signature(target: &LaunchTarget) -> Result<String> {
    let mut target = target.clone();
    match &mut target {
        LaunchTarget::Program {
            path,
            working_directory,
            ..
        } => {
            *path = path.replace('/', "\\").to_lowercase();
            if let Some(directory) = working_directory {
                *directory = directory.to_lowercase();
            }
        }
        LaunchTarget::RegisteredApplication { identity, .. } => match identity {
            RegisteredIdentity::AppsFolder { parsing_name } => {
                *parsing_name = parsing_name.to_lowercase()
            }
            RegisteredIdentity::Aumid { id } => *id = id.to_lowercase(),
        },
        LaunchTarget::Uri { .. } => {}
    }
    Ok(serde_json::to_string(&target)?)
}
pub(crate) fn validate(library: &ApplicationLibrary) -> Result<()> {
    if library.version != 2 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b006",
            "不支持的应用预设版本"
        ))
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut signatures = HashSet::new();
    for item in &library.items {
        if item.id.is_empty() || !ids.insert(&item.id) {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b007",
                "应用 ID 无效或重复"
            ))
        }
        if item.name.trim() != item.name
            || item.name.is_empty()
            || item.name.chars().count() > 120
            || item.name.chars().any(char::is_control)
        {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b008",
                "应用名称须为 1–120 字，不能包含控制字符或首尾空格"
            ))
        }
        if !names.insert(item.name.to_lowercase()) {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b009",
                "应用名称已存在，请使用不同名称"
            ))
        }
        launch_service::validate(&item.target)?;
        if !signatures.insert(signature(&item.target)?) {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b010",
                "相同的启动配置已在应用预设中"
            ))
        }
        if let Some(id) = &item.icon_asset_id {
            storage::validate_asset_id(id)?;
        }
    }
    Ok(())
}
fn read(root: &Path) -> Result<ApplicationLibrary> {
    let bytes = std::fs::read(root.join("applications.json"))?;
    let library: ApplicationLibrary = serde_json::from_slice(&bytes)?;
    validate(&library)?;
    Ok(library)
}
impl ApplicationStore {
    pub(crate) fn backup_snapshot(&self) -> Result<ApplicationLibrary> {
        self.state
            .lock()
            .unwrap()
            .clone()
            .map_err(anyhow::Error::msg)
    }
    pub fn new(root: &Path, config: &AppConfig) -> Self {
        let loaded = (|| -> Result<ApplicationLibrary> {
            if root.join("applications.json").exists() {
                return read(root);
            }
            let mut library = ApplicationLibrary {
                version: 2,
                items: vec![],
            };
            let mut signatures = HashSet::new();
            let mut configs = vec![config.clone()];
            if let Ok(bytes) = std::fs::read(root.join("config.json")) {
                if serde_json::from_slice::<serde_json::Value>(&bytes)?["format"]
                    == storage::WORKSPACE_FORMAT
                {
                    configs.extend(
                        storage::decode_workspace(&bytes)?
                            .profiles
                            .into_iter()
                            .map(|profile| profile.config),
                    );
                }
            }
            for action in configs.iter().flat_map(|config| {
                config
                    .bindings
                    .iter()
                    .chain(config.pages.iter().flat_map(|page| page.bindings.iter()))
                    .map(|binding| &binding.action)
            }) {
                let Some(target) = launch_service::action_target(action) else {
                    continue;
                };
                if !signatures.insert(signature(&target)?) {
                    continue;
                }
                let base = match &target {
                    LaunchTarget::Program { path, .. } => Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("应用")
                        .to_string(),
                    LaunchTarget::RegisteredApplication { identity, .. } => match identity {
                        RegisteredIdentity::Aumid { id } => id.clone(),
                        RegisteredIdentity::AppsFolder { parsing_name } => parsing_name.clone(),
                    },
                    LaunchTarget::Uri { uri } => uri.clone(),
                };
                let base: String = base.chars().filter(|c| !c.is_control()).take(100).collect();
                let base = if base.trim().is_empty() {
                    "应用"
                } else {
                    base.trim()
                };
                let mut name = base.to_string();
                let mut n = 2;
                while library
                    .items
                    .iter()
                    .any(|entry| entry.name.to_lowercase() == name.to_lowercase())
                {
                    name = format!("{base} ({n})");
                    n += 1;
                }
                library.items.push(ApplicationEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    target,
                    icon_asset_id: None,
                });
            }
            validate(&library)?;
            storage::save_json(root, "applications.json", &library)?;
            Ok(library)
        })()
        .map_err(|e| format!("应用预设无法加载：{e:#}"));
        Self {
            root: root.into(),
            state: Mutex::new(loaded),
        }
    }
    pub fn snapshot(&self) -> Result<ApplicationLibrary> {
        let mut state = self.state.lock().unwrap();
        if state.is_err() {
            *state = read(&self.root).map_err(|e| format!("应用预设无法加载：{e:#}"));
        }
        let mut next = state.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?.clone();
        let mut changed = false;
        for item in &mut next.items {
            if item
                .icon_asset_id
                .as_ref()
                .is_none_or(|id| storage::asset(&self.root, id).is_err())
            {
                let icon = launch_service::icon(&self.root, &item.target).ok();
                if item.icon_asset_id != icon {
                    item.icon_asset_id = icon;
                    changed = true;
                }
            }
        }
        if changed && storage::save_json(&self.root, "applications.json", &next).is_ok() {
            *state = Ok(next.clone());
        } else if changed {
            next = state.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?.clone();
            for item in &mut next.items {
                if item
                    .icon_asset_id
                    .as_ref()
                    .is_some_and(|id| storage::asset(&self.root, id).is_err())
                {
                    item.icon_asset_id = None;
                }
            }
        }
        Ok(next)
    }
    pub fn edit(&self, edit: ApplicationEdit) -> Result<ApplicationLibrary> {
        self.edit_checked(edit, None)
    }
    pub fn edit_checked(
        &self,
        edit: ApplicationEdit,
        expected: Option<&str>,
    ) -> Result<ApplicationLibrary> {
        let mut state = self.state.lock().unwrap();
        let current = state.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
        crate::revisions::check(expected, current)?;
        let mut next = current.clone();
        match edit {
            ApplicationEdit::RefreshIcon { id } => {
                let item = next.items.iter_mut().find(|item| item.id == id).context(
                    alchemy_device_api::UiFailure::new("backend.b011", "应用条目不存在"),
                )?;
                item.icon_asset_id = Some(launch_service::refresh_icon(&self.root, &item.target)?);
            }
            ApplicationEdit::Save { id, name, target } => {
                launch_service::validate(&target)?;
                let icon_asset_id = id
                    .as_ref()
                    .and_then(|id| next.items.iter().find(|item| &item.id == id))
                    .filter(|item| same_icon(&item.target, &target))
                    .and_then(|item| item.icon_asset_id.clone())
                    .filter(|id| storage::asset(&self.root, id).is_ok())
                    .or_else(|| launch_service::icon(&self.root, &target).ok());
                let item = ApplicationEntry {
                    id: id
                        .clone()
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    name,
                    target,
                    icon_asset_id,
                };
                if let Some(id) = id {
                    *next.items.iter_mut().find(|item| item.id == id).context(
                        alchemy_device_api::UiFailure::new("backend.b011", "应用条目不存在"),
                    )? = item;
                } else {
                    next.items.push(item);
                }
            }
            ApplicationEdit::Delete { id } => {
                let index = next.items.iter().position(|item| item.id == id).context(
                    alchemy_device_api::UiFailure::new("backend.b011", "应用条目不存在"),
                )?;
                next.items.remove(index);
            }
        }
        validate(&next)?;
        storage::save_json(&self.root, "applications.json", &next)?;
        *state = Ok(next.clone());
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("m18-applications-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn save(name: &str, path: &Path, arguments: &str) -> ApplicationEdit {
        ApplicationEdit::Save {
            id: None,
            name: name.into(),
            target: LaunchTarget::Program {
                path: path.to_string_lossy().into(),
                arguments: arguments.into(),
                working_directory: None,
            },
        }
    }
    #[test]
    fn migration_deduplicates_launch_presets_and_never_changes_pages() {
        let root = root();
        let mut config = crate::devices::default_config();
        let action = KeyAction::Launch {
            target: LaunchTarget::Program {
                path: root.join("编辑器.exe").to_string_lossy().into(),
                arguments: "--profile 工作".into(),
                working_directory: None,
            },
        };
        config.pages[0].bindings[0].action = action.clone();
        config.pages[0].bindings[1].action = action;
        let before = serde_json::to_vec(&config).unwrap();
        let store = ApplicationStore::new(&root, &config);
        let data = store.snapshot().unwrap();
        assert_eq!(data.items.len(), 1);
        assert_eq!(data.items[0].name, "编辑器");
        assert!(
            matches!(&data.items[0].target, LaunchTarget::Program {arguments,..} if arguments=="--profile 工作")
        );
        store
            .edit(ApplicationEdit::Delete {
                id: data.items[0].id.clone(),
            })
            .unwrap();
        assert!(ApplicationStore::new(&root, &config)
            .snapshot()
            .unwrap()
            .items
            .is_empty());
        assert_eq!(before, serde_json::to_vec(&config).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn indexes_presets_without_copying_files_and_rolls_back_failed_saves() {
        let root = root();
        let path = root.join("应用.lnk");
        std::fs::write(&path, "fixture").unwrap();
        let store = ApplicationStore::new(&root, &crate::devices::default_config());
        let first = store.edit(save("编辑器", &path, "--one")).unwrap().items[0].clone();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fixture");
        assert!(store.edit(save("重复配置", &path, "--one")).is_err());
        assert!(store.edit(save("编辑器", &path, "--two")).is_err());
        let data = store.edit(save("其他配置", &path, "--two")).unwrap();
        assert_eq!(data.items.len(), 2);
        std::fs::create_dir(root.join("applications.json.tmp")).unwrap();
        assert!(store
            .edit(ApplicationEdit::Delete {
                id: first.id.clone()
            })
            .is_err());
        assert_eq!(store.snapshot().unwrap().items.len(), 2);
        std::fs::remove_dir(root.join("applications.json.tmp")).unwrap();
        store
            .edit(ApplicationEdit::Save {
                id: Some(first.id.clone()),
                name: "新名称".into(),
                target: LaunchTarget::Program {
                    path: root.join("moved.exe").to_string_lossy().into(),
                    arguments: "--changed".into(),
                    working_directory: Some(root.to_string_lossy().into()),
                },
            })
            .unwrap();
        assert_eq!(store.snapshot().unwrap().items[0].id, first.id);
        store
            .edit(ApplicationEdit::Delete { id: first.id })
            .unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_catalog_is_isolated_preserved_and_retryable() {
        let root = root();
        std::fs::write(root.join("applications.json"), "broken").unwrap();
        let core = crate::core::Core::new_test_device(root.clone()).unwrap();
        assert!(core.applications.snapshot().is_err());
        assert!(core.library.snapshot().is_ok());
        assert_eq!(
            std::fs::read_to_string(root.join("applications.json")).unwrap(),
            "broken"
        );
        assert!(core
            .applications
            .edit(save("A", &root.join("A.exe"), ""))
            .is_err());
        storage::save_json(
            &root,
            "applications.json",
            &ApplicationLibrary {
                version: 2,
                items: vec![],
            },
        )
        .unwrap();
        assert!(core.applications.snapshot().unwrap().items.is_empty());
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unsupported_catalog_preserves_original_bytes() {
        let root = root();
        let bytes = br#"{"version":1,"items":[]}"#;
        std::fs::write(root.join("applications.json"), bytes).unwrap();
        assert!(
            ApplicationStore::new(&root, &crate::devices::default_config())
                .snapshot()
                .is_err()
        );
        assert_eq!(
            std::fs::read(root.join("applications.json")).unwrap(),
            bytes
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_v1_migration_keeps_original_bytes_and_reports_error() {
        let root = root();
        let bytes=br#"{"version":1,"items":[{"id":"keep","name":"keep","path":"relative.exe","arguments":"preserve"}]}"#;
        std::fs::write(root.join("applications.json"), bytes).unwrap();
        let store = ApplicationStore::new(&root, &crate::devices::default_config());
        assert!(store.snapshot().is_err());
        assert_eq!(
            std::fs::read(root.join("applications.json")).unwrap(),
            bytes
        );
        assert!(!root.join("applications.v1.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn absent_index_initializes_from_all_workspace_profiles_and_input_bindings() {
        let root = root();
        let mut config = crate::devices::default_config();
        config.pages[0].bindings[0].action = KeyAction::Launch {
            target: LaunchTarget::Program {
                path: root.join("profile.exe").to_string_lossy().into(),
                arguments: "--original 中文".into(),
                working_directory: None,
            },
        };
        config.bindings.push(alchemy_device_api::ControlBinding {
            control_id: "1".into(),
            trigger: alchemy_device_api::InputTrigger::Press,
            enabled: false,
            action: KeyAction::Launch {
                target: LaunchTarget::Uri {
                    uri: "msteams:".into(),
                },
            },
        });
        let mut workspace = WorkspaceConfig {
            format: storage::WORKSPACE_FORMAT.into(),
            version: 1,
            extensions: Default::default(),
            global: storage::global_from_config(&config),
            profiles: vec![DeviceProfile::new(
                crate::devices::default_adapter_id().into(),
                "test".into(),
                None,
                config,
            )],
        };
        workspace.profiles[0].device_id = Some("profile-device".into());
        storage::save_workspace(&root, &workspace).unwrap();
        let store = ApplicationStore::new(&root, &crate::devices::default_config());
        let entries = store.snapshot().unwrap().items;
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry|matches!(&entry.target,LaunchTarget::Program{arguments,..} if arguments=="--original 中文")));
        assert!(entries
            .iter()
            .any(|entry| matches!(&entry.target,LaunchTarget::Uri{uri} if uri=="msteams:")));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cached_icons_survive_source_loss_catalog_edit_and_removal() {
        let root = root();
        let source = root.join("missing.exe");
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([200, 20, 10, 128]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let icon = storage::put_asset(&root, &png.into_inner()).unwrap();
        storage::save_json(
            &root,
            "applications.json",
            &ApplicationLibrary {
                version: 2,
                items: vec![ApplicationEntry {
                    id: "app".into(),
                    name: "应用".into(),
                    target: LaunchTarget::Program {
                        path: source.to_string_lossy().into(),
                        arguments: String::new(),
                        working_directory: None,
                    },
                    icon_asset_id: Some(icon.clone()),
                }],
            },
        )
        .unwrap();
        let store = ApplicationStore::new(&root, &crate::devices::default_config());
        assert_eq!(
            store.snapshot().unwrap().items[0].icon_asset_id.as_ref(),
            Some(&icon)
        );
        let edited = store
            .edit(ApplicationEdit::Save {
                id: Some("app".into()),
                name: "改名".into(),
                target: LaunchTarget::Program {
                    path: source.to_string_lossy().into(),
                    arguments: "--changed".into(),
                    working_directory: None,
                },
            })
            .unwrap();
        assert_eq!(edited.items[0].icon_asset_id.as_ref(), Some(&icon));
        let mut config = crate::devices::default_config();
        config.pages[0].keys[0].asset_id = Some(icon.clone());
        config.pages[0].keys[0].default_icon_asset_id = Some(icon.clone());
        storage::test_save_config(&root, &config).unwrap();
        store
            .edit(ApplicationEdit::Delete { id: "app".into() })
            .unwrap();
        assert!(storage::asset(&root, &icon).is_ok());
        assert_eq!(
            storage::test_config(&root).unwrap().pages[0].keys[0].asset_id,
            Some(icon)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
