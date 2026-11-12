//! Shared AppsFolder catalog. Stable Shell identities, never row indexes or names.
use crate::model::*;
use anyhow::{Context, Result};
use std::sync::{Mutex, OnceLock};
static CATALOG: OnceLock<Mutex<Vec<RegisteredApplication>>> = OnceLock::new();
static ICONS: OnceLock<Mutex<std::collections::BTreeMap<(std::path::PathBuf, String), String>>> =
    OnceLock::new();
pub struct ApplicationCatalog;
impl ApplicationCatalog {
    pub fn refresh_icon(root: &std::path::Path, identity: &RegisteredIdentity) -> Result<String> {
        ICONS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .remove(&(root.to_path_buf(), serde_json::to_string(identity)?));
        Self::icon(root, identity)
    }
    pub fn list() -> Result<Vec<RegisteredApplication>> {
        let entries = crate::launcher::registered_applications()?;
        *CATALOG.get_or_init(Default::default).lock().unwrap() = entries.clone();
        Ok(entries)
    }
    pub fn icon(root: &std::path::Path, identity: &RegisteredIdentity) -> Result<String> {
        let key = (root.to_path_buf(), serde_json::to_string(identity)?);
        if let Some(id) = ICONS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .get(&key)
        {
            if crate::storage::asset(root, id).is_ok() {
                return Ok(id.clone());
            }
        }
        let source = match identity {
            RegisteredIdentity::Aumid { id } => {
                format!("{}{id}", crate::launcher::WINDOWS_APP_PREFIX)
            }
            RegisteredIdentity::AppsFolder { parsing_name } => {
                let cached=CATALOG.get_or_init(Default::default).lock().unwrap().iter().any(|entry|matches!(&entry.target,LaunchTarget::RegisteredApplication{identity:found,..} if found==identity));
                if !cached {
                    Self::list()?.iter().find(|entry|matches!(&entry.target,LaunchTarget::RegisteredApplication{identity:found,..} if found==identity)).context(alchemy_device_api::UiFailure::new("backend.b012", "应用不再注册于 AppsFolder"))?;
                }
                parsing_name.clone()
            }
        };
        let id = crate::application_icons::cache_registered(root, &source)?;
        ICONS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(key, id.clone());
        Ok(id)
    }
}
