//! Versioned, bounded ZIP backups. Restores run before Core/SDK startup.
use crate::{model::*, storage};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const FORMAT: &str = "desktop-alchemy.platform.backup";
const METADATA: [&str; 4] = [
    "config.json",
    "library.json",
    "applications.json",
    "app-settings.json",
];
const MAX_FILES: usize = 10_000;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
const MAX_JSON: u64 = 16 * 1024 * 1024;
const MAX_IMAGE: u64 = 20 * 1024 * 1024;
const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
const PENDING: &str = "backup-pending";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    size: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    format_version: u32,
    app_version: String,
    created_at: f64,
    files: BTreeMap<String, Entry>,
}

/// Only durable user data; credentials, logs and caches never enter the archive.
pub struct BackupData {
    pub config: WorkspaceConfig,
    pub library: AssetLibrary,
    pub applications: ApplicationLibrary,
    pub settings: AppSettings,
}
impl BackupData {
    fn validate(&self, root: &Path) -> Result<()> {
        storage::validate_workspace(&self.config)?;
        let adapters = crate::devices::built_in_adapters();
        for profile in &self.config.profiles {
            if let Some(adapter) = adapters.get(&profile.adapter_id) {
                if profile.schema_version == adapter.config_version()
                    && profile.config_version == 1
                    && profile.config_error.is_none()
                {
                    storage::validate_device_config(
                        &profile.config,
                        &adapter.definition().normalized(),
                    )?;
                    adapter.validate(&profile.config)?;
                    adapter.validate_module(&profile.module_data)?;
                    for id in adapter.asset_references(&profile.module_data)? {
                        storage::asset(root, &id)?;
                    }
                }
            }
        }
        crate::library::validate(&self.library)?;
        crate::applications::validate(&self.applications)?;
        storage::validate_app_settings(&self.settings)?;
        for id in self
            .config
            .profiles
            .iter()
            .flat_map(|profile| profile.config.pages.iter())
            .flat_map(|p| {
                p.background_asset.iter().chain(
                    p.keys
                        .iter()
                        .flat_map(|k| k.asset_id.iter().chain(k.default_icon_asset_id.iter())),
                )
            })
            .chain(self.library.items.iter().map(|i| &i.asset_id))
            .chain(
                self.applications
                    .items
                    .iter()
                    .filter_map(|i| i.icon_asset_id.as_ref()),
            )
            .chain(self.settings.model_appearances.values().filter_map(
                |appearance| match appearance {
                    ModelAppearance::Image {
                        image: ImageRef::Asset { asset_id },
                    } => Some(asset_id),
                    _ => None,
                },
            ))
        {
            storage::asset(root, id).with_context(|| format!("备份缺少引用的图片：{id}"))?;
        }
        Ok(())
    }
    fn read(root: &Path) -> Result<Self> {
        Ok(Self {
            config: storage::decode_workspace(&std::fs::read(root.join(METADATA[0]))?)?,
            library: read_json(&root.join(METADATA[1]))?,
            applications: read_json(&root.join(METADATA[2]))?,
            settings: read_json(&root.join(METADATA[3]))?,
        })
    }
    fn json(&self) -> Result<[Vec<u8>; 4]> {
        Ok([
            serde_json::to_vec_pretty(&self.config)?,
            serde_json::to_vec_pretty(&self.library)?,
            serde_json::to_vec_pretty(&self.applications)?,
            serde_json::to_vec_pretty(&self.settings)?,
        ])
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&read_limited(
        File::open(path)?,
        MAX_JSON,
    )?)?)
}
fn read_limited(reader: impl Read, max: u64) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    reader.take(max + 1).read_to_end(&mut data)?;
    if data.len() as u64 > max {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b017",
            "备份文件超过大小限制"
        ))
    }
    Ok(data)
}
fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn fingerprint(file: &mut File) -> Result<String> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    std::io::copy(file, &mut hash)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(format!("{:x}", hash.finalize()))
}
fn entry_limit(name: &str) -> Result<u64> {
    if METADATA.contains(&name) {
        return Ok(MAX_JSON);
    }
    if let Some(id) = name.strip_prefix("assets/") {
        storage::validate_asset_id(id)?;
        return Ok(MAX_IMAGE);
    }
    bail!(UiFailure::new(
        "errors.backupPath",
        format!("备份包含不支持的文件或路径：{name}")
    )
    .arg("name", name))
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(root: &Path) -> Result<Self> {
        let path = root.join(format!("backup-stage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(path.join("assets"))?;
        Ok(Self(path))
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_entry(
    zip: &mut ZipWriter<File>,
    manifest: &mut Manifest,
    name: &str,
    bytes: &[u8],
) -> Result<()> {
    let limit = entry_limit(name)?;
    if bytes.len() as u64 > limit
        || manifest.files.len() >= MAX_FILES
        || manifest.files.values().map(|e| e.size).sum::<u64>() + bytes.len() as u64 > MAX_TOTAL
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b018",
            "备份超过文件数量或体积限制"
        ))
    }
    zip.start_file(
        name,
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )?;
    zip.write_all(bytes)?;
    manifest.files.insert(
        name.into(),
        Entry {
            size: bytes.len() as u64,
            sha256: digest(bytes),
        },
    );
    Ok(())
}

/// Build beside the destination, sync, then atomically replace the chosen file.
pub fn export(root: &Path, target: &Path, data: &BackupData) -> Result<()> {
    data.validate(root)?;
    let parent = target
        .parent()
        .context(alchemy_device_api::UiFailure::new(
            "backend.b019",
            "备份保存目录无效",
        ))?
        .canonicalize()?;
    // Prevent choosing a live data file (including a materialized asset) as output.
    if parent == root.canonicalize()? || parent.starts_with(root.join("assets")) {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b020",
            "请将备份保存到应用数据目录之外或 backups 子目录"
        ))
    }
    let temp_name = format!(".m18backup-{}.tmp", uuid::Uuid::new_v4());
    let temp = parent.join(&temp_name);
    let result = (|| -> Result<()> {
        let mut zip = ZipWriter::new(File::create(&temp)?);
        let mut manifest = Manifest {
            format: FORMAT.into(),
            format_version: 1,
            app_version: env!("CARGO_PKG_VERSION").into(),
            created_at: now(),
            files: BTreeMap::new(),
        };
        for (name, bytes) in METADATA.iter().zip(data.json()?) {
            write_entry(&mut zip, &mut manifest, name, &bytes)?;
        }
        let mut assets = std::fs::read_dir(root.join("assets"))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        assets.sort();
        for path in assets {
            let id = path.file_name().and_then(|s| s.to_str()).context(
                alchemy_device_api::UiFailure::new("backend.b021", "素材名称无效"),
            )?;
            if id
                .strip_suffix(".tmp")
                .is_some_and(|id| storage::validate_asset_id(id).is_ok())
            {
                continue; // An interrupted atomic write, not an imported image.
            }
            if !path.is_file() {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b022",
                    "素材目录包含非文件条目"
                ))
            }
            storage::validate_asset_id(id)?;
            let bytes = read_limited(File::open(&path)?, MAX_IMAGE)?;
            if !id.starts_with(&digest(&bytes)) {
                bail!(
                    UiFailure::new("errors.assetChecksum", format!("素材校验失败：{id}"))
                        .arg("id", id)
                )
            }
            write_entry(&mut zip, &mut manifest, &format!("assets/{id}"), &bytes)?;
        }
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        if bytes.len() as u64 > MAX_MANIFEST {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b023",
                "备份清单过大"
            ))
        }
        zip.start_file("manifest.json", SimpleFileOptions::default())?;
        zip.write_all(&bytes)?;
        zip.finish()?.sync_all()?;
        storage::replace_file(&temp, target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

/// Extract only explicitly named files, never ZipArchive::extract on user input.
fn unpack(root: &Path, path: &Path) -> Result<(TempDir, BackupSummary)> {
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Keep the bytes fingerprinted and the bytes decoded identical, even if
        // another program attempts to replace the source while it is being read.
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut file = options
        .open(path)
        .context(alchemy_device_api::UiFailure::new(
            "backend.b024",
            "无法打开备份文件（文件可能正被其他程序写入）",
        ))?;
    if file.metadata()?.len() > MAX_TOTAL + 64 * 1024 * 1024 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b025",
            "备份包过大"
        ))
    }
    let fingerprint = fingerprint(&mut file)?;
    let mut zip = ZipArchive::new(file).context(alchemy_device_api::UiFailure::new(
        "backend.b026",
        "不是有效的 M18 备份压缩包",
    ))?;
    if zip.len() > MAX_FILES + 1 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b027",
            "备份文件数量过多"
        ))
    }
    let mut names = BTreeSet::new();
    for index in 0..zip.len() {
        let f = zip.by_index(index)?;
        let name = f.name();
        if !names.insert(name.to_owned()) {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b028",
                "备份存在重名文件"
            ))
        }
        let max = if name == "manifest.json" {
            MAX_MANIFEST
        } else {
            entry_limit(name)?
        };
        if f.size() > max || f.is_dir() || f.is_symlink() || f.encrypted() {
            bail!(
                UiFailure::new("errors.backupEntry", format!("备份条目无效或过大：{name}"))
                    .arg("name", name)
            )
        }
    }
    let manifest: Manifest =
        serde_json::from_slice(&read_limited(zip.by_name("manifest.json")?, MAX_MANIFEST)?)
            .context(alchemy_device_api::UiFailure::new(
                "backend.b029",
                "M18 备份清单损坏或缺失",
            ))?;
    if manifest.format != FORMAT || manifest.format_version != 1 {
        bail!(UiFailure::new(
            "errors.backupVersion",
            format!("不支持的备份格式或版本：{}", manifest.format_version)
        )
        .arg("version", manifest.format_version))
    }
    if !manifest.created_at.is_finite() || manifest.created_at < 0.0 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b030",
            "备份时间无效"
        ))
    }
    if manifest
        .files
        .keys()
        .cloned()
        .chain(std::iter::once("manifest.json".into()))
        .collect::<BTreeSet<_>>()
        != names
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b031",
            "备份清单与文件不一致"
        ))
    }
    for name in METADATA {
        if !manifest.files.contains_key(name) {
            bail!(
                UiFailure::new("errors.backupMissing", format!("备份缺少 {name}"))
                    .arg("name", name)
            )
        }
    }
    let total = manifest
        .files
        .values()
        .try_fold(0_u64, |total, e| total.checked_add(e.size))
        .context(alchemy_device_api::UiFailure::new(
            "backend.b032",
            "备份体积无效",
        ))?;
    if total > MAX_TOTAL {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b033",
            "备份解压后超过 2 GiB"
        ))
    }
    let stage = TempDir::new(root)?;
    for (name, entry) in &manifest.files {
        let limit = entry_limit(name)?;
        if entry.size > limit {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b034",
                "备份条目过大"
            ))
        }
        let bytes = read_limited(zip.by_name(name)?, limit)?;
        if bytes.len() as u64 != entry.size || digest(&bytes) != entry.sha256 {
            bail!(
                UiFailure::new("errors.backupChecksum", format!("备份文件校验失败：{name}"))
                    .arg("name", name)
            )
        }
        if let Some(id) = name.strip_prefix("assets/") {
            if storage::put_asset(&stage.0, &bytes)? != id {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b035",
                    "图片内容与素材 ID 不匹配"
                ))
            }
        } else {
            std::fs::write(stage.0.join(name), bytes)?;
        }
    }
    let data = BackupData::read(&stage.0)?;
    data.validate(&stage.0)?;
    std::fs::write(
        stage.0.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let summary = BackupSummary {
        format_version: manifest.format_version,
        app_version: manifest.app_version,
        created_at: manifest.created_at,
        pages: data
            .config
            .profiles
            .iter()
            .map(|p| p.config.pages.len() as u32)
            .sum(),
        scenes: data
            .config
            .profiles
            .iter()
            .map(|p| p.config.scene_settings.scenes.len() as u32)
            .sum(),
        images: (manifest.files.len() - METADATA.len()) as u32,
        folders: data.library.folders.len() as u32,
        library_items: data.library.items.len() as u32,
        applications: data.applications.items.len() as u32,
        plugins: data.config.global.managed_plugin_ids.len() as u32,
        total_bytes: total as f64,
        fingerprint,
    };
    Ok((stage, summary))
}

pub fn inspect(root: &Path, path: &Path) -> Result<BackupSummary> {
    Ok(unpack(root, path)?.1)
}

/// The reviewed archive must still be byte-for-byte identical at confirmation.
pub fn stage(root: &Path, path: &Path, expected: &str) -> Result<()> {
    if root.join(PENDING).exists() {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b036",
            "已有待恢复的备份，请重启客户端"
        ))
    }
    let (stage, summary) = unpack(root, path)?;
    if summary.fingerprint != expected {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b037",
            "备份文件已改变，请重新选择并确认"
        ))
    }
    // Flush every staged file before publishing the pending directory.
    for name in METADATA.into_iter().chain(std::iter::once("manifest.json")) {
        File::options()
            .write(true)
            .open(stage.0.join(name))?
            .sync_all()?;
    }
    for entry in std::fs::read_dir(stage.0.join("assets"))? {
        File::options()
            .write(true)
            .open(entry?.path())?
            .sync_all()?;
    }
    std::fs::rename(&stage.0, root.join(PENDING))?;
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
struct Journal {
    absent: Vec<String>,
    #[serde(default)]
    replaced_assets: Vec<String>,
}

fn copy_atomic(source: &Path, directory: &Path, name: &str) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let temp = directory.join(format!("{name}.tmp"));
    let result = (|| -> Result<()> {
        let mut output = File::create(&temp)?;
        std::io::copy(&mut File::open(source)?, &mut output)?;
        output.sync_all()?;
        drop(output);
        storage::replace_file(&temp, &directory.join(name))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn same_file_bytes(left: &Path, right: &Path) -> Result<bool> {
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    Ok(left.metadata()?.len() == right.metadata()?.len()
        && fingerprint(&mut left)? == fingerprint(&mut right)?)
}

fn rollback(root: &Path, pending: &Path, journal: &Journal) -> Result<()> {
    for id in &journal.replaced_assets {
        storage::validate_asset_id(id)?;
        let original = pending.join("undo/assets").join(id);
        if !same_file_bytes(&original, &root.join("assets").join(id)).unwrap_or(false) {
            copy_atomic(&original, &root.join("assets"), id)?;
        }
    }
    for name in METADATA {
        if journal.absent.iter().any(|n| n == name) {
            match std::fs::remove_file(root.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        } else {
            let original = std::fs::read(pending.join("undo").join(name))?;
            if std::fs::read(root.join(name)).ok().as_ref() != Some(&original) {
                storage::write_atomic(root, name, &original)?;
            }
        }
    }
    Ok(())
}

fn retire_pending(root: &Path, pending: &Path) -> Result<()> {
    // Remove the transaction name atomically BEFORE recursive cleanup. Otherwise a
    // crash while deleting could remove the commit marker but leave the journal.
    let retired = root.join(format!("backup-cleanup-{}", uuid::Uuid::new_v4()));
    std::fs::rename(pending, &retired)?;
    let _ = std::fs::remove_dir_all(retired);
    Ok(())
}

/// Called once, before any in-memory settings, plugins, HTTP server or SDK exists.
pub fn apply_pending(root: &Path) -> Result<Option<BackupRestoreNotice>> {
    match apply_pending_inner(root) {
        Ok(note) => Ok(note),
        Err(error) if !root.join(PENDING).join("journal.json").exists() => {
            let rejected = root.join(format!("backup-rejected-{}", uuid::Uuid::new_v4()));
            if root.join(PENDING).exists() {
                std::fs::rename(root.join(PENDING), &rejected)?;
            }
            Ok(Some(BackupRestoreNotice {
                ui_message: Some(UiMessage::new("errors.restoreFailed")),
                restored: false,
                message: format!(
                    "备份未恢复，原配置保留：{error:#}；恢复文件：{}",
                    rejected.display()
                ),
            }))
        }
        Err(error) => Err(error),
    }
}

fn apply_pending_inner(root: &Path) -> Result<Option<BackupRestoreNotice>> {
    let pending = root.join(PENDING);
    if !pending.exists() {
        return Ok(None);
    }
    if pending.join("committed.json").exists() {
        let _ = retire_pending(root, &pending);
        return Ok(Some(BackupRestoreNotice {
            ui_message: Some(UiMessage::new("app.backupRestored")),
            restored: true,
            message: "备份已恢复".into(),
        }));
    }
    if pending.join("journal.json").exists() {
        let journal = read_json(&pending.join("journal.json"))?;
        rollback(root, &pending, &journal).context(alchemy_device_api::UiFailure::new(
            "backend.b038",
            "恢复中断后的回滚失败，已保留恢复文件",
        ))?;
        retire_pending(root, &pending)?;
        return Ok(Some(BackupRestoreNotice {
            ui_message: Some(UiMessage::new("errors.restoreFailed")),
            restored: false,
            message: "上次导入中断，已恢复导入前的配置；请重新导入备份".into(),
        }));
    }
    // Revalidate staged bytes after restart (including checksums and references).
    let manifest: Manifest = read_json(&pending.join("manifest.json"))?;
    if manifest.format != FORMAT || manifest.format_version != 1 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b039",
            "不支持的待恢复备份版本"
        ))
    }
    for name in METADATA {
        if !manifest.files.contains_key(name) {
            bail!(
                UiFailure::new("errors.pendingMissing", format!("待恢复备份缺少 {name}"))
                    .arg("name", name)
            )
        }
    }
    let data = BackupData::read(&pending)?;
    data.validate(&pending)?;
    for (name, entry) in &manifest.files {
        let limit = entry_limit(name)?;
        let bytes = read_limited(File::open(pending.join(name))?, limit)?;
        if bytes.len() as u64 != entry.size || digest(&bytes) != entry.sha256 {
            bail!(UiFailure::new(
                "errors.pendingChecksum",
                format!("待恢复备份校验失败：{name}")
            )
            .arg("name", name))
        }
    }
    let backups = root.join("backups");
    std::fs::create_dir_all(&backups)?;
    let recovery = backups.join(format!("before-import-{}.m18backup", uuid::Uuid::new_v4()));
    // Preserve the original files even if an old metadata file is damaged.
    let undo = pending.join("undo");
    std::fs::create_dir_all(&undo)?;
    let mut journal = Journal::default();
    for name in METADATA {
        if root.join(name).exists() {
            let bytes = std::fs::read(root.join(name))?;
            storage::write_atomic(&undo, name, &bytes)?;
        } else {
            journal.absent.push(name.into());
        }
    }
    // Content-addressed assets normally already match. Keep exact original bytes
    // before repairing a corrupt local file, including files larger than our
    // import limit; stream the originals instead of loading them into memory.
    for (name, entry) in manifest
        .files
        .iter()
        .filter(|(n, _)| n.starts_with("assets/"))
    {
        let target = root.join(name);
        if target.exists() {
            let matches = read_limited(File::open(&target)?, MAX_IMAGE)
                .is_ok_and(|bytes| digest(&bytes) == entry.sha256);
            if !matches {
                let id = name.strip_prefix("assets/").unwrap();
                copy_atomic(&target, &undo.join("assets"), id)?;
                journal.replaced_assets.push(id.into());
            }
        }
    }
    // A normal, portable recovery archive when current metadata is healthy.
    // Otherwise retain the raw undo directory so damaged originals are not lost.
    let previous = (|| -> Result<BackupData> {
        Ok(BackupData {
            config: if root.join("config.json").exists() {
                storage::load_workspace(root)?
            } else {
                WorkspaceConfig {
                    format: crate::storage::WORKSPACE_FORMAT.into(),
                    extensions: Default::default(),
                    version: 1,
                    global: GlobalConfig::default(),
                    profiles: vec![],
                }
            },
            library: read_json(&root.join("library.json"))?,
            applications: read_json(&root.join("applications.json"))?,
            settings: if root.join("app-settings.json").exists() {
                read_json(&root.join("app-settings.json"))?
            } else {
                AppSettings::default()
            },
        })
    })();
    let recovery_note = match previous.and_then(|old| export(root, &recovery, &old)) {
        Ok(()) => format!("恢复前备份：{}", recovery.display()),
        Err(_) => {
            let raw = backups.join(format!("before-import-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&raw)?;
            for name in METADATA {
                if undo.join(name).exists() {
                    storage::write_atomic(&raw, name, &std::fs::read(undo.join(name))?)?;
                }
            }
            for id in &journal.replaced_assets {
                copy_atomic(&undo.join("assets").join(id), &raw.join("assets"), id)?;
            }
            storage::save_json(&raw, "original-files.json", &journal)?;
            format!("原始配置保留于：{}", raw.display())
        }
    };
    storage::save_json(&pending, "journal.json", &journal)?;
    let result = (|| -> Result<()> {
        std::fs::create_dir_all(root.join("assets"))?;
        for name in manifest.files.keys().filter(|n| n.starts_with("assets/")) {
            let target = root.join(name);
            let id = name.strip_prefix("assets/").unwrap();
            if !target.exists() || journal.replaced_assets.iter().any(|old| old == id) {
                copy_atomic(&pending.join(name), &root.join("assets"), id)?;
            }
        }
        for (name, bytes) in METADATA.iter().zip(data.json()?) {
            storage::write_atomic(root, name, &bytes)?;
        }
        storage::save_json(&pending, "committed.json", &true)?;
        Ok(())
    })();
    if let Err(error) = result {
        rollback(root, &pending, &journal).context(alchemy_device_api::UiFailure::new(
            "backend.b040",
            "备份恢复失败且回滚未完成，已保留恢复文件",
        ))?;
        retire_pending(root, &pending)?;
        return Ok(Some(BackupRestoreNotice {
            ui_message: Some(UiMessage::new("errors.restoreFailed")),
            restored: false,
            message: format!("导入失败，已恢复原配置：{error:#}；{recovery_note}"),
        }));
    }
    // Leaving a committed transaction after cleanup failure is safe on next startup.
    let _ = retire_pending(root, &pending);
    Ok(Some(BackupRestoreNotice {
        ui_message: Some(UiMessage::new("app.backupRestored")),
        restored: true,
        message: format!("备份已恢复；{recovery_note}"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("m18-backup-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn root(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            std::fs::create_dir_all(p.join("assets")).unwrap();
            p
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn data(root: &Path) -> BackupData {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let id = storage::put_asset(root, png.get_ref()).unwrap();
        // Unreferenced original imports are also retained.
        let mut other = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(3, 3)
            .write_to(&mut other, image::ImageFormat::Png)
            .unwrap();
        storage::put_asset(root, other.get_ref()).unwrap();
        let mut config = crate::devices::default_config();
        let mut second = config.pages[0].clone();
        second.id = "second".into();
        second.name = "工作".into();
        config.pages.push(second);
        config.pages[0].bindings[0].action = KeyAction::Page {
            page_id: "second".into(),
        };
        config.pages[1].bindings[1].action = KeyAction::Launch {
            target: LaunchTarget::Program {
                path: "C:\\missing\\app.lnk".into(),
                arguments: "--中文 test".into(),
                working_directory: Some("C:\\work".into()),
            },
        };
        config.pages[0].keys[0].asset_id = Some(id.clone());
        config.pages[0].keys[0].default_icon_asset_id = Some(id.clone());
        config.pages[0].keys[0].widget = Some(WidgetBinding {
            source_id: Some("volume-test".into()),
            source_name: Some("C: · System".into()),
            presentation_id: Some("available".into()),
            plugin_id: "dev.masterain.performance".into(),
            widget_id: "disk".into(),
            style: WidgetStyle {
                foreground: "#FFFFFF".into(),
                background: "#101820".into(),
                accent: "#34D399".into(),
                show_progress: true,
            },
        });
        config
            .disabled_plugins
            .push("dev.masterain.performance".into());
        config.pages[1].background_asset = Some(id.clone());
        config.scene_settings = SceneSettings {
            enabled: true,
            paused: true,
            scenes: vec![Scene {
                id: "scene".into(),
                name: "测试场景".into(),
                enabled: true,
                page_id: "second".into(),
                trigger: SceneTrigger::Continuous,
                condition: SceneCondition::MediaPlaying,
                enter_delay_ms: 300,
                exit_delay_ms: 1000,
            }],
            ..Default::default()
        };
        config.theme = "dark".into();
        config.managed_plugin_ids.push("example.fixture".into());
        BackupData {
            config: WorkspaceConfig {
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
            },
            library: AssetLibrary {
                version: 1,
                folders: vec![LibraryFolder {
                    id: "folder".into(),
                    parent_id: None,
                    name: "图片".into(),
                }],
                items: vec![LibraryItem {
                    id: "item".into(),
                    folder_id: Some("folder".into()),
                    name: "测试图片".into(),
                    asset_id: id.clone(),
                }],
            },
            applications: ApplicationLibrary {
                version: 2,
                items: vec![ApplicationEntry {
                    id: "app".into(),
                    name: "快捷方式".into(),
                    target: LaunchTarget::Program {
                        path: "C:\\missing\\app.lnk".into(),
                        arguments: "--中文 test".into(),
                        working_directory: Some("C:\\work".into()),
                    },
                    icon_asset_id: Some(id),
                }],
            },
            settings: AppSettings {
                language: LanguagePreference::English,
                close_behavior: CloseBehavior::Exit,
                ..Default::default()
            },
        }
    }
    fn save(root: &Path, data: &BackupData) {
        for (name, bytes) in METADATA.iter().zip(data.json().unwrap()) {
            storage::write_atomic(root, name, &bytes).unwrap();
        }
    }
    fn rewrite(path: &Path, edit: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) {
        let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
        let mut entries = BTreeMap::new();
        for index in 0..zip.len() {
            let mut f = zip.by_index(index).unwrap();
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            entries.insert(f.name().to_string(), bytes);
        }
        drop(zip);
        edit(&mut entries);
        let mut writer = ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in entries {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    fn refresh_manifest(entries: &mut BTreeMap<String, Vec<u8>>) {
        let mut m: Manifest = serde_json::from_slice(&entries["manifest.json"]).unwrap();
        for (name, entry) in &mut m.files {
            if let Some(bytes) = entries.get(name) {
                entry.size = bytes.len() as u64;
                entry.sha256 = digest(bytes);
            }
        }
        entries.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
    }

    #[test]
    fn rejects_old_backup_and_exports_all_device_profiles() {
        let fixture = Fixture::new();
        let root = fixture.root("source");
        let mut original = data(&root);
        let archive = fixture.0.join("old.m18backup");
        export(&root, &archive, &original).unwrap();
        rewrite(&archive, |entries| {
            entries.insert(
                "config.json".into(),
                serde_json::to_vec(&original.config.profiles[0].config).unwrap(),
            );
            refresh_manifest(entries);
            let mut manifest: Manifest = serde_json::from_slice(&entries["manifest.json"]).unwrap();
            manifest.format_version = 1;
            entries.insert(
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            );
        });
        assert!(inspect(&root, &archive).is_err());
        let mut second = original.config.profiles[0].clone();
        second.id = "other".into();
        second.device_id = Some("other-device".into());
        original.config.profiles.push(second);
        export(&root, &archive, &original).unwrap();
        assert_eq!(inspect(&root, &archive).unwrap().pages, 4);
        rewrite(&archive, |entries| {
            let mut manifest: Manifest = serde_json::from_slice(&entries["manifest.json"]).unwrap();
            manifest.format_version = 2;
            entries.insert(
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            );
        });
        assert!(inspect(&root, &archive).is_err());
    }

    #[test]
    #[cfg(feature = "device-ms-audio-dock")]
    fn platform_restore_preserves_input_bindings_and_typed_targets_without_placeholder_pages() {
        use crate::devices::{
            ms_audio_dock::{Adapter, ID},
            DeviceAdapter,
        };
        let fixture = Fixture::new();
        let source = fixture.root("dock-source");
        let dest = fixture.root("dock-dest");
        let mut original = data(&source);
        let mut config = Adapter::default().default_config();
        config.bindings.get_mut(0).unwrap().enabled = true;
        config.bindings.get_mut(0).unwrap().action = KeyAction::Launch {
            target: LaunchTarget::Uri {
                uri: "msteams:".into(),
            },
        };
        original.config.profiles.push(DeviceProfile {
            config_version: 1,
            module_data: serde_json::json!({}),
            raw: serde_json::Value::Null,
            config_error: None,
            id: "dock".into(),
            adapter_id: ID.into(),
            schema_version: 1,
            name: "Audio Dock".into(),
            device_id: Some("test-dock".into()),
            config,
        });
        let extra_file = source.join("opaque.png");
        image::RgbImage::from_pixel(7, 9, image::Rgb([12, 34, 56]))
            .save(&extra_file)
            .unwrap();
        let opaque_asset = storage::import(&source, &extra_file).unwrap();
        let opaque = serde_json::json!({"id":"unknown","adapter_id":"future.module","name":"Opaque","schema_version":99,"config_version":99,
            "module_data":{"vendor":{"image":opaque_asset}},"config":{"future":true},"extension":[1,2]});
        original
            .config
            .profiles
            .push(serde_json::from_value(opaque.clone()).unwrap());
        let archive = fixture.0.join("dock.m18backup");
        export(&source, &archive, &original).unwrap();
        let summary = inspect(&dest, &archive).unwrap();
        assert_eq!(summary.format_version, 1);
        stage(&dest, &archive, &summary.fingerprint).unwrap();
        assert!(apply_pending(&dest).unwrap().unwrap().restored);
        let restored = storage::load_workspace(&dest).unwrap();
        let dock = restored
            .profiles
            .iter()
            .find(|profile| profile.id == "dock")
            .unwrap();
        assert_eq!(
            serde_json::to_value(
                restored
                    .profiles
                    .iter()
                    .find(|p| p.id == "unknown")
                    .unwrap()
            )
            .unwrap(),
            opaque
        );
        assert_eq!(
            std::fs::read(storage::asset(&dest, &opaque_asset).unwrap()).unwrap(),
            std::fs::read(storage::asset(&source, &opaque_asset).unwrap()).unwrap()
        );
        assert!(dock.config.pages.is_empty());
        assert!(dock.config.bindings[0].enabled);
        assert!(
            matches!(&dock.config.bindings[0].action,KeyAction::Launch{target:LaunchTarget::Uri{uri}} if uri=="msteams:")
        );
    }

    #[test]
    fn portable_roundtrip_preserves_actions_assets_indexes_settings_and_local_secrets() {
        let f = Fixture::new();
        let source = f.root("source");
        let dest = f.root("dest");
        let mut original = data(&source);
        original.config.profiles[0].config.pages[1].keys[0].content_mode =
            Some(KeyContentMode::None);
        original.config.profiles[0].config.pages[1].keys[0].show_text = false;
        original.config.profiles[0].config.pages[1].keys[0].text_position = TextPosition::Bottom;
        original.config.profiles[0].config.pages[1].keys[0].text = "Hidden label retained".into();
        original.config.profiles[0].config.pages[1].keys[0].asset_id =
            Some(original.library.items[0].asset_id.clone());
        original.settings.model_appearances.insert(
            original.config.profiles[0].id.clone(),
            ModelAppearance::Image {
                image: ImageRef::Asset {
                    asset_id: original.library.items[0].asset_id.clone(),
                },
            },
        );
        let previous = data(&dest);
        save(&dest, &previous);
        std::fs::write(source.join("mcp.json"), "PRIVATE SOURCE TOKEN").unwrap();
        std::fs::write(source.join("runtime.log"), "private log").unwrap();
        std::fs::write(
            source
                .join("assets")
                .join(format!("{}.png.tmp", "e".repeat(64))),
            "interrupted scratch",
        )
        .unwrap();
        std::fs::write(dest.join("mcp.json"), "LOCAL TOKEN").unwrap();
        let path = f.0.join("settings.m18backup");
        export(&source, &path, &original).unwrap();
        let summary = inspect(&dest, &path).unwrap();
        assert_eq!(
            (
                summary.pages,
                summary.images,
                summary.folders,
                summary.applications
            ),
            (2, 2, 1, 1)
        );
        let zip = ZipArchive::new(File::open(&path).unwrap()).unwrap();
        assert_eq!(zip.len(), 7);
        assert_eq!(summary.scenes, 1);
        assert!(!zip
            .file_names()
            .any(|n| n.contains("mcp") || n.contains("log")));
        stage(&dest, &path, &summary.fingerprint).unwrap();
        assert_eq!(
            serde_json::to_value(storage::load_workspace(&dest).unwrap()).unwrap(),
            serde_json::to_value(&previous.config).unwrap()
        );
        let note = apply_pending(&dest).unwrap().unwrap();
        assert!(note.restored);
        assert!(note.message.starts_with("备份已恢复"));
        let restored = BackupData::read(&dest).unwrap();
        assert_eq!(restored.json().unwrap(), original.json().unwrap());
        assert_eq!(
            std::fs::read_to_string(dest.join("mcp.json")).unwrap(),
            "LOCAL TOKEN"
        );
        assert!(std::fs::read_dir(dest.join("backups")).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|e| e == "m18backup")));
        assert!(!dest.join(PENDING).exists());
        assert!(apply_pending(&dest).unwrap().is_none());
    }

    #[test]
    fn archive_changes_after_review_cannot_be_staged() {
        let f = Fixture::new();
        let root = f.root("root");
        let d = data(&root);
        let p = f.0.join("a.m18backup");
        export(&root, &p, &d).unwrap();
        let s = inspect(&root, &p).unwrap();
        rewrite(&p, |_| {});
        assert!(stage(&root, &p, &s.fingerprint).is_err());
        assert!(!root.join(PENDING).exists());
    }

    #[test]
    fn restores_fresh_install_and_preserves_damaged_original_metadata() {
        let f = Fixture::new();
        let source = f.root("source");
        let incoming = data(&source);
        let path = f.0.join("portable.m18backup");
        export(&source, &path, &incoming).unwrap();
        for name in ["fresh", "damaged"] {
            let root = f.root(name);
            if name == "damaged" {
                let old = data(&root);
                save(&root, &old);
                std::fs::write(root.join("library.json"), "damaged original").unwrap();
            }
            let summary = inspect(&root, &path).unwrap();
            stage(&root, &path, &summary.fingerprint).unwrap();
            assert!(apply_pending(&root).unwrap().unwrap().restored);
            assert_eq!(
                BackupData::read(&root).unwrap().json().unwrap(),
                incoming.json().unwrap()
            );
            if name == "damaged" {
                let raw = std::fs::read_dir(root.join("backups"))
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                assert_eq!(
                    std::fs::read_to_string(raw.join("library.json")).unwrap(),
                    "damaged original"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn refuses_archive_open_for_writing_during_fingerprint_and_decode() {
        let f = Fixture::new();
        let root = f.root("root");
        let d = data(&root);
        let path = f.0.join("shared.m18backup");
        export(&root, &path, &d).unwrap();
        let writer = File::options().write(true).open(&path).unwrap();
        assert!(inspect(&root, &path).is_err());
        drop(writer);
        assert!(inspect(&root, &path).is_ok());
    }

    #[test]
    fn rejects_traversal_unknown_files_missing_images_bad_hash_and_future_version() {
        let f = Fixture::new();
        let root = f.root("root");
        let d = data(&root);
        let p = f.0.join("a.m18backup");
        for bad_name in [
            "../escaped.json",
            "assets/../../escaped.json",
            "C:/escaped.json",
            "assets\\evil.png",
            "mcp.json",
        ] {
            export(&root, &p, &d).unwrap();
            rewrite(&p, |e| {
                e.insert(bad_name.into(), vec![1]);
            });
            assert!(inspect(&root, &p).is_err(), "{bad_name}");
        }
        export(&root, &p, &d).unwrap();
        rewrite(&p, |e| {
            let image = e.keys().find(|n| n.starts_with("assets/")).unwrap().clone();
            e.remove(&image);
        });
        assert!(inspect(&root, &p).is_err());
        export(&root, &p, &d).unwrap();
        rewrite(&p, |e| {
            e.get_mut("config.json").unwrap().push(b' ');
        });
        assert!(inspect(&root, &p).is_err());
        export(&root, &p, &d).unwrap();
        rewrite(&p, |e| {
            let mut m: Manifest = serde_json::from_slice(&e["manifest.json"]).unwrap();
            m.format_version = 99;
            e.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
        });
        assert!(inspect(&root, &p).is_err());
        assert!(!f.0.join("escaped.json").exists());
    }

    #[test]
    fn rejects_invalid_configuration_even_with_valid_checksums() {
        let f = Fixture::new();
        let root = f.root("root");
        let d = data(&root);
        let p = f.0.join("a.m18backup");
        for field in ["config.json", "library.json", "applications.json"] {
            export(&root, &p, &d).unwrap();
            rewrite(&p, |e| {
                let mut value: serde_json::Value = serde_json::from_slice(&e[field]).unwrap();
                value["version"] = 999.into();
                e.insert(field.into(), serde_json::to_vec(&value).unwrap());
                refresh_manifest(e);
            });
            assert!(inspect(&root, &p).is_err());
        }
        export(&root, &p, &d).unwrap();
        rewrite(&p, |e| {
            let mut config = d.config.clone();
            config.profiles[0].config.pages[0].keys[0].asset_id =
                Some(format!("{}.png", "f".repeat(64)));
            e.insert("config.json".into(), serde_json::to_vec(&config).unwrap());
            refresh_manifest(e);
        });
        assert!(inspect(&root, &p).is_err());
    }

    #[test]
    fn rejects_decompression_bomb_and_total_size_overflow() {
        let f = Fixture::new();
        let root = f.root("root");
        let d = data(&root);
        let p = f.0.join("a.m18backup");
        export(&root, &p, &d).unwrap();
        rewrite(&p, |e| {
            e.insert("config.json".into(), vec![0; MAX_JSON as usize + 1]);
            refresh_manifest(e);
        });
        assert!(inspect(&root, &p).is_err());
        export(&root, &p, &d).unwrap();
        rewrite(&p, |e| {
            let mut m: Manifest = serde_json::from_slice(&e["manifest.json"]).unwrap();
            for entry in m.files.values_mut() {
                entry.size = u64::MAX;
            }
            e.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
        });
        assert!(inspect(&root, &p).is_err());
    }

    #[test]
    fn failed_restore_rolls_back_all_metadata_and_preserves_automatic_backup() {
        let f = Fixture::new();
        let source = f.root("source");
        let root = f.root("root");
        let incoming = data(&source);
        let mut old = data(&root);
        old.config.global.theme = "light".into();
        save(&root, &old);
        let p = f.0.join("a.m18backup");
        export(&source, &p, &incoming).unwrap();
        let s = inspect(&root, &p).unwrap();
        stage(&root, &p, &s.fingerprint).unwrap();
        std::fs::create_dir(root.join("applications.json.tmp")).unwrap();
        assert!(apply_pending(&root)
            .unwrap()
            .unwrap()
            .message
            .starts_with("导入失败"));
        assert_eq!(
            BackupData::read(&root).unwrap().json().unwrap(),
            old.json().unwrap()
        );
        assert!(!root.join(PENDING).exists());
    }

    #[test]
    fn repairs_corrupt_images_and_rolls_back_the_original_bytes_on_later_failure() {
        let f = Fixture::new();
        let source = f.root("source");
        let incoming = data(&source);
        let id = incoming.library.items[0].asset_id.clone();
        let path = f.0.join("repair.m18backup");
        export(&source, &path, &incoming).unwrap();
        for name in ["success", "failure", "image-failure"] {
            let root = f.root(name);
            let old = data(&root);
            save(&root, &old);
            std::fs::write(root.join("assets").join(&id), "corrupt original image").unwrap();
            let summary = inspect(&root, &path).unwrap();
            stage(&root, &path, &summary.fingerprint).unwrap();
            if name == "failure" {
                std::fs::create_dir(root.join("applications.json.tmp")).unwrap();
            }
            if name == "image-failure" {
                std::fs::create_dir(root.join("assets").join(format!("{id}.tmp"))).unwrap();
            }
            let notice = apply_pending(&root).unwrap().unwrap();
            let bytes = std::fs::read(root.join("assets").join(&id)).unwrap();
            if name == "success" {
                assert!(notice.restored);
                assert_eq!(
                    bytes,
                    std::fs::read(source.join("assets").join(&id)).unwrap()
                );
                assert_eq!(
                    BackupData::read(&root).unwrap().json().unwrap(),
                    incoming.json().unwrap()
                );
            } else {
                assert!(!notice.restored);
                assert_eq!(bytes, b"corrupt original image");
                assert_eq!(
                    BackupData::read(&root).unwrap().json().unwrap(),
                    old.json().unwrap()
                );
            }
            let raw = std::fs::read_dir(root.join("backups"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert_eq!(
                std::fs::read(raw.join("assets").join(&id)).unwrap(),
                b"corrupt original image"
            );
        }
    }

    #[test]
    fn startup_recovers_interrupted_transaction_before_loading_config() {
        let f = Fixture::new();
        let root = f.root("root");
        let old = data(&root);
        save(&root, &old);
        let pending = root.join(PENDING);
        std::fs::create_dir_all(pending.join("undo")).unwrap();
        for name in METADATA {
            std::fs::copy(root.join(name), pending.join("undo").join(name)).unwrap();
        }
        storage::save_json(&pending, "journal.json", &Journal::default()).unwrap();
        std::fs::write(root.join("config.json"), "interrupted write").unwrap();
        assert!(apply_pending(&root)
            .unwrap()
            .unwrap()
            .message
            .contains("上次导入中断"));
        assert_eq!(
            BackupData::read(&root).unwrap().json().unwrap(),
            old.json().unwrap()
        );
    }

    #[test]
    fn corrupt_staging_keeps_originals_and_does_not_prevent_startup() {
        let f = Fixture::new();
        let root = f.root("root");
        let d = data(&root);
        save(&root, &d);
        let p = f.0.join("a.m18backup");
        export(&root, &p, &d).unwrap();
        let s = inspect(&root, &p).unwrap();
        stage(&root, &p, &s.fingerprint).unwrap();
        std::fs::write(root.join(PENDING).join("config.json"), "broken").unwrap();
        assert!(apply_pending(&root)
            .unwrap()
            .unwrap()
            .message
            .starts_with("备份未恢复"));
        assert_eq!(
            BackupData::read(&root).unwrap().json().unwrap(),
            d.json().unwrap()
        );
        assert!(!root.join(PENDING).exists());
    }

    #[test]
    fn committed_transaction_never_rolls_back_and_export_cannot_overwrite_live_config() {
        let f = Fixture::new();
        let root = f.root("root");
        let d = data(&root);
        save(&root, &d);
        let pending = root.join(PENDING);
        std::fs::create_dir(&pending).unwrap();
        storage::save_json(&pending, "committed.json", &true).unwrap();
        storage::save_json(&pending, "journal.json", &Journal::default()).unwrap();
        assert_eq!(apply_pending(&root).unwrap().unwrap().message, "备份已恢复");
        assert!(export(&root, &root.join("config.json"), &d).is_err());
        assert_eq!(
            BackupData::read(&root).unwrap().json().unwrap(),
            d.json().unwrap()
        );
    }
}
