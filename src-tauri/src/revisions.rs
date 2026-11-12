//! Content revisions describe saved resources, never transient device state.
use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn revision(value: &impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("serializable resource"))
    )
}

pub fn check(expected: Option<&str>, value: &impl Serialize) -> Result<()> {
    if expected.is_some_and(|expected| expected != revision(value)) {
        return Err(alchemy_device_api::ApiError::localized(
            alchemy_device_api::ErrorCode::StaleOperation,
            "errors.savedResourceChanged",
            "The saved resource changed. Read it again before applying this edit; your draft was retained.",
        ).into());
    }
    Ok(())
}
