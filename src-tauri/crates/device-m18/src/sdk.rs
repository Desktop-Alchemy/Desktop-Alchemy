use alchemy_device_api::*;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
/// Only the official native C SDK is a device transport. No SDK subprocess or port.
#[derive(Clone)]
pub struct SdkPaths {
    pub native: PathBuf,
}
/// Pinned official C SDK and its app-local Microsoft C++ runtime dependencies.
pub const NATIVE_FILES: [(&str, &str); 4] = [
    (
        "transport.dll",
        "0fe8361d25a2c1b6d401c6156900758d346b6ecbe5772621e6ec8c81b445eb88",
    ),
    (
        "MSVCP140.dll",
        "def46aa6a8f72f27bafac0c43334419486a4d1dcdb6c479a8ef7034b3e1fa4cb",
    ),
    (
        "VCRUNTIME140.dll",
        "184146852727a9db4eea06178716bec3cdbb1015c911f6b0f915b184ad7775b2",
    ),
    (
        "VCRUNTIME140_1.dll",
        "e6bfb3662ab4b1969a73441dbe35c96d51441b6bff8cf1fe7430bd5b246ca605",
    ),
];
impl SdkPaths {
    /// Installed resources are strict; development/diagnostics explicitly pass None.
    pub fn resolve(resource: Option<&Path>) -> Result<Self> {
        Ok(Self {
            native: match resource {
                Some(root) => root.join("sdk/native"),
                None => {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../vendor/device-sdk/native")
                }
            },
        })
    }
    pub fn verified_transport(&self, root: &Path) -> Result<PathBuf> {
        let dir = root.join("native-sdk-runtime");
        std::fs::create_dir_all(&dir)?;
        for (name, expected) in NATIVE_FILES {
            let data = std::fs::read(self.native.join(name)).with_context(|| {
                format!("官方 C SDK 缺失：{name}，请运行 pnpm sdk:fetch 或重新安装")
            })?;
            if format!("{:x}", Sha256::digest(&data)) != expected {
                bail!("官方 C SDK 文件校验失败：{name}")
            }
            let target = dir.join(name);
            if !target.exists()
                || format!("{:x}", Sha256::digest(std::fs::read(&target)?)) != expected
            {
                std::fs::write(&target, data)?;
            }
        }
        Ok(dir.join("transport.dll"))
    }
}

pub fn device_descriptor(
    vendor: u16,
    product: u16,
    serial: String,
    firmware: String,
    path: &str,
) -> DeviceDescriptor {
    DeviceDescriptor {
        adapter_id: super::ID.into(),
        layout: super::layout().into(),
        id: format!(
            "{vendor:04x}:{product:04x}:{}",
            if serial.is_empty() { path } else { &serial }
        ),
        model: "M18".into(),
        serial,
        capabilities: super::capabilities(&firmware),
        firmware,
        vendor_id: u32::from(vendor),
        product_id: u32::from(product),
    }
}

pub fn m18_key_index(key: u8) -> u8 {
    match key {
        1..=5 => key + 10,
        11..=15 => key - 10,
        _ => key,
    }
}

pub async fn run(host: AdapterHost, paths: SdkPaths) -> Result<()> {
    loop {
        if host.cancel.is_cancelled() {
            return Ok(());
        }
        host.api.backend_status("connecting", None);
        #[cfg(windows)]
        let result = super::native_sdk::session(host.clone(), &paths).await;
        #[cfg(not(windows))]
        let result: Result<()> = Err(anyhow::anyhow!(alchemy_device_api::UiFailure::new("backend.b322", "M18 仅支持 Windows x64")));
        for connection in host.api.connections() {
            host.api.disconnected(&connection);
        }
        if host.cancel.is_cancelled() {
            return Ok(());
        }
        if let Err(e) = result {
            host.api.backend_status("error", Some(e.to_string()));
        }
        tokio::select! {_=host.cancel.cancelled()=>return Ok(()),_=tokio::time::sleep(Duration::from_secs(2))=>{}}
    }
}
#[derive(Debug)]
pub enum SdkEvent {
    Key {
        path: String,
        key: u8,
        pressed: bool,
        reader_id: u64,
    },
    InputReport {
        header: String,
    },
}
#[cfg(windows)]
pub async fn acquire_owner() -> Result<alchemy_device_api::ownership::NamedGuard> {
    let guard = alchemy_device_api::ownership::NamedGuard::acquire(
        "Local\\DesktopAlchemy.DeviceSDK.Owner",
    )?;
    let mut cmd = tokio::process::Command::new("tasklist");
    cmd.args(["/FI", "IMAGENAME eq StreamDock.exe", "/NH"])
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    if String::from_utf8_lossy(&cmd.output().await?.stdout)
        .to_ascii_lowercase()
        .contains("streamdock.exe")
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b323", "请先退出官方 Stream Dock 软件，再重试设备连接"))
    }
    Ok(guard)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_matches_runtime_pins() {
        let m: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../vendor/device-sdk/native/SOURCES.json"
        ))
        .unwrap();
        for (name, hash) in NATIVE_FILES {
            assert_eq!(
                m["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|f| f["name"] == name)
                    .unwrap()["sha256"],
                hash
            );
        }
    }
    #[test]
    fn mapping_is_symmetric() {
        for k in 1..=18 {
            assert_eq!(m18_key_index(m18_key_index(k)), k);
        }
    }
}
