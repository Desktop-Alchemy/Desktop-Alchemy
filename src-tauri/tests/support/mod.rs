use desktop_alchemy_core::{core::Core, devices, model::DeviceProfile, storage};
use std::{path::PathBuf, sync::Arc};
pub fn device_core(root: PathBuf) -> Arc<Core> {
    let host = Core::new(root.clone()).unwrap();
    let existing = storage::load_workspace(&root)
        .unwrap()
        .profiles
        .into_iter()
        .next();
    let mut profile = existing.unwrap_or_else(|| {
        let p = DeviceProfile::new(
            devices::default_adapter_id().into(),
            "test".into(),
            None,
            devices::default_config(),
        );
        storage::edit_workspace(&root, |w| {
            w.profiles.push(p.clone());
            Ok(())
        })
        .unwrap();
        p
    });
    let global = storage::load_workspace(&root).unwrap().global;
    profile.config.theme = global.theme;
    profile.config.managed_plugin_ids = global.managed_plugin_ids;
    profile.config.disabled_plugins = global.disabled_plugins;
    let core =
        Core::for_profile(&host, &profile, devices::definition(&profile.adapter_id)).unwrap();
    let _ = core.adapter.set(Arc::new(devices::m18::Adapter));
    core
}
