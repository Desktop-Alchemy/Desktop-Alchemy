//! Managed executable packages. Nothing in this module executes package content.
use crate::{model::*, plugins::validate_manifest, storage};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
const MAX_FILE: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST: u64 = 256 * 1024;
const MAX_ENTRIES: usize = 4096;
const BUILTIN: &str = "dev.masterain.performance";

/// Windows names must be safe even when a package was produced on another OS.
pub fn safe_path(name: &str) -> Result<String> {
    if name.is_empty() || name.contains('\\') || name.starts_with('/') || name.len() > 1000 {
        bail!("插件包路径无效：{name}");
    }
    let name = name.strip_suffix('/').unwrap_or(name);
    for part in name.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"]
                    .contains(&&stem[3..]))
        {
            bail!("插件包包含不安全的 Windows 路径：{name}");
        }
    }
    Ok(name.to_lowercase())
}

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 100
        || id.contains('/')
        || id.contains('\\')
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b167", "插件 ID 无效"));
    }
    safe_path(id)?;
    Ok(())
}

fn fingerprint(file: &mut File) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    std::io::copy(file, &mut digest)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(format!("{:x}", digest.finalize()))
}

pub struct Stage {
    pub directory: PathBuf,
    pub summary: PluginPackageSummary,
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Read every entry, including its CRC, with limits on actual decompressed bytes.
fn read_package(path: &Path, output: Option<&Path>) -> Result<PluginPackageSummary> {
    if path.extension().and_then(|s| s.to_str()) != Some("m18plugin") {
        bail!(alchemy_device_api::UiFailure::new("backend.b176", "请选择 .m18plugin 插件包"));
    }
    let mut file = File::open(path)?;
    if file.metadata()?.len() > MAX_ARCHIVE {
        bail!(alchemy_device_api::UiFailure::new("backend.b177", "插件包超过 512 MiB"));
    }
    let hash = fingerprint(&mut file)?;
    let mut archive = ZipArchive::new(&mut file).context(alchemy_device_api::UiFailure::new("backend.b178", "插件包不是有效的 ZIP 文件"))?;
    if archive.len() > MAX_ENTRIES {
        bail!(alchemy_device_api::UiFailure::new("backend.b179", "插件包条目过多"));
    }
    let mut names = BTreeMap::new();
    // Validate the complete namespace before writing any entries.
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = safe_path(entry.name())?;
        if entry.encrypted() {
            bail!(alchemy_device_api::UiFailure::new("backend.b180", "不支持加密插件包"));
        }
        let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
        if kind != 0 && kind != 0o100000 && !(kind == 0o040000 && entry.is_dir()) {
            bail!(alchemy_device_api::UiFailure::new("backend.b181", "插件包不能包含链接或特殊文件"));
        }
        if names.insert(name, entry.is_dir()).is_some() {
            bail!(alchemy_device_api::UiFailure::new("backend.b182", "插件包路径重复或大小写冲突"));
        }
    }
    for name in names.keys() {
        let mut parent = name.as_str();
        while let Some((p, _)) = parent.rsplit_once('/') {
            if names.get(p) == Some(&false) {
                bail!(alchemy_device_api::UiFailure::new("backend.b183", "插件包文件与目录冲突"));
            }
            parent = p;
        }
    }
    let mut manifest = None;
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let limit = if entry.name() == "plugin.json" {
            MAX_MANIFEST
        } else {
            MAX_FILE
        };
        if entry.size() > limit {
            bail!(alchemy_device_api::UiFailure::new("backend.b184", "插件包单文件超过大小限制"));
        }
        let target = output.map(|p| p.join(entry.name()));
        let mut writer = if let Some(target) = target {
            std::fs::create_dir_all(target.parent().context(alchemy_device_api::UiFailure::new("backend.b185", "插件路径无效"))?)?;
            Some(File::options().create_new(true).write(true).open(target)?)
        } else {
            None
        };
        let mut bytes = Vec::new();
        let mut count = 0_u64;
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            total += n as u64;
            if count > limit || total > MAX_TOTAL {
                bail!(alchemy_device_api::UiFailure::new("backend.b186", "插件包解压数据超过大小限制"));
            }
            if entry.name() == "plugin.json" {
                bytes.extend_from_slice(&buffer[..n]);
            }
            if let Some(writer) = &mut writer {
                writer.write_all(&buffer[..n])?;
            }
        }
        if let Some(writer) = writer {
            writer.sync_all()?;
        }
        if entry.name() == "plugin.json" {
            manifest = Some(serde_json::from_slice::<PluginManifest>(&bytes)?);
        }
    }
    let manifest = manifest.context(alchemy_device_api::UiFailure::new("backend.b187", "插件包根目录缺少 plugin.json"))?;
    validate_manifest(&manifest)?;
    validate_id(&manifest.id)?;
    if manifest.id.eq_ignore_ascii_case(BUILTIN) {
        bail!(alchemy_device_api::UiFailure::new("backend.b188", "不能覆盖内置插件"));
    }
    if let Some(executable) = &manifest.executable {
        let exe = executable.replace('\\', "/");
        let exe_key = safe_path(&exe)?;
        if !exe.to_ascii_lowercase().ends_with(".exe") || names.get(&exe_key) != Some(&false) {
            bail!(alchemy_device_api::UiFailure::new("backend.b189", "插件包缺少清单指定的 EXE"));
        }
    }
    let mut icon_bytes = 0_usize;
    for icon in &manifest.icons {
        let mut entry = archive
            .by_name(&icon.path)
            .context(alchemy_device_api::UiFailure::new("backend.b190", "插件包缺少声明的图标"))?;
        if entry.size() > 20 * 1024 * 1024 {
            bail!(alchemy_device_api::UiFailure::new("backend.b191", "插件图标超过大小限制"));
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        icon_bytes += bytes.len();
        if icon_bytes > crate::plugin_icons::MAX_BYTES {
            bail!(alchemy_device_api::UiFailure::new("backend.b192", "插件图标合计超过 128 MiB"));
        }
        crate::plugin_icons::validate_png(&bytes)?;
    }
    drop(archive);
    if fingerprint(&mut file)? != hash {
        bail!(alchemy_device_api::UiFailure::new("backend.b193", "插件包读取期间发生变化，请重新选择"));
    }
    Ok(PluginPackageSummary {
        manifest,
        fingerprint: hash,
        installed_version: None,
        total_bytes: total as f64,
    })
}

pub fn inspect(path: &Path) -> Result<PluginPackageSummary> {
    read_package(path, None)
}

pub fn stage(root: &Path, path: &Path, expected: &str) -> Result<Stage> {
    let directory = root
        .join("plugin-staging")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&directory)?;
    let result = read_package(path, Some(&directory));
    match result {
        Ok(summary) if summary.fingerprint == expected => Ok(Stage { directory, summary }),
        other => {
            let _ = std::fs::remove_dir_all(directory);
            match other {
                Ok(_) => bail!(alchemy_device_api::UiFailure::new("backend.b194", "插件包已改变，请重新检查后导入")),
                Err(e) => Err(e),
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Journal {
    id: String,
    existed: bool,
    #[serde(default)]
    icons_existed: bool,
}

fn rollback(root: &Path, transaction: &Path, journal: &Journal) -> Result<()> {
    let target = root.join("plugins").join(&journal.id);
    let old = transaction.join("old");
    if old.exists() || !journal.existed {
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        if old.exists() {
            std::fs::rename(old, &target)?;
        }
    }
    let old_icons = transaction.join("old-icons");
    if journal.icons_existed && old_icons.exists() {
        let target = root.join("plugin-icons").join(&journal.id);
        std::fs::create_dir_all(root.join("plugin-icons"))?;
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(old_icons, target)?;
    }
    let config: AppConfig =
        serde_json::from_slice(&std::fs::read(transaction.join("config.json"))?)?;
    let expected = storage::global_from_config(&config);
    let current = storage::load_workspace(root)?.global;
    if current.theme != expected.theme
        || current.managed_plugin_ids != expected.managed_plugin_ids
        || current.disabled_plugins != expected.disabled_plugins
    {
        storage::save(root, &config)?;
    }
    Ok(())
}

fn cleanup(root: &Path, transaction: &Path) -> Result<()> {
    let garbage = root
        .join("cache")
        .join(format!("plugin-gc-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("cache"))?;
    std::fs::rename(transaction, &garbage)?;
    // A sharing violation can leave only garbage, never an active recovery journal.
    let _ = std::fs::remove_dir_all(garbage);
    Ok(())
}

/// Called before Core loads config, also before a pending settings restore.
pub fn recover(root: &Path) -> Result<()> {
    let transactions = root.join("plugin-transactions");
    if !transactions.exists() {
        if root.join("plugin-staging").exists() {
            std::fs::remove_dir_all(root.join("plugin-staging"))?;
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(transactions)? {
        let directory = entry?.path();
        if !directory.is_dir() {
            bail!(alchemy_device_api::UiFailure::new("backend.b195", "插件事务目录损坏"));
        }
        if directory.join("journal.json").exists() {
            let journal: Journal =
                serde_json::from_slice(&std::fs::read(directory.join("journal.json"))?)?;
            validate_id(&journal.id)?;
            if !directory.join("committed").exists() {
                rollback(root, &directory, &journal)?;
            }
        }
        cleanup(root, &directory)?;
    }
    if root.join("plugin-staging").exists() {
        std::fs::remove_dir_all(root.join("plugin-staging"))?;
    }
    Ok(())
}

/// Caller serializes lifecycle and holds the Core configuration lock.
pub fn ensure_ready(root: &Path) -> Result<()> {
    let directory = root.join("plugin-transactions");
    if directory.exists() {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.join("journal.json").exists() && !path.join("committed").exists() {
                bail!(alchemy_device_api::UiFailure::new("backend.b196", "插件事务尚未完成恢复，请重启客户端后再操作"));
            }
        }
    }
    Ok(())
}

pub fn commit(
    root: &Path,
    id: &str,
    old: &AppConfig,
    next: &AppConfig,
    payload: Option<&Path>,
) -> Result<()> {
    ensure_ready(root)?;
    validate_id(id)?;
    let target = root.join("plugins").join(id);
    std::fs::create_dir_all(root.join("plugins"))?;
    let transaction = root
        .join("plugin-transactions")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&transaction)?;
    let journal = Journal {
        id: id.into(),
        existed: target.exists(),
        icons_existed: payload.is_none() && root.join("plugin-icons").join(id).exists(),
    };
    storage::save_json(&transaction, "config.json", old)?;
    storage::save_json(&transaction, "journal.json", &journal)?;
    let operation = (|| -> Result<()> {
        if journal.icons_existed {
            std::fs::rename(
                root.join("plugin-icons").join(id),
                transaction.join("old-icons"),
            )?;
        }
        if journal.existed {
            std::fs::rename(&target, transaction.join("old"))?;
        }
        if let Some(payload) = payload {
            std::fs::rename(payload, &target)?;
        }
        storage::save(root, next)?;
        storage::write_atomic(&transaction, "committed", b"committed")?;
        Ok(())
    })();
    if let Err(error) = operation {
        rollback(root, &transaction, &journal).context(alchemy_device_api::UiFailure::new("backend.b197", "插件事务回滚失败，必须重启恢复后再操作"))?;
        cleanup(root, &transaction)?;
        return Err(error);
    }
    // Committed data remains valid even if deferred garbage cleanup fails.
    let _ = cleanup(root, &transaction);
    Ok(())
}

pub fn pack(source: &Path, destination: &Path) -> Result<()> {
    let source = source.canonicalize()?;
    if destination.exists() && destination.canonicalize()?.starts_with(&source) {
        bail!(alchemy_device_api::UiFailure::new("backend.b198", "输出插件包不能位于插件源目录内"));
    }
    fn walk(root: &Path, path: &Path, zip: &mut ZipWriter<File>) -> Result<()> {
        for item in std::fs::read_dir(path)? {
            let item = item?;
            let metadata = std::fs::symlink_metadata(item.path())?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    bail!(alchemy_device_api::UiFailure::new("backend.b199", "插件源目录不能包含重解析点"));
                }
            }
            if metadata.file_type().is_symlink() {
                bail!(alchemy_device_api::UiFailure::new("backend.b200", "插件源目录不能包含链接"));
            }
            if metadata.is_dir() {
                walk(root, &item.path(), zip)?;
            } else if metadata.is_file() {
                let name = item
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                safe_path(&name)?;
                zip.start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )?;
                std::io::copy(&mut File::open(item.path())?, zip)?;
            } else {
                bail!(alchemy_device_api::UiFailure::new("backend.b201", "插件源目录含特殊文件"));
            }
        }
        Ok(())
    }
    let parent = destination.parent().context(alchemy_device_api::UiFailure::new("backend.b202", "插件包输出目录无效"))?;
    std::fs::create_dir_all(parent)?;
    // Canonicalize the parent as destination need not exist yet.
    if parent.canonicalize()?.starts_with(&source) {
        bail!(alchemy_device_api::UiFailure::new("backend.b198", "输出插件包不能位于插件源目录内"));
    }
    let temporary = parent.join(format!("{}.m18plugin", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut zip = ZipWriter::new(File::create(&temporary)?);
        walk(&source, &source, &mut zip)?;
        zip.finish()?.sync_all()?;
        inspect(&temporary)?;
        storage::replace_file(&temporary, destination)
    })();
    let _ = std::fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("m18-package-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn manifest() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"id":"test.package","name":"Test","version":"1","api_version":3,"executable":"plugin.exe","permissions":[]})).unwrap()
    }
    fn archive(root: &Path, extra: &[(&str, &[u8])]) -> PathBuf {
        let path = root.join(format!("{}.m18plugin", uuid::Uuid::new_v4()));
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in [
            ("plugin.json", manifest().as_slice()),
            ("plugin.exe", b"test"),
        ]
        .into_iter()
        .chain(extra.iter().copied())
        {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }
    #[test]
    fn validates_namespace_crc_manifest_fingerprint_and_limits() {
        let root = root();
        let package = archive(&root, &[("assets/text.txt", b"hello")]);
        let summary = inspect(&package).unwrap();
        assert_eq!(summary.manifest.id, "test.package");
        let staged = stage(&root, &package, &summary.fingerprint).unwrap();
        assert_eq!(
            std::fs::read(staged.directory.join("assets/text.txt")).unwrap(),
            b"hello"
        );
        assert!(stage(&root, &package, "wrong").is_err());
        drop(staged);
        for name in [
            "../bad",
            "/bad",
            "C:/bad",
            "a\\bad",
            "NUL.txt",
            "COM1.exe",
            "a./bad",
            "a ",
            "a:b",
            "plugin.EXE",
            "a/../bad",
        ] {
            assert!(
                inspect(&archive(&root, &[(name, b"bad")])).is_err(),
                "{name}"
            );
        }
        assert!(inspect(&archive(
            &root,
            &[("assets", b"file"), ("assets/child", b"bad")]
        ))
        .is_err());
        assert!(inspect(&archive(
            &root,
            &[("other.json", &vec![b' '; MAX_MANIFEST as usize + 1])]
        ))
        .is_ok());
        // Root manifest is bounded before parsing.
        let path = root.join("huge.m18plugin");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("plugin.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&vec![b' '; MAX_MANIFEST as usize + 1])
            .unwrap();
        zip.finish().unwrap();
        assert!(inspect(&path).unwrap_err().to_string().contains("大小限制"));
        let path = root.join("link.m18plugin");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.add_symlink("link", "../escape", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        assert!(inspect(&path).is_err());
        let path = root.join("reserved.m18plugin");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("plugin.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&manifest()).unwrap();
        zip.finish().unwrap();
        assert!(inspect(&path).is_err()); // missing executable
                                          // Malformed central-directory limits, CRC, and encryption flags must all
                                          // be rejected even during a read-only review (no extraction required).
        let valid = std::fs::read(&package).unwrap();
        let headers: Vec<_> = valid
            .windows(4)
            .enumerate()
            .filter_map(|(i, bytes)| (bytes == b"PK\x01\x02").then_some(i))
            .collect();
        let executable_header = headers[1];
        for kind in ["size", "crc", "encrypted"] {
            let mut damaged = valid.clone();
            match kind {
                "size" => damaged[executable_header + 24..executable_header + 28]
                    .copy_from_slice(&u32::MAX.to_le_bytes()),
                "crc" => damaged[executable_header + 16] ^= 1,
                _ => damaged[executable_header + 8] |= 1,
            }
            let path = root.join(format!("{kind}.m18plugin"));
            std::fs::write(&path, damaged).unwrap();
            assert!(inspect(&path).is_err(), "{kind}");
        }
        let path = root.join("too-many.m18plugin");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        for n in 0..=MAX_ENTRIES {
            zip.start_file(format!("file-{n}"), SimpleFileOptions::default())
                .unwrap();
        }
        zip.finish().unwrap();
        assert!(inspect(&path).unwrap_err().to_string().contains("条目过多"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_uninstall_restores_icons_unless_committed() {
        for committed in [false, true] {
            let root = root();
            std::fs::create_dir_all(root.join("plugins")).unwrap();
            let config = crate::devices::default_config();
            storage::save(&root, &config).unwrap();
            let tx = root.join("plugin-transactions/crash");
            std::fs::create_dir_all(tx.join("old")).unwrap();
            std::fs::create_dir_all(tx.join("old-icons")).unwrap();
            std::fs::write(tx.join("old-icons/index.json"), b"{}").unwrap();
            storage::save_json(&tx, "config.json", &config).unwrap();
            storage::save_json(
                &tx,
                "journal.json",
                &Journal {
                    id: "test.package".into(),
                    existed: true,
                    icons_existed: true,
                },
            )
            .unwrap();
            if committed {
                storage::write_atomic(&tx, "committed", b"committed").unwrap();
            }
            recover(&root).unwrap();
            assert_eq!(root.join("plugins/test.package").exists(), !committed);
            assert_eq!(
                root.join("plugin-icons/test.package/index.json").exists(),
                !committed
            );
            assert!(!tx.exists());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn transactions_rollback_failed_save_and_recover_both_crash_outcomes() {
        let root = root();
        let config = crate::devices::default_config();
        storage::save(&root, &config).unwrap();
        let package = archive(&root, &[]);
        let summary = inspect(&package).unwrap();
        let staged = stage(&root, &package, &summary.fingerprint).unwrap();
        let mut next = config.clone();
        next.managed_plugin_ids.push("test.package".into());
        next.disabled_plugins.push("test.package".into());
        commit(
            &root,
            "test.package",
            &config,
            &next,
            Some(&staged.directory),
        )
        .unwrap();
        let target = root.join("plugins/test.package");
        std::fs::write(target.join("old-marker"), "old").unwrap();
        let staged = stage(&root, &package, &summary.fingerprint).unwrap();
        std::fs::create_dir(root.join("config.json.tmp")).unwrap();
        let mut changed = next.clone();
        changed.theme = "dark".into();
        assert!(commit(
            &root,
            "test.package",
            &next,
            &changed,
            Some(&staged.directory)
        )
        .is_err());
        assert!(target.join("old-marker").exists());
        std::fs::remove_dir(root.join("config.json.tmp")).unwrap();
        for committed in [false, true] {
            let tx = root.join("plugin-transactions/crash");
            std::fs::create_dir_all(&tx).unwrap();
            storage::save_json(&tx, "config.json", &next).unwrap();
            storage::save_json(
                &tx,
                "journal.json",
                &Journal {
                    id: "test.package".into(),
                    existed: true,
                    icons_existed: false,
                },
            )
            .unwrap();
            std::fs::rename(&target, tx.join("old")).unwrap();
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join("new-marker"), "new").unwrap();
            if committed {
                storage::write_atomic(&tx, "committed", b"committed").unwrap();
            }
            recover(&root).unwrap();
            assert_eq!(target.join("new-marker").exists(), committed);
            assert!(!tx.exists());
        }
        commit(&root, "test.package", &next, &config, None).unwrap();
        assert!(!target.exists());
        assert!(storage::load(&root).unwrap().managed_plugin_ids.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
