use anyhow::{bail, Result};
use base64::Engine;
use std::path::{Path, PathBuf};
pub fn color(value: &str) -> Result<[u8; 3]> {
    if !value.is_ascii()
        || value.len() != 7
        || !value.starts_with('#')
        || !value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!(crate::UiFailure::new("backend.b237", "颜色须为 #RRGGBB"))
    }
    Ok([
        u8::from_str_radix(&value[1..3], 16)?,
        u8::from_str_radix(&value[3..5], 16)?,
        u8::from_str_radix(&value[5..7], 16)?,
    ])
}
pub fn asset(root: &Path, id: &str) -> Result<PathBuf> {
    if !id.is_ascii()
        || id.len() < 68
        || id.len() > 69
        || !id[..64].bytes().all(|b| b.is_ascii_hexdigit())
        || ![".png", ".jpg", ".gif"].contains(&&id[64..])
    {
        bail!(crate::UiFailure::new("backend.b335", "素材标识无效"))
    }
    Ok(root.join("assets").join(id))
}
pub fn decode_data(value: &str) -> Result<Vec<u8>> {
    let raw = value
        .strip_prefix("data:")
        .map(|_| value.split_once(',').map(|(_, v)| v).unwrap_or(""))
        .unwrap_or(value);
    Ok(base64::engine::general_purpose::STANDARD.decode(raw)?)
}
