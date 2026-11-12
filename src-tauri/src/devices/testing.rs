//! Test-only transport events; never links a native device library.
use super::*;
pub enum Event {
    Connected {
        device: DeviceDescriptor,
        path: String,
    },
    Disconnected {
        path: String,
    },
    Key {
        path: String,
        key: u8,
        pressed: bool,
    },
}
pub async fn handle(host: &Arc<Core>, event: Event) {
    let core = if let Some(manager) = host.device_manager() {
        match &event {
            Event::Connected { device, .. } => manager.device_profile(&device.id).ok(),
            Event::Disconnected { path } | Event::Key { path, .. } => manager
                .runtimes()
                .into_iter()
                .map(|r| r.core.clone())
                .find(|c| c.inner.lock().unwrap().paths.values().any(|p| p == path)),
        }
    } else {
        Some(host.clone())
    };
    let Some(core) = core else { return };
    match event {
        Event::Connected { device, path } => {
            core.attach_input_device(device, path);
            if core.supports_pages {
                let c = core.clone();
                tokio::spawn(async move {
                    let _ = c.activate(&c.snapshot().active_page).await;
                });
            }
        }
        Event::Disconnected { .. } => {
            core.clear_connection();
            core.connection("disconnected", None);
        }
        Event::Key { path, key, pressed } => {
            let epoch = core.inner.lock().unwrap().connection_epoch;
            let id = core.snapshot().selected_device.unwrap_or_default();
            core.input_event(&id, &path, epoch, key, pressed);
        }
    }
}
