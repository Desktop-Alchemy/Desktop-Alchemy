//! Page-bound plugin displays. Samples and rendered frames are ephemeral and bounded.
use crate::{
    core::{Core, Inner, Work},
    model::*,
    render, storage,
};
use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct WidgetFrame {
    pub key: u8,
    pub png: Arc<Vec<u8>>,
    pub binding: WidgetBinding,
    pub device_path: String,
    pub data: WidgetData,
}
#[derive(Clone)]
struct Record {
    sample: WidgetSample,
    received: Instant,
}
#[derive(Default)]
pub struct WidgetState {
    samples: BTreeMap<(String, String, Option<String>), Record>,
    catalogs: BTreeMap<String, WidgetCatalog>,
    pub pending: BTreeMap<u8, Work>,
    last: BTreeMap<u8, WidgetFrame>,
    pub page_ready: bool,
    pub submitted: u64,
    pub failures: u64,
}
impl WidgetState {
    pub fn seed_from(&mut self, other: &WidgetState) {
        self.samples = other.samples.clone();
        self.catalogs = other.catalogs.clone();
    }
    pub fn clear_frames(&mut self) {
        self.pending.clear();
        self.last.clear();
        self.page_ready = false;
    }
    pub fn cancel(&mut self, owner: &str) {
        self.pending
            .retain(|_, w| w.source.as_deref() != Some(owner));
        self.last.retain(|_, w| w.binding.plugin_id != owner);
        self.samples.retain(|(p, _, _), _| p != owner);
        self.catalogs.remove(owner);
    }
}
pub fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
}
pub fn validate_binding(binding: &WidgetBinding) -> Result<()> {
    if !identifier(&binding.plugin_id) || !identifier(&binding.widget_id) {
        bail!(alchemy_device_api::UiFailure::new("backend.b253", "插件显示绑定 ID 无效"))
    }
    if binding.source_id.as_ref().is_some_and(|s| !identifier(s))
        || binding
            .source_name
            .as_ref()
            .is_some_and(|s| s.chars().count() > 160 || s.chars().any(char::is_control))
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b254", "插件数据来源无效"));
    }
    if binding
        .presentation_id
        .as_ref()
        .is_some_and(|id| !identifier(id))
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b255", "插件显示方式 ID 无效"));
    }
    for color in [
        &binding.style.foreground,
        &binding.style.background,
        &binding.style.accent,
    ] {
        storage::color(color)?;
    }
    Ok(())
}
pub fn validate_data(data: &WidgetData) -> Result<()> {
    if !identifier(&data.widget_id)
        || data.source_id.as_ref().is_some_and(|s| !identifier(s))
        || data
            .secondary_value
            .as_ref()
            .is_some_and(|s| s.chars().count() > 24 || s.chars().any(char::is_control))
        || data.title.chars().count() > 32
        || data.value.chars().count() > 24
        || data.detail.as_ref().is_some_and(|v| v.chars().count() > 48)
        || data
            .progress
            .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        || [&data.title, &data.value]
            .into_iter()
            .chain(data.detail.iter())
            .any(|v| v.chars().any(char::is_control))
    {
        bail!(alchemy_device_api::UiFailure::new("backend.b256", "插件指标内容无效或超出限制"))
    }
    if data.presentations.len() > 16 {
        bail!(alchemy_device_api::UiFailure::new("backend.b257", "插件显示方式数据超过限制"));
    }
    for (id, content) in &data.presentations {
        if !identifier(id)
            || content
                .secondary_value
                .as_ref()
                .is_some_and(|s| s.chars().count() > 24 || s.chars().any(char::is_control))
            || content.title.chars().count() > 32
            || content.value.chars().count() > 24
            || content
                .detail
                .as_ref()
                .is_some_and(|v| v.chars().count() > 48)
            || content
                .progress
                .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            || [&content.title, &content.value]
                .into_iter()
                .chain(content.detail.iter())
                .any(|v| v.chars().any(char::is_control))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b258", "插件显示方式内容无效或超出限制"));
        }
    }
    Ok(())
}
fn running(i: &Inner, owner: &str) -> bool {
    !i.snapshot
        .config
        .disabled_plugins
        .iter()
        .any(|p| p == owner)
        && i.snapshot
            .plugins
            .iter()
            .any(|p| p.manifest.id == owner && p.status == "running")
}
fn supports_images(i: &Inner, device: &str) -> bool {
    i.snapshot
        .devices
        .iter()
        .find(|d| d.id == device)
        .and_then(|d| d.capabilities.get("key_images"))
        .is_some_and(|capability| capability.available)
}
fn data_for(i: &Inner, binding: &WidgetBinding) -> Option<WidgetData> {
    if !running(i, &binding.plugin_id) {
        return None;
    }
    let definition = i
        .snapshot
        .plugins
        .iter()
        .find(|p| p.manifest.id == binding.plugin_id)?
        .manifest
        .widgets
        .iter()
        .find(|w| w.id == binding.widget_id)?;
    let mut data = i
        .widgets
        .samples
        .get(&(
            binding.plugin_id.clone(),
            binding.widget_id.clone(),
            binding.source_id.clone(),
        ))
        .filter(|r| r.received.elapsed() < Duration::from_secs(5))
        .filter(|_| {
            !definition.select_source
                || i.widgets.catalogs.get(&binding.plugin_id).is_some_and(|c| {
                    c.sources.iter().any(|s| {
                        s.widget_id == binding.widget_id
                            && Some(&s.id) == binding.source_id.as_ref()
                            && s.available
                    })
                })
        })
        .map(|r| r.sample.data.clone())
        .unwrap_or(WidgetData {
            source_id: binding.source_id.clone(),
            secondary_value: None,
            presentations: Default::default(),
            widget_id: binding.widget_id.clone(),
            title: binding
                .source_name
                .clone()
                .unwrap_or_else(|| definition.name.clone())
                .chars()
                .take(32)
                .collect(),
            value: "—".into(),
            detail: None,
            progress: None,
            valid: false,
        });
    if let Some(id) = &binding.presentation_id {
        if definition.presentations.first().is_none_or(|p| &p.id != id) {
            if let Some(content) = data
                .presentations
                .get(id)
                .filter(|_| definition.presentations.iter().any(|p| &p.id == id))
                .cloned()
            {
                data.title = content.title;
                data.value = content.value;
                data.secondary_value = content.secondary_value;
                data.detail = content.detail;
                data.progress = content.progress;
                data.valid = content.valid;
            } else {
                data.valid = false;
            }
        }
    }
    data.presentations.clear();
    if !data.valid {
        data.value = "—".into();
        data.detail = None;
        data.progress = None;
        data.secondary_value = None;
    }
    Some(data)
}
pub fn frame_is_current(i: &Inner, work: &Work) -> bool {
    let Some(frame) = &work.widget_frame else {
        return true;
    };
    i.widgets.page_ready
        && supports_images(i, &work.device_id)
        && running(i, &frame.binding.plugin_id)
        && i.source_epochs
            .get(&frame.binding.plugin_id)
            .copied()
            .unwrap_or(0)
            == work.source_epoch
        && i.paths.get(&work.device_id) == Some(&frame.device_path)
        && !i.overrides.contains_key(&format!("key:{}", frame.key))
        && i.snapshot
            .config
            .pages
            .iter()
            .find(|p| p.id == i.snapshot.active_page)
            .and_then(|p| p.keys.iter().find(|k| k.key == frame.key))
            .and_then(|k| k.widget.as_ref())
            == Some(&frame.binding)
}
impl Core {
    pub fn widget_catalog(&self, owner: &str) -> WidgetCatalog {
        self.inner
            .lock()
            .unwrap()
            .widgets
            .catalogs
            .get(owner)
            .cloned()
            .unwrap_or_default()
    }
    pub fn publish_widget_catalog(&self, owner: &str, catalog: WidgetCatalog) -> Result<()> {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                manager.sync_plugins();
                for runtime in manager.runtimes() {
                    runtime
                        .core
                        .publish_widget_catalog(owner, catalog.clone())?;
                }
            }
        }
        let mut i = self.inner.lock().unwrap();
        if !running(&i, owner) {
            bail!(alchemy_device_api::UiFailure::new("backend.b259", "插件已停止或停用"));
        }
        let manifest = &i
            .snapshot
            .plugins
            .iter()
            .find(|p| p.manifest.id == owner)
            .context(alchemy_device_api::UiFailure::new("backend.b124", "插件不存在"))?
            .manifest;
        if catalog.sources.len() > 512
            || ![
                "",
                "ready",
                "starting",
                "driver_required",
                "permission_required",
                "unavailable",
                "error",
            ]
            .contains(&catalog.hardware_status.as_str())
            || catalog
                .message
                .as_ref()
                .is_some_and(|m| m.chars().count() > 512 || m.chars().any(char::is_control))
        {
            bail!(alchemy_device_api::UiFailure::new("backend.b260", "插件来源目录无效或超过限制"));
        }
        let mut seen = std::collections::HashSet::new();
        for source in &catalog.sources {
            if !identifier(&source.id)
                || !seen.insert((&source.widget_id, &source.id))
                || source.name.is_empty()
                || source.name.chars().count() > 160
                || source.name.chars().any(char::is_control)
                || source
                    .error
                    .as_ref()
                    .is_some_and(|e| e.chars().count() > 512 || e.chars().any(char::is_control))
                || !manifest
                    .widgets
                    .iter()
                    .any(|w| w.id == source.widget_id && w.select_source)
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b261", "插件数据来源未声明或无效"));
            }
        }
        i.widgets.samples.retain(|(plugin, widget, source), _| {
            plugin != owner
                || source.as_ref().is_none_or(|id| {
                    catalog
                        .sources
                        .iter()
                        .any(|s| &s.widget_id == widget && &s.id == id)
                })
        });
        i.widgets.catalogs.insert(owner.into(), catalog);
        drop(i);
        self.emit_widgets();
        Ok(())
    }
    pub fn widget_samples(&self) -> Vec<WidgetSample> {
        self.inner
            .lock()
            .unwrap()
            .widgets
            .samples
            .values()
            .map(|r| r.sample.clone())
            .collect()
    }
    pub fn emit_widgets(&self) {
        let _ = self.widget_changed.send(self.widget_samples());
    }
    pub fn widget_data(&self, visual: &KeyVisual) -> Option<WidgetData> {
        let i = self.inner.lock().unwrap();
        data_for(&i, visual.widget.as_ref()?)
    }
    pub fn publish_widget(&self, owner: &str, data: WidgetData) -> Result<()> {
        self.publish_widgets(owner, vec![data])
    }
    pub fn publish_widgets(&self, owner: &str, values: Vec<WidgetData>) -> Result<()> {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                manager.sync_plugins();
                for runtime in manager.runtimes() {
                    runtime.core.publish_widgets(owner, values.clone())?;
                }
            }
        }
        if values.len() > 512 {
            bail!(alchemy_device_api::UiFailure::new("backend.b262", "指标批次超过限制"));
        }
        for data in &values {
            validate_data(data)?;
        }
        let mut i = self.inner.lock().unwrap();
        if !running(&i, owner) {
            bail!(alchemy_device_api::UiFailure::new("backend.b259", "插件已停止或停用"));
        }
        let plugin = i
            .snapshot
            .plugins
            .iter()
            .find(|p| p.manifest.id == owner)
            .context(alchemy_device_api::UiFailure::new("backend.b124", "插件不存在"))?;
        let mut seen = std::collections::HashSet::new();
        for data in &values {
            if !seen.insert((&data.widget_id, &data.source_id)) {
                bail!(alchemy_device_api::UiFailure::new("backend.b263", "指标批次重复来源"));
            }
            let definition = plugin
                .manifest
                .widgets
                .iter()
                .find(|w| w.id == data.widget_id)
                .context(alchemy_device_api::UiFailure::new("backend.b264", "插件未声明该显示控件"))?;
            if definition.select_source != data.source_id.is_some() {
                bail!(alchemy_device_api::UiFailure::new("backend.b265", "插件指标来源与声明不符"));
            }
            if let Some(source) = &data.source_id {
                if !i.widgets.catalogs.get(owner).is_some_and(|c| {
                    c.sources
                        .iter()
                        .any(|s| s.widget_id == data.widget_id && &s.id == source)
                }) {
                    bail!(alchemy_device_api::UiFailure::new("backend.b266", "插件未发布该数据来源"));
                }
            }
            if data
                .presentations
                .keys()
                .any(|id| !definition.presentations.iter().any(|p| &p.id == id))
            {
                bail!(alchemy_device_api::UiFailure::new("backend.b267", "插件未声明该显示方式"));
            }
        }
        for data in values {
            i.widgets.samples.insert(
                (owner.into(), data.widget_id.clone(), data.source_id.clone()),
                Record {
                    sample: WidgetSample {
                        plugin_id: owner.into(),
                        data,
                        updated_at: now(),
                    },
                    received: Instant::now(),
                },
            );
        }
        drop(i);
        self.emit_widgets();
        Ok(())
    }
    /// Runs on a blocking worker, never while holding the Core lock during rasterization.
    pub fn refresh_widgets(&self) -> Result<()> {
        if self.profile_id.is_none() {
            if let Some(manager) = self.device_manager() {
                for runtime in manager.runtimes() {
                    runtime.core.refresh_widgets()?;
                }
            }
        }
        let (targets, expired) = {
            let mut i = self.inner.lock().unwrap();
            let mut expired = false;
            for record in i.widgets.samples.values_mut() {
                if (record.sample.data.valid
                    || record.sample.data.presentations.values().any(|c| c.valid))
                    && record.received.elapsed() >= Duration::from_secs(5)
                {
                    record.sample.data.valid = false;
                    for content in record.sample.data.presentations.values_mut() {
                        content.valid = false;
                    }
                    expired = true;
                }
            }
            let mut targets = vec![];
            if i.widgets.page_ready
                && i.snapshot
                    .selected_device
                    .as_ref()
                    .is_some_and(|id| supports_images(&i, id))
            {
                if let Some(device) = i.snapshot.selected_device.as_ref() {
                    if let Some(path) = i.paths.get(device) {
                        if let Some(page) = i
                            .snapshot
                            .config
                            .pages
                            .iter()
                            .find(|p| p.id == i.snapshot.active_page)
                        {
                            for key in page
                                .keys
                                .iter()
                                .filter(|k| self.layout.display_keys.contains(&k.key))
                            {
                                if i.overrides.contains_key(&format!("key:{}", key.key)) {
                                    continue;
                                }
                                if let Some(binding) = &key.widget {
                                    if let Some(data) = data_for(&i, binding) {
                                        targets.push((
                                            key.key,
                                            binding.clone(),
                                            data,
                                            device.clone(),
                                            path.clone(),
                                            i.epoch,
                                            i.source_epochs
                                                .get(&binding.plugin_id)
                                                .copied()
                                                .unwrap_or(0),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let keys: Vec<_> = targets.iter().map(|t| t.0).collect();
            i.widgets.pending.retain(|key, _| keys.contains(key));
            i.widgets.last.retain(|key, _| keys.contains(key));
            (targets, expired)
        };
        if expired {
            self.emit_widgets();
        }
        for (key, binding, data, device_id, device_path, epoch, source_epoch) in targets {
            {
                let i = self.inner.lock().unwrap();
                if i.widgets
                    .last
                    .get(&key)
                    .is_some_and(|old| old.binding == binding && old.data == data)
                {
                    continue;
                }
            }
            let png = Arc::new(render::widget_sized(
                &binding,
                &data,
                self.display_size(key).0,
                self.display_size(key).1,
            )?);
            let frame = WidgetFrame {
                key,
                png,
                binding: binding.clone(),
                device_path,
                data,
            };
            let work = Work {
                image_frame: None,
                icon_revision: None,
                widget_frame: Some(frame.clone()),
                source: Some(binding.plugin_id.clone()),
                source_epoch,
                page_batch: None,
                receipt_id: None,
                device_path: frame.device_path.clone(),
                connection_epoch: self.inner.lock().unwrap().connection_epoch,
                device_id,
                epoch: Some(epoch),
                command: HardwareCommand::KeyData {
                    key,
                    data: String::new(),
                },
                led_command: None,
            };
            let mut i = self.inner.lock().unwrap();
            if i.epoch != epoch || !frame_is_current(&i, &work) {
                continue;
            }
            if i.widgets
                .last
                .get(&key)
                .is_some_and(|old| old.png == frame.png && old.binding == frame.binding)
            {
                i.widgets.last.insert(key, frame);
                continue;
            }
            i.widgets.last.insert(key, frame);
            i.widgets.pending.insert(key, work);
        }
        self.wake.notify_one();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(Arc<Core>);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("m18-widgets-{}", uuid::Uuid::new_v4()));
            let core = Core::new_test_device(root).unwrap();
            let manifest: PluginManifest =
                serde_json::from_str(include_str!("../../plugins/performance/plugin.json"))
                    .unwrap();
            {
                let mut i = core.inner.lock().unwrap();
                i.snapshot.plugins.push(PluginInfo { ui_message: None,
                    manifest,
                    path: "unused".into(),
                    status: "running".into(),
                    error: None,
                    built_in: true,
                });
                i.snapshot.selected_device = Some("device".into());
                i.paths.insert("device".into(), "path-1".into());
                i.snapshot.devices.push(DeviceDescriptor {
                    adapter_id: crate::devices::default_adapter_id().into(),
                    layout: crate::devices::default_layout().into(),
                    id: "device".into(),
                    model: "M18".into(),
                    serial: "test".into(),
                    firmware: "v3".into(),
                    vendor_id: 0x6603,
                    product_id: 0x1012,
                    capabilities: crate::devices::default_capabilities("v3"),
                });
                let style = i.snapshot.plugins[0].manifest.widgets[0]
                    .default_style
                    .clone();
                for key in i.snapshot.config.pages[0].keys.iter_mut().take(2) {
                    key.widget = Some(WidgetBinding {
                        source_id: None,
                        source_name: None,
                        presentation_id: None,
                        plugin_id: "dev.masterain.performance".into(),
                        widget_id: "cpu".into(),
                        style: style.clone(),
                    });
                }
                i.widgets.page_ready = true;
            }
            Self(core)
        }
        fn publish(&self, value: u32) {
            self.0
                .publish_widget(
                    "dev.masterain.performance",
                    WidgetData {
                        source_id: None,
                        secondary_value: None,
                        presentations: Default::default(),
                        widget_id: "cpu".into(),
                        title: "CPU".into(),
                        value: format!("{value}%"),
                        detail: None,
                        progress: Some(f64::from(value) / 100.0),
                        valid: true,
                    },
                )
                .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0.root);
        }
    }
    #[test]
    fn sources_are_isolated_bounded_removed_and_saved_without_rebinding() {
        let f = Fixture::new();
        let owner = "dev.masterain.performance";
        let catalog = WidgetCatalog { ui_message: None,
            hardware_status: "ready".into(),
            message: None,
            sources: ["adapter-a", "adapter-b"]
                .into_iter()
                .map(|id| WidgetSource { ui_message: None,
                    widget_id: "network".into(),
                    id: id.into(),
                    name: format!("网卡 {id}"),
                    available: true,
                    error: None,
                })
                .collect(),
        };
        f.0.publish_widget_catalog(owner, catalog.clone()).unwrap();
        let mut pages = f.0.snapshot().config.pages;
        for (key, id) in pages[0]
            .keys
            .iter_mut()
            .zip(["adapter-a", "adapter-b", "adapter-a"])
        {
            let binding = key.widget.get_or_insert_with(|| WidgetBinding {
                plugin_id: owner.into(),
                widget_id: "network".into(),
                style: f.0.snapshot().plugins[0].manifest.widgets[0]
                    .default_style
                    .clone(),
                presentation_id: None,
                source_id: None,
                source_name: None,
            });
            binding.widget_id = "network".into();
            binding.source_id = Some(id.into());
            binding.source_name = Some(format!("网卡 {id}"));
        }
        let sample = |id: &str, value: &str| WidgetData {
            widget_id: "network".into(),
            source_id: Some(id.into()),
            title: "网卡".into(),
            value: value.into(),
            secondary_value: Some("0 B/s".into()),
            detail: None,
            progress: None,
            valid: true,
            presentations: Default::default(),
        };
        f.0.publish_widgets(
            owner,
            vec![
                sample("adapter-a", "1 MiB/s"),
                sample("adapter-b", "2 MiB/s"),
            ],
        )
        .unwrap();
        let values: Vec<_> = pages[0]
            .keys
            .iter()
            .take(3)
            .map(|k| f.0.widget_data(k).unwrap().value)
            .collect();
        assert_eq!(values, ["1 MiB/s", "2 MiB/s", "1 MiB/s"]);
        assert!(f
            .0
            .publish_widgets(
                owner,
                vec![sample("adapter-a", "bad"), sample("missing", "invalid")]
            )
            .is_err());
        assert_eq!(f.0.widget_data(&pages[0].keys[0]).unwrap().value, "1 MiB/s");
        assert!(f
            .0
            .publish_widgets(
                owner,
                vec![sample("adapter-a", "1"), sample("adapter-a", "2")]
            )
            .is_err());
        f.0.update_pages(pages.clone()).unwrap();
        assert_eq!(
            storage::test_config(&f.0.root).unwrap().pages[0].keys[0].widget,
            pages[0].keys[0].widget
        );
        let mut removed = catalog.clone();
        removed.sources.remove(0);
        f.0.publish_widget_catalog(owner, removed).unwrap();
        assert_eq!(f.0.widget_samples().len(), 1);
        assert!(!f.0.widget_data(&pages[0].keys[0]).unwrap().valid);
        assert_eq!(f.0.widget_data(&pages[0].keys[1]).unwrap().value, "2 MiB/s");
        f.0.publish_widget_catalog(owner, catalog.clone()).unwrap();
        assert!(!f.0.widget_data(&pages[0].keys[0]).unwrap().valid);
        f.0.publish_widget(owner, sample("adapter-a", "0 B/s"))
            .unwrap();
        assert!(f.0.widget_data(&pages[0].keys[0]).unwrap().valid);
        let mut duplicate = catalog.clone();
        duplicate.sources.push(duplicate.sources[0].clone());
        assert!(f.0.publish_widget_catalog(owner, duplicate).is_err());
        let mut invalid = catalog;
        invalid.sources[0].id = "../device".into();
        assert!(f.0.publish_widget_catalog(owner, invalid).is_err());
        f.0.cancel_owner(owner);
        assert!(f.0.widget_catalog(owner).sources.is_empty());
        assert!(f.0.widget_samples().is_empty());
    }
    #[test]
    fn old_configs_and_manifests_load_and_binding_validation_is_strict() {
        let config = crate::devices::default_config();
        let mut json = serde_json::to_value(config).unwrap();
        json.as_object_mut().unwrap().remove("disabled_plugins");
        for key in json["pages"][0]["keys"].as_array_mut().unwrap() {
            key.as_object_mut().unwrap().remove("widget");
        }
        let mut old: AppConfig = serde_json::from_value(json).unwrap();
        assert!(old.disabled_plugins.is_empty());
        assert!(old.pages[0].keys.iter().all(|k| k.widget.is_none()));
        let manifest: PluginManifest =
            serde_json::from_str(include_str!("../../examples/counter/plugin.json")).unwrap();
        assert!(manifest.widgets.is_empty());
        let f = Fixture::new();
        old.pages[0].keys[15].widget = f.0.snapshot().config.pages[0].keys[0].widget.clone();
        assert!(storage::validate(&old).is_err());
        old.pages[0].keys[15].widget = None;
        storage::validate(&old).unwrap();
        let mut binding = f.0.snapshot().config.pages[0].keys[0]
            .widget
            .clone()
            .unwrap();
        binding.style.accent = "bad".into();
        assert!(validate_binding(&binding).is_err());
    }
    #[test]
    fn concurrent_baseline_preparation_never_exposes_partial_images() {
        let f = Fixture::new();
        let visual = f.0.snapshot().config.pages[0].keys[0].clone();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    for _ in 0..20 {
                        let path = render::key(&f.0.root, &visual).unwrap();
                        image::open(&path).unwrap();
                        let id = storage::import(&f.0.root, &path).unwrap();
                        image::open(storage::asset(&f.0.root, &id).unwrap()).unwrap();
                        let background = render::blank_background(&f.0.root).unwrap();
                        image::open(background).unwrap();
                    }
                });
            }
        });
        assert!(std::fs::read_dir(f.0.root.join("cache")).unwrap().all(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|e| e == "png")));
    }
    #[test]
    fn plugin_owned_presentations_are_selected_per_binding_saved_and_expire() {
        let f = Fixture::new();
        let mut sample = WidgetData {
            source_id: None,
            secondary_value: None,
            widget_id: "memory".into(),
            title: "内存".into(),
            value: "63%".into(),
            detail: Some("20.0/32.0 GiB".into()),
            progress: Some(0.625),
            valid: true,
            presentations: BTreeMap::new(),
        };
        for (id, title, value, progress) in [
            ("available", "剩余内存", "12.0", 0.375),
            ("used", "已用内存", "20.0", 0.625),
        ] {
            sample.presentations.insert(
                id.into(),
                WidgetContent {
                    secondary_value: None,
                    title: title.into(),
                    value: value.into(),
                    detail: Some("/32.0 GiB".into()),
                    progress: Some(progress),
                    valid: true,
                },
            );
        }
        let mut invalid = sample.clone();
        invalid
            .presentations
            .insert("undeclared".into(), invalid.presentations["used"].clone());
        assert!(f
            .0
            .publish_widget("dev.masterain.performance", invalid)
            .is_err());
        f.0.publish_widget("dev.masterain.performance", sample.clone())
            .unwrap();
        let mut pages = f.0.snapshot().config.pages;
        for (key, presentation) in pages[0]
            .keys
            .iter_mut()
            .zip(["percent", "available", "used"])
        {
            key.widget = Some(WidgetBinding {
                source_id: None,
                source_name: None,
                plugin_id: "dev.masterain.performance".into(),
                widget_id: "memory".into(),
                style: f.0.snapshot().plugins[0].manifest.widgets[1]
                    .default_style
                    .clone(),
                presentation_id: Some(presentation.into()),
            });
        }
        f.0.update_pages(pages.clone()).unwrap();
        let loaded = storage::test_config(&f.0.root).unwrap();
        assert_eq!(loaded.pages[0].keys[1].widget, pages[0].keys[1].widget);
        let selected: Vec<_> = pages[0]
            .keys
            .iter()
            .take(3)
            .map(|v| f.0.widget_data(v).unwrap())
            .collect();
        assert_eq!(
            selected
                .iter()
                .map(|v| v.value.as_str())
                .collect::<Vec<_>>(),
            ["63%", "12.0", "20.0"]
        );
        let mut unknown = pages[0].keys[1].clone();
        unknown.widget.as_mut().unwrap().presentation_id = Some("removed-option".into());
        assert!(!f.0.widget_data(&unknown).unwrap().valid);
        invalid = sample;
        invalid.presentations.get_mut("used").unwrap().progress = Some(f64::NAN);
        assert!(validate_data(&invalid).is_err());
        f.0.inner
            .lock()
            .unwrap()
            .widgets
            .samples
            .values_mut()
            .next()
            .unwrap()
            .received = Instant::now() - Duration::from_secs(6);
        f.0.refresh_widgets().unwrap();
        for visual in pages[0].keys.iter().take(3) {
            assert_eq!(f.0.widget_data(visual).unwrap().value, "—");
        }
    }
    #[test]
    fn unavailable_image_capability_blocks_pending_and_preparing_frames() {
        let f = Fixture::new();
        f.publish(42);
        f.0.refresh_widgets().unwrap();
        let work = f.0.pop().unwrap();
        assert!(f.0.work_is_current(&work));
        f.0.inner.lock().unwrap().snapshot.devices[0]
            .capabilities
            .get_mut("key_images")
            .unwrap()
            .available = false;
        assert!(!f.0.work_is_current(&work));
        f.0.refresh_widgets().unwrap();
        assert!(f.0.inner.lock().unwrap().widgets.pending.is_empty());
        f.0.inner.lock().unwrap().snapshot.devices[0]
            .capabilities
            .get_mut("key_images")
            .unwrap()
            .available = true;
        f.0.refresh_widgets().unwrap();
        assert!(f.0.pop().is_some_and(|w| w.widget_frame.is_some()));
    }
    #[test]
    fn duplicate_bindings_coalesce_without_assets_or_receipts_and_unchanged_frames_are_skipped() {
        let f = Fixture::new();
        let assets_before = std::fs::read_dir(f.0.root.join("assets")).unwrap().count();
        let cache_before = std::fs::read_dir(f.0.root.join("cache"))
            .map(|files| files.count())
            .unwrap_or(0);
        for value in 0..150 {
            f.publish(value % 101);
            f.0.refresh_widgets().unwrap();
            let i = f.0.inner.lock().unwrap();
            assert_eq!(i.widgets.pending.len(), 2);
            assert_eq!(i.widgets.samples.len(), 1);
            assert!(i.queue.is_empty());
            assert!(i.snapshot.receipts.is_empty());
        }
        let first = f.0.pop().unwrap();
        assert_eq!(first.widget_frame.as_ref().unwrap().data.value, "48%");
        assert!(f.0.work_is_current(&first));
        assert!(f.0.pop().unwrap().widget_frame.is_some());
        f.0.refresh_widgets().unwrap();
        assert!(f.0.pop().is_none());
        assert_eq!(
            std::fs::read_dir(f.0.root.join("assets")).unwrap().count(),
            assets_before
        );
        assert_eq!(
            std::fs::read_dir(f.0.root.join("cache"))
                .map(|files| files.count())
                .unwrap_or(0),
            cache_before
        );
    }
    #[tokio::test]
    async fn page_batch_precedes_widgets_and_late_frames_cannot_cross_pages_or_device_paths() {
        let f = Fixture::new();
        f.publish(32);
        f.0.refresh_widgets().unwrap();
        let old = f.0.pop().unwrap();
        let mut pages = f.0.snapshot().config.pages;
        let other = crate::devices::default_page("Other");
        let other_id = other.id.clone();
        pages.push(other);
        f.0.update_pages(pages).unwrap();
        f.0.activate(&other_id).await.unwrap();
        assert!(!f.0.work_is_current(&old));
        f.0.refresh_widgets().unwrap();
        assert!(f.0.inner.lock().unwrap().widgets.pending.is_empty());
        let home = f.0.snapshot().config.pages[0].id.clone();
        f.0.activate(&home).await.unwrap();
        f.0.refresh_widgets().unwrap();
        let batch = f.0.pop_batch();
        assert!(batch.len() > 15 && batch.iter().all(|w| w.page_batch.is_some()));
        assert_eq!(
            batch
                .iter()
                .filter(|w| matches!(w.command, HardwareCommand::Refresh))
                .count(),
            1
        );
        let frame = f.0.pop().unwrap();
        assert!(f.0.work_is_current(&frame));
        f.0.inner
            .lock()
            .unwrap()
            .paths
            .insert("device".into(), "path-2".into());
        assert!(!f.0.work_is_current(&frame));
    }
    #[tokio::test]
    async fn temporary_overrides_win_and_stopping_restores_base_without_late_republication() {
        let f = Fixture::new();
        f.publish(42);
        f.0.refresh_widgets().unwrap();
        let preparing = f.0.pop().unwrap();
        f.0.plugin_command("mcp", HardwareCommand::ClearKey { key: 1 }, Some(0))
            .unwrap();
        assert!(!f.0.work_is_current(&preparing));
        f.0.refresh_widgets().unwrap();
        assert!(!f.0.inner.lock().unwrap().widgets.pending.contains_key(&1));
        f.0.restore_plugin("mcp").await.unwrap();
        f.0.refresh_widgets().unwrap();
        assert!(f.0.inner.lock().unwrap().widgets.pending.contains_key(&1));
        f.0.pop_batch(); // Whole base page, before either dynamic key.
        let preparing = f.0.pop().unwrap();
        f.0.inner.lock().unwrap().snapshot.plugins[0].status = "stopping".into();
        f.0.cancel_owner("dev.masterain.performance");
        assert!(!f.0.work_is_current(&preparing));
        assert!(f
            .0
            .publish_widget(
                "dev.masterain.performance",
                preparing.widget_frame.unwrap().data
            )
            .is_err());
        f.0.restore_plugin("dev.masterain.performance")
            .await
            .unwrap();
        f.0.refresh_widgets().unwrap();
        assert!(f.0.inner.lock().unwrap().widgets.pending.is_empty());
        assert!(f
            .0
            .widget_data(&f.0.snapshot().config.pages[0].keys[0])
            .is_none());
    }
    #[test]
    fn stale_and_failed_samples_never_look_like_zero_and_unbinding_cancels_preparing_work() {
        let f = Fixture::new();
        f.publish(42);
        {
            let mut i = f.0.inner.lock().unwrap();
            i.widgets.samples.values_mut().next().unwrap().received =
                Instant::now() - Duration::from_secs(6);
        }
        f.0.refresh_widgets().unwrap();
        assert!(!f.0.widget_samples()[0].data.valid);
        let work = f.0.pop().unwrap();
        assert_eq!(work.widget_frame.as_ref().unwrap().data.value, "—");
        let mut pages = f.0.snapshot().config.pages;
        pages[0].keys[0].widget = None;
        f.0.update_pages(pages).unwrap();
        assert!(!f.0.work_is_current(&work));
        assert!(f.0.inner.lock().unwrap().widgets.pending.is_empty());
        f.0.clear_connection();
        assert!(f.0.inner.lock().unwrap().widgets.pending.is_empty());
    }
}
