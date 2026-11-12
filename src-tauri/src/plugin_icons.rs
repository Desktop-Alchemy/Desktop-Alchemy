//! Installation-owned PNG catalogs. These bytes never enter the personal asset store.
use crate::{model::*, plugin_package, storage};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::broadcast;

pub const MAX_ICONS: usize = 2048;
pub const MAX_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone)]
struct Icon {
    name: String,
    png: Arc<Vec<u8>>,
    revision: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, PluginManifest, Vec<u8>) {
        let root = std::env::temp_dir().join(format!("m18-icons-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("package/icons")).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 128]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        std::fs::write(root.join("package/icons/play.png"), &png).unwrap();
        let manifest = serde_json::from_value(serde_json::json!({"id":"example.icons","name":"图标","version":"1","api_version":1,"permissions":[],"icons":[{"id":"play","name":"播放","path":"icons/play.png"}]})).unwrap();
        (root, manifest, png)
    }
    #[test]
    fn runtime_override_is_durable_and_removal_restores_package_without_personal_assets() {
        let (root, manifest, png) = fixture();
        let store = PluginIconStore::new(&root);
        store.mount(
            &manifest.id,
            store.prepare(&manifest, &root.join("package")).unwrap(),
        );
        let reference = PluginIconRef {
            plugin_id: manifest.id.clone(),
            icon_id: "play".into(),
        };
        assert_eq!(store.resolve(&reference).unwrap().unwrap().as_slice(), png);
        store
            .publish(&manifest.id, "play", "运行时覆盖", &png)
            .unwrap();
        store.publish(&manifest.id, "new", "新图标", &png).unwrap();
        let restored = PluginIconStore::new(&root);
        restored.mount(
            &manifest.id,
            restored.prepare(&manifest, &root.join("package")).unwrap(),
        );
        assert_eq!(restored.list(&manifest.id).len(), 2);
        assert_eq!(
            restored
                .list(&manifest.id)
                .iter()
                .find(|i| i.id == "play")
                .unwrap()
                .name,
            "运行时覆盖"
        );
        assert!(restored.remove(&manifest.id, "play").unwrap());
        assert_eq!(
            restored
                .list(&manifest.id)
                .iter()
                .find(|i| i.id == "play")
                .unwrap()
                .origin,
            "package"
        );
        assert!(restored.remove(&manifest.id, "new").unwrap());
        assert!(!restored.remove(&manifest.id, "new").unwrap());
        assert!(!root.join("assets").exists());
        assert!(!root.join("library.json").exists());
        restored.unmount(&manifest.id);
        assert!(restored.resolve(&reference).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_data_limits_and_failed_index_write_leave_visible_catalog_unchanged() {
        let (root, manifest, png) = fixture();
        let store = PluginIconStore::new(&root);
        store.mount(
            &manifest.id,
            store.prepare(&manifest, &root.join("package")).unwrap(),
        );
        let revision = store.revision();
        assert!(store.publish(&manifest.id, "../bad", "bad", &png).is_err());
        assert!(store
            .publish(&manifest.id, "bad", "bad", &png[..png.len() / 2])
            .is_err());
        assert!(store.publish(&manifest.id, "PLAY", "case", &png).is_err());
        let path = root.join("plugin-icons").join(&manifest.id);
        std::fs::create_dir_all(path.join("index.json.tmp")).unwrap();
        assert!(store.publish(&manifest.id, "new", "新图标", &png).is_err());
        assert_eq!(store.revision(), revision);
        assert_eq!(store.list(&manifest.id).len(), 1);
        assert!(!path.join("index.json").exists());
        let mut definitions = manifest.icons.clone();
        definitions[0].path = "../outside.png".into();
        assert!(validate_definitions(&definitions).is_err());
        assert!(validate_definitions(&vec![manifest.icons[0].clone(); MAX_ICONS + 1]).is_err());
        let icon = Icon {
            name: "large".into(),
            png: Arc::new(vec![0; 1024 * 1024]),
            revision: "0".repeat(64),
        };
        let mut mount = Mount {
            name: "quota".into(),
            packaged: BTreeMap::new(),
            runtime: BTreeMap::new(),
        };
        for i in 0..129 {
            mount.runtime.insert(i.to_string(), icon.clone());
        }
        assert!(mount.validate().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
impl Icon {
    fn new(name: String, png: Vec<u8>) -> Result<Self> {
        validate_png(&png)?;
        Ok(Self {
            name,
            revision: format!("{:x}", Sha256::digest(&png)),
            png: Arc::new(png),
        })
    }
}
#[derive(Clone)]
pub struct Mount {
    name: String,
    packaged: BTreeMap<String, Icon>,
    runtime: BTreeMap<String, Icon>,
}
impl Mount {
    fn effective(&self) -> BTreeMap<String, (&Icon, &'static str)> {
        let mut icons: BTreeMap<_, _> = self
            .packaged
            .iter()
            .map(|(id, icon)| (id.clone(), (icon, "package")))
            .collect();
        icons.extend(
            self.runtime
                .iter()
                .map(|(id, icon)| (id.clone(), (icon, "runtime"))),
        );
        icons
    }
    fn validate(&self) -> Result<()> {
        let icons = self.effective();
        let mut ids = HashSet::new();
        if icons.len() > MAX_ICONS
            || self
                .packaged
                .values()
                .chain(self.runtime.values())
                .map(|i| i.png.len())
                .sum::<usize>()
                > MAX_BYTES
        {
            bail!(alchemy_device_api::UiFailure::new(
                "errors.iconQuota",
                "每个插件最多 2048 张图标，原始图片合计不能超过 128 MiB"
            ));
        }
        for id in icons.keys() {
            if !ids.insert(id.to_ascii_lowercase()) {
                bail!(alchemy_device_api::UiFailure::new(
                    "errors.iconCase",
                    "图标 ID 大小写冲突"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeEntry {
    name: String,
    revision: String,
}

pub fn validate_ref(icon: &PluginIconRef) -> Result<()> {
    plugin_package::validate_id(&icon.plugin_id)?;
    validate_name_id(&icon.icon_id, "图标")
}
fn validate_name_id(id: &str, name: &str) -> Result<()> {
    if !crate::widgets::identifier(id)
        || name.trim().is_empty()
        || name.trim() != name
        || name.chars().count() > 120
        || name.chars().any(char::is_control)
    {
        bail!(alchemy_device_api::UiFailure::new(
            "errors.iconIdentity",
            "图标 ID 或名称无效"
        ));
    }
    Ok(())
}
pub fn validate_definitions(icons: &[PluginIconDefinition]) -> Result<()> {
    if icons.len() > MAX_ICONS {
        bail!(alchemy_device_api::UiFailure::new(
            "errors.iconCount",
            "每个插件最多声明 2048 张图标"
        ));
    }
    let mut ids = HashSet::new();
    for icon in icons {
        validate_name_id(&icon.id, &icon.name)?;
        plugin_package::safe_path(&icon.path)?;
        if !icon.path.to_ascii_lowercase().ends_with(".png")
            || !ids.insert(icon.id.to_ascii_lowercase())
        {
            bail!(alchemy_device_api::UiFailure::new(
                "errors.iconPng",
                "图标须为 PNG，且 ID 不能重复"
            ));
        }
    }
    Ok(())
}
pub fn validate_png(png: &[u8]) -> Result<()> {
    if png.len() > 20 * 1024 * 1024 || image::guess_format(png)? != image::ImageFormat::Png {
        bail!(alchemy_device_api::UiFailure::new(
            "errors.iconSize",
            "插件图标须为不超过 20 MiB 的 PNG"
        ));
    }
    let reader =
        image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png);
    let (w, h) = reader.into_dimensions()?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 || u64::from(w) * u64::from(h) > 32_000_000 {
        bail!(alchemy_device_api::UiFailure::new(
            "errors.iconDimensions",
            "插件图标尺寸过大"
        ));
    }
    image::load_from_memory_with_format(png, image::ImageFormat::Png)?;
    Ok(())
}
fn safe_read(root: &Path, relative: &str) -> Result<Vec<u8>> {
    plugin_package::safe_path(relative)?;
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for part in Path::new(relative).components() {
        path.push(part);
        let meta = std::fs::symlink_metadata(&path)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                bail!(alchemy_device_api::UiFailure::new(
                    "errors.iconReparse",
                    "图标路径不能包含重解析点"
                ));
            }
        }
        if meta.file_type().is_symlink() {
            bail!(alchemy_device_api::UiFailure::new(
                "errors.iconLink",
                "图标路径不能包含链接"
            ));
        }
    }
    let path = path.canonicalize()?;
    if !path.starts_with(root) || std::fs::metadata(&path)?.len() > 20 * 1024 * 1024 {
        bail!(alchemy_device_api::UiFailure::new(
            "errors.iconPath",
            "图标路径或大小无效"
        ));
    }
    Ok(std::fs::read(path)?)
}

pub struct PluginIconStore {
    root: PathBuf,
    mounts: Mutex<BTreeMap<String, Mount>>,
    revision: AtomicU64,
    pub changed: broadcast::Sender<u64>,
}
impl PluginIconStore {
    pub fn new(root: &Path) -> Self {
        let (changed, _) = broadcast::channel(32);
        Self {
            root: root.to_owned(),
            mounts: Mutex::new(BTreeMap::new()),
            revision: AtomicU64::new(0),
            changed,
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }
    fn notify(&self) {
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.changed.send(revision);
    }
    pub fn prepare(&self, manifest: &PluginManifest, directory: &Path) -> Result<Mount> {
        plugin_package::validate_id(&manifest.id)?;
        validate_definitions(&manifest.icons)?;
        let mut mount = Mount {
            name: manifest.name.clone(),
            packaged: BTreeMap::new(),
            runtime: BTreeMap::new(),
        };
        for icon in &manifest.icons {
            mount.packaged.insert(
                icon.id.clone(),
                Icon::new(icon.name.clone(), safe_read(directory, &icon.path)?)?,
            );
            mount.validate()?;
        }
        let runtime = self.root.join("plugin-icons").join(&manifest.id);
        if runtime.join("index.json").exists() {
            let data = safe_read(&runtime, "index.json")?;
            if data.len() > 1024 * 1024 {
                bail!(alchemy_device_api::UiFailure::new(
                    "errors.iconIndex",
                    "插件图标索引过大"
                ));
            }
            let entries: BTreeMap<String, RuntimeEntry> = serde_json::from_slice(&data)?;
            for (id, entry) in entries {
                validate_name_id(&id, &entry.name)?;
                storage::validate_asset_id(&format!("{}.png", entry.revision))?;
                let png = safe_read(&runtime, &format!("{}.png", entry.revision))?;
                let icon = Icon::new(entry.name, png)?;
                if icon.revision != entry.revision {
                    bail!(alchemy_device_api::UiFailure::new(
                        "errors.iconChecksum",
                        "插件图标内容校验失败"
                    ));
                }
                mount.runtime.insert(id, icon);
                mount.validate()?;
            }
        }
        mount.validate()?;
        Ok(mount)
    }
    pub fn mount(&self, id: &str, mount: Mount) {
        let mut mounts = self.mounts.lock().unwrap();
        mounts.insert(id.into(), mount);
        self.notify();
    }
    pub fn unmount(&self, id: &str) {
        let mut mounts = self.mounts.lock().unwrap();
        mounts.remove(id);
        self.notify();
    }
    pub fn resolve(&self, icon: &PluginIconRef) -> Result<Option<Arc<Vec<u8>>>> {
        validate_ref(icon)?;
        Ok(self
            .mounts
            .lock()
            .unwrap()
            .get(&icon.plugin_id)
            .and_then(|m| {
                m.runtime
                    .get(&icon.icon_id)
                    .or_else(|| m.packaged.get(&icon.icon_id))
            })
            .map(|i| i.png.clone()))
    }
    pub fn list(&self, owner: &str) -> Vec<PluginIconInfo> {
        self.mounts
            .lock()
            .unwrap()
            .get(owner)
            .map(|m| {
                m.effective()
                    .into_iter()
                    .map(|(id, (icon, origin))| PluginIconInfo {
                        id,
                        name: icon.name.clone(),
                        origin: origin.into(),
                        revision: icon.revision.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn persist(&self, owner: &str, mount: &Mount) -> Result<()> {
        let dir = self.root.join("plugin-icons").join(owner);
        std::fs::create_dir_all(&dir)?;
        for path in [self.root.join("plugin-icons"), dir.clone()] {
            let meta = std::fs::symlink_metadata(path)?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    bail!(alchemy_device_api::UiFailure::new(
                        "errors.iconDirectoryReparse",
                        "插件图标目录不能包含重解析点"
                    ));
                }
            }
            if meta.file_type().is_symlink() {
                bail!(alchemy_device_api::UiFailure::new(
                    "errors.iconDirectoryLink",
                    "插件图标目录不能包含链接"
                ));
            }
        }
        for icon in mount.runtime.values() {
            let name = format!("{}.png", icon.revision);
            if std::fs::read(dir.join(&name)).ok().as_deref() != Some(icon.png.as_slice()) {
                storage::write_atomic(&dir, &name, &icon.png)?;
            }
        }
        let entries: BTreeMap<_, _> = mount
            .runtime
            .iter()
            .map(|(id, i)| {
                (
                    id,
                    RuntimeEntry {
                        name: i.name.clone(),
                        revision: i.revision.clone(),
                    },
                )
            })
            .collect();
        storage::save_json(&dir, "index.json", &entries)?;
        // Only files owned by this catalog are reclaimed, after the index is durable.
        let keep: HashSet<_> = mount
            .runtime
            .values()
            .map(|i| format!("{}.png", i.revision))
            .chain(std::iter::once("index.json".into()))
            .collect();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_file())
                    && !keep.contains(&entry.file_name().to_string_lossy().into_owned())
                {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
    pub fn publish(
        &self,
        owner: &str,
        id: &str,
        name: &str,
        data: &[u8],
    ) -> Result<PluginIconInfo> {
        validate_name_id(id, name)?;
        let icon = Icon::new(name.into(), data.to_vec())?;
        let result = PluginIconInfo {
            id: id.into(),
            name: name.into(),
            origin: "runtime".into(),
            revision: icon.revision.clone(),
        };
        let mut mounts = self.mounts.lock().unwrap();
        let mut next = mounts
            .get(owner)
            .context(alchemy_device_api::UiFailure::new(
                "errors.iconMount",
                "插件图标目录未挂载",
            ))?
            .clone();
        next.runtime.insert(id.into(), icon);
        next.validate()?;
        self.persist(owner, &next)?;
        mounts.insert(owner.into(), next);
        self.notify();
        Ok(result)
    }
    pub fn remove(&self, owner: &str, id: &str) -> Result<bool> {
        validate_name_id(id, "图标")?;
        let mut mounts = self.mounts.lock().unwrap();
        let mut next = mounts
            .get(owner)
            .context(alchemy_device_api::UiFailure::new(
                "errors.iconMount",
                "插件图标目录未挂载",
            ))?
            .clone();
        let removed = next.runtime.remove(id).is_some();
        if removed {
            self.persist(owner, &next)?;
            mounts.insert(owner.into(), next);
            self.notify();
        }
        Ok(removed)
    }
    pub fn catalog(&self, personal: AssetLibrary) -> LibraryCatalog {
        let personal_revision = Some(crate::revisions::revision(&personal));
        let mounts = self.mounts.lock().unwrap();
        let mut folders: Vec<_> = personal
            .folders
            .into_iter()
            .map(|f| CatalogFolder {
                id: f.id,
                parent_id: f.parent_id,
                name: f.name,
                plugin_id: None,
            })
            .collect();
        let mut items: Vec<_> = personal
            .items
            .into_iter()
            .map(|i| CatalogItem {
                id: i.id,
                folder_id: i.folder_id,
                name: i.name,
                revision: i.asset_id.clone(),
                image: ImageRef::Asset {
                    asset_id: i.asset_id,
                },
            })
            .collect();
        for (owner, mount) in mounts.iter() {
            let icons = mount.effective();
            if icons.is_empty() {
                continue;
            }
            let folder = format!("plugin:{owner}");
            folders.push(CatalogFolder {
                id: folder.clone(),
                parent_id: None,
                name: mount.name.clone(),
                plugin_id: Some(owner.clone()),
            });
            for (id, (icon, _)) in icons {
                items.push(CatalogItem {
                    id: format!("{folder}:{id}"),
                    folder_id: Some(folder.clone()),
                    name: icon.name.clone(),
                    revision: icon.revision.clone(),
                    image: ImageRef::PluginIcon {
                        plugin_id: owner.clone(),
                        icon_id: id,
                    },
                });
            }
        }
        LibraryCatalog {
            personal_revision,
            revision: self.revision(),
            folders,
            items,
        }
    }
}
