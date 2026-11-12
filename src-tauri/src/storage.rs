use crate::model::*;
use anyhow::{bail, Context, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn validate_app_settings(settings: &AppSettings) -> Result<()> {
    if settings.model_appearances.len() > 100 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b210",
            "模型外观设置数量超过限制"
        ))
    }
    for (profile_id, appearance) in &settings.model_appearances {
        if profile_id.is_empty()
            || profile_id.len() > 128
            || profile_id.chars().any(char::is_control)
        {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b211",
                "模型外观的设备配置标识无效"
            ))
        }
        match appearance {
            ModelAppearance::Default => {}
            ModelAppearance::Text { text } => {
                if text.trim().is_empty()
                    || text.chars().count() > 32
                    || text.chars().any(char::is_control)
                {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b212",
                        "模型标识文字须为 1–32 个字符，且不能包含换行或控制字符"
                    ))
                }
            }
            ModelAppearance::Image {
                image: ImageRef::Asset { asset_id },
            } => {
                validate_asset_id(asset_id)?;
                if asset_id.ends_with(".gif") {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b213",
                        "模型标识只支持静态图片"
                    ))
                }
            }
            ModelAppearance::Image {
                image: ImageRef::PluginIcon { plugin_id, icon_id },
            } => {
                crate::plugin_icons::validate_ref(&PluginIconRef {
                    plugin_id: plugin_id.clone(),
                    icon_id: icon_id.clone(),
                })?;
            }
        }
    }
    Ok(())
}

pub fn load(root: &Path) -> Result<AppConfig> {
    std::fs::create_dir_all(root.join("assets"))?;
    initialize_workspace(root)?;
    Ok(global_as_config(&load_workspace(root)?.global))
}

pub fn validate(c: &AppConfig) -> Result<()> {
    validate_layout(c, &crate::devices::default_layout())
}

pub fn validate_layout(c: &AppConfig, layout: &DeviceLayout) -> Result<()> {
    validate_device_config(
        c,
        &DeviceDefinition {
            inputs: vec![],
            displays: vec![],
            adapter_id: String::new(),
            name: String::new(),
            layout: layout.clone(),
            supports_pages: !layout.display_keys.is_empty(),
            input_names: Default::default(),
        },
    )
}
pub fn validate_definition(definition: &DeviceDefinition) -> Result<()> {
    let mut controls = std::collections::HashSet::new();
    let mut displays = std::collections::HashSet::new();
    let mut slots = std::collections::HashSet::new();
    for control in &definition.inputs {
        if control.id.is_empty() || control.id.len() > 128 || !controls.insert(&control.id) {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b214",
                "输入控件标识无效或重复"
            ))
        }
        if let alchemy_device_api::InputKind::Analog { min, max, .. } = &control.kind {
            if !min.is_finite() || !max.is_finite() || max <= min {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b215",
                    "模拟量范围无效"
                ))
            }
        }
    }
    for display in &definition.displays {
        if display.id.is_empty()
            || !displays.insert(&display.id)
            || !slots.insert(display.key)
            || display.width == 0
            || display.height == 0
            || !definition.layout.display_keys.contains(&display.key)
        {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b216",
                "显示区域标识、尺寸或位置无效"
            ))
        }
    }
    Ok(())
}
pub fn validate_device_config(c: &AppConfig, definition: &DeviceDefinition) -> Result<()> {
    let normalized = definition.clone().normalized();
    let definition = &normalized;
    validate_definition(definition)?;
    validate_bindings(&c.bindings, definition)?;
    for page in &c.pages {
        validate_bindings(&page.bindings, definition)?;
    }
    let layout = &definition.layout;
    if !definition.supports_pages
        && (!c.pages.is_empty() || c.scene_settings.enabled || !c.scene_settings.scenes.is_empty())
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b217",
            "此设备不支持页面或场景"
        ))
    }

    if c.managed_plugin_ids
        .iter()
        .any(|id| id.eq_ignore_ascii_case("dev.masterain.performance"))
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b218",
            "内置插件不能注册为外部插件"
        ));
    }
    for list in [&c.managed_plugin_ids, &c.disabled_plugins] {
        let mut ids = std::collections::HashSet::new();
        for id in list {
            crate::plugin_package::validate_id(id)?;
            if !ids.insert(id.to_ascii_lowercase()) {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b219",
                    "插件 ID 重复"
                ));
            }
        }
    }

    if c.version != 1 {
        bail!(UiFailure::new(
            "errors.configVersion",
            format!("不支持配置版本 {}", c.version)
        )
        .arg("version", c.version))
    }
    if (c.pages.is_empty() && definition.supports_pages) || c.pages.len() > 200 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b220",
            "页面数量须为 1–200"
        ))
    }
    let mut ids = std::collections::HashSet::new();
    for page in &c.pages {
        if !ids.insert(&page.id) || page.id.is_empty() || page.name.trim().is_empty() {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b221",
                "页面名称或 ID 无效"
            ))
        }
        let expected_keys: std::collections::BTreeSet<_> = layout
            .input_keys
            .iter()
            .chain(&layout.display_keys)
            .copied()
            .collect();
        if page.keys.len() != expected_keys.len() {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b222",
                "页面按键数量与设备布局不一致"
            ))
        }
        let mut keys = std::collections::HashSet::new();
        for k in &page.keys {
            if let Some(icon) = &k.plugin_icon {
                crate::plugin_icons::validate_ref(icon)?;
                if !layout.display_keys.contains(&k.key)
                    || k.asset_id.is_some()
                    || k.default_icon_asset_id.is_some()
                {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b223",
                        "插件图标只用于显示键，不能与个人图片或默认应用图标同时设置"
                    ));
                }
            }
            if let Some(binding) = &k.widget {
                if !layout.display_keys.contains(&k.key) {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b224",
                        "底部物理键不支持插件显示"
                    ))
                }
                crate::widgets::validate_binding(binding)?;
            }
            if let Some(id) = &k.default_icon_asset_id {
                validate_asset_id(id)?;
            }
            if !expected_keys.contains(&k.key) || !keys.insert(k.key) {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b225",
                    "按键编号无效"
                ))
            };
            color(&k.foreground)?;
            color(&k.background)?;
            if k.text.chars().count() > 80 {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b226",
                    "按键文字不能超过 80 字"
                ))
            }
        }
    }
    if !c.pages.is_empty() && !ids.contains(&c.active_page) {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b227",
            "当前页面不存在"
        ))
    }
    for binding in c
        .bindings
        .iter()
        .chain(c.pages.iter().flat_map(|p| p.bindings.iter()))
    {
        if let KeyAction::Page { page_id } = &binding.action {
            if !ids.contains(page_id) {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b228",
                    "页面跳转目标不存在"
                ))
            }
        }
    }
    crate::scenes::validate(&c.scene_settings, &c.pages)?;
    validate_led_settings_count(&c.device_settings, layout.led_count as usize)?;
    if c.device_settings.screen_brightness > 100 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b229",
            "屏幕亮度须为 0–100"
        ))
    }
    if !["system", "light", "dark"].contains(&c.theme.as_str()) {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b230",
            "主题无效"
        ))
    }
    Ok(())
}

pub fn validate_bindings(
    bindings: &[alchemy_device_api::ControlBinding],
    definition: &DeviceDefinition,
) -> Result<()> {
    use alchemy_device_api::{InputKind, InputTrigger};
    if bindings.len() > 4096 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b231",
            "输入绑定数量超限"
        ))
    }
    for binding in bindings {
        let control = definition
            .inputs
            .iter()
            .find(|c| c.id == binding.control_id)
            .context(alchemy_device_api::UiFailure::new(
                "backend.b232",
                "未知输入控件",
            ))?;
        match (&control.kind, &binding.trigger) {
            (InputKind::Button, InputTrigger::Press)
            | (InputKind::Rotary, InputTrigger::Clockwise | InputTrigger::Counterclockwise) => {}
            (
                InputKind::Analog { min, max, .. },
                InputTrigger::Above {
                    threshold,
                    hysteresis,
                }
                | InputTrigger::Below {
                    threshold,
                    hysteresis,
                },
            ) => {
                if !min.is_finite()
                    || !max.is_finite()
                    || max <= min
                    || !threshold.is_finite()
                    || !hysteresis.is_finite()
                    || *threshold <= 0.0
                    || *threshold >= 1.0
                    || *hysteresis < 0.0
                    || *hysteresis > threshold.min(1.0 - threshold)
                {
                    bail!(alchemy_device_api::UiFailure::new(
                        "backend.b233",
                        "模拟量范围、阈值或回差无效"
                    ))
                }
            }
            _ => bail!(alchemy_device_api::UiFailure::new(
                "backend.b234",
                "输入触发器与控件类型不匹配"
            )),
        }
        validate_action(&binding.action, definition.supports_pages)?;
    }
    Ok(())
}
pub fn validate_action(action: &KeyAction, pages: bool) -> Result<()> {
    if let Some(target) = crate::launch_service::action_target(action) {
        return crate::launch_service::validate(&target);
    }
    if !pages && !matches!(action, KeyAction::None) {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b235",
            "无页面设备不支持导航动作"
        ))
    }
    Ok(())
}
pub fn validate_led_settings(settings: &DeviceSettings) -> Result<()> {
    validate_led_settings_count(
        settings,
        crate::devices::default_layout().led_count as usize,
    )
}
pub fn validate_led_settings_count(settings: &DeviceSettings, count: usize) -> Result<()> {
    color(&settings.led_color)?;
    if settings.led_colors.len() != count {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b048",
            "逐灯颜色数量与设备不一致"
        ))
    }
    if !(1..=10).contains(&settings.led_speed) {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b236",
            "LED 灯效速度须为 1–10"
        ))
    }
    Ok(())
}
pub fn save(root: &Path, c: &AppConfig) -> Result<()> {
    let _guard = WORKSPACE_LOCK.lock().unwrap();
    let mut workspace = load_workspace_unlocked(root)?;
    let mut next = global_from_config(c);
    next.extensions = workspace.global.extensions.clone();
    workspace.global = next;
    validate_workspace(&workspace)?;
    save_json(root, "config.json", &workspace)
}

pub(crate) fn save_json(root: &Path, name: &str, value: &impl serde::Serialize) -> Result<()> {
    write_atomic(root, name, &serde_json::to_vec_pretty(value)?)
}

pub(crate) fn write_atomic(root: &Path, name: &str, data: &[u8]) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let tmp = root.join(format!("{name}.tmp"));
    let target = root.join(name);
    use std::io::Write;
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    replace_file(&tmp, &target)
}

pub(crate) fn replace_file(tmp: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let a: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(tmp, target)?;
    Ok(())
}
/// Publish immutable image bytes only after the complete file has been written.
/// Unique staging files allow page preparation and previews to run concurrently.
pub(crate) fn publish_image(root: &Path, target: &Path, data: &[u8]) -> Result<()> {
    if std::fs::read(target).ok().as_deref() == Some(data) {
        return Ok(());
    }
    let staging = root.join("cache");
    std::fs::create_dir_all(&staging)?;
    let temporary = staging.join(format!("image-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        std::fs::write(&temporary, data)?;
        match replace_file(&temporary, target) {
            Ok(()) => Ok(()),
            // A concurrent writer may have published this same immutable image,
            // while a Windows decoder has it open without delete sharing.
            Err(_) if std::fs::read(target).ok().as_deref() == Some(data) => Ok(()),
            Err(error) => Err(error).with_context(|| format!("发布图片失败：{}", target.display())),
        }
    })();
    let _ = std::fs::remove_file(&temporary);
    result
}
pub fn color(s: &str) -> Result<[u8; 3]> {
    if s.len() != 7 || !s.starts_with('#') || !s.is_ascii() {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b237",
            "颜色须为 #RRGGBB"
        ))
    };
    Ok([
        u8::from_str_radix(&s[1..3], 16)?,
        u8::from_str_radix(&s[3..5], 16)?,
        u8::from_str_radix(&s[5..7], 16)?,
    ])
}
pub fn import(root: &Path, path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > 20 * 1024 * 1024 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b238",
            "图片不能超过 20 MB"
        ))
    };
    let data = std::fs::read(path)?;
    put_asset(root, &data)
}
pub fn validate_image(data: &[u8]) -> Result<&'static str> {
    if data.len() > 20 * 1024 * 1024 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b238",
            "图片不能超过 20 MB"
        ))
    }
    let format = image::guess_format(data)?;
    let ext = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Gif => "gif",
        _ => bail!(alchemy_device_api::UiFailure::new(
            "backend.b239",
            "仅支持 PNG、JPEG、GIF"
        )),
    };
    let reader = image::ImageReader::with_format(std::io::Cursor::new(data), format);
    let (w, h) = reader.into_dimensions()?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 || u64::from(w) * u64::from(h) > 32_000_000 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b240",
            "图片尺寸过大"
        ))
    }
    image::load_from_memory_with_format(data, format)?;
    Ok(ext)
}
pub fn put_asset(root: &Path, data: &[u8]) -> Result<String> {
    let ext = validate_image(data)?;
    let id = format!("{:x}.{ext}", Sha256::digest(data));
    std::fs::create_dir_all(root.join("assets"))?;
    let p = root.join("assets").join(&id);
    if !p.exists() {
        publish_image(root, &p, data)?;
    }
    Ok(id)
}
pub fn decode_data(data: &str) -> Result<Vec<u8>> {
    if data.len() > 28 * 1024 * 1024 {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b241",
            "图片数据过大"
        ))
    };
    let raw = data.split_once(",").map(|(_, b)| b).unwrap_or(data);
    Ok(base64::engine::general_purpose::STANDARD.decode(raw)?)
}
pub fn validate_asset_id(id: &str) -> Result<()> {
    if !id.is_ascii()
        || id.len() < 68
        || id.len() > 69
        || !id[..64].bytes().all(|b| b.is_ascii_hexdigit())
        || ![".png", ".jpg", ".gif"].contains(&&id[64..])
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b242",
            "素材 ID 无效"
        ))
    };
    Ok(())
}
pub fn asset(root: &Path, id: &str) -> Result<PathBuf> {
    validate_asset_id(id)?;
    let p = root.join("assets").join(id);
    if !p.is_file() {
        bail!(UiFailure::new("errors.assetMissing", format!("素材不存在：{id}")).arg("id", id))
    };
    Ok(p)
}
pub fn data_url(path: &Path) -> Result<String> {
    let mime = match path.extension().and_then(|s| s.to_str()) {
        Some("gif") => "image/gif",
        Some("jpg") => "image/jpeg",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(path)?)
    ))
}

// All configuration writers share this lock, including profile autosave and plugin transactions.
static WORKSPACE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn global_from_config(c: &AppConfig) -> GlobalConfig {
    GlobalConfig {
        extensions: Default::default(),
        theme: c.theme.clone(),
        managed_plugin_ids: c.managed_plugin_ids.clone(),
        disabled_plugins: c.disabled_plugins.clone(),
    }
}
pub fn global_as_config(global: &GlobalConfig) -> AppConfig {
    AppConfig {
        theme: global.theme.clone(),
        managed_plugin_ids: global.managed_plugin_ids.clone(),
        disabled_plugins: global.disabled_plugins.clone(),
        ..AppConfig::empty()
    }
}
pub const WORKSPACE_FORMAT: &str = "desktop-alchemy.workspace";
pub fn decode_workspace(bytes: &[u8]) -> Result<WorkspaceConfig> {
    let workspace: WorkspaceConfig = serde_json::from_slice(bytes).context(
        alchemy_device_api::UiFailure::new("backend.b243", "工作区格式无效，原文件已保留"),
    )?;
    validate_workspace(&workspace)?;
    Ok(workspace)
}
pub fn validate_workspace(workspace: &WorkspaceConfig) -> Result<()> {
    if workspace.format != WORKSPACE_FORMAT
        || workspace.version != 1
        || workspace.profiles.len() > 100
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b244",
            "设备配置版本或数量无效"
        ))
    }
    if !["system", "light", "dark"].contains(&workspace.global.theme.as_str()) {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b230",
            "主题无效"
        ))
    }
    if workspace
        .global
        .managed_plugin_ids
        .iter()
        .any(|id| id.eq_ignore_ascii_case("dev.masterain.performance"))
    {
        bail!(alchemy_device_api::UiFailure::new(
            "backend.b218",
            "内置插件不能注册为外部插件"
        ))
    }
    for list in [
        &workspace.global.managed_plugin_ids,
        &workspace.global.disabled_plugins,
    ] {
        let mut ids = std::collections::HashSet::new();
        for id in list {
            crate::plugin_package::validate_id(id)?;
            if !ids.insert(id.to_ascii_lowercase()) {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b219",
                    "插件 ID 重复"
                ))
            }
        }
    }
    let mut ids = std::collections::HashSet::new();
    let mut devices = std::collections::HashSet::new();
    for profile in &workspace.profiles {
        if profile.id.is_empty() || !ids.insert(&profile.id) || profile.adapter_id.is_empty() {
            bail!(alchemy_device_api::UiFailure::new(
                "backend.b245",
                "设备配置标识或版本无效"
            ))
        }
        if let Some(id) = &profile.device_id {
            if id.is_empty() || !devices.insert(id) {
                bail!(alchemy_device_api::UiFailure::new(
                    "backend.b246",
                    "设备重复绑定"
                ))
            }
        }
    }
    Ok(())
}
pub fn load_workspace(root: &Path) -> Result<WorkspaceConfig> {
    let _guard = WORKSPACE_LOCK.lock().unwrap();
    load_workspace_unlocked(root)
}
fn load_workspace_unlocked(root: &Path) -> Result<WorkspaceConfig> {
    let path = root.join("config.json");
    if path.exists() {
        decode_workspace(&std::fs::read(path)?)
    } else {
        Ok(WorkspaceConfig {
            format: WORKSPACE_FORMAT.into(),
            extensions: Default::default(),
            version: 1,
            global: GlobalConfig::default(),
            profiles: vec![],
        })
    }
}
pub fn save_workspace(root: &Path, workspace: &WorkspaceConfig) -> Result<()> {
    let _guard = WORKSPACE_LOCK.lock().unwrap();
    validate_workspace(workspace)?;
    save_json(root, "config.json", workspace)
}
pub fn edit_workspace(
    root: &Path,
    edit: impl FnOnce(&mut WorkspaceConfig) -> Result<()>,
) -> Result<()> {
    let _guard = WORKSPACE_LOCK.lock().unwrap();
    let mut workspace = load_workspace_unlocked(root)?;
    let previous = serde_json::to_value(&workspace)?;
    edit(&mut workspace)?;
    validate_workspace(&workspace)?;
    if serde_json::to_value(&workspace)? == previous {
        return Ok(());
    }
    save_json(root, "config.json", &workspace)
}
pub fn save_profile(root: &Path, profile_id: &str, config: &AppConfig) -> Result<()> {
    edit_workspace(root, |workspace| {
        let profile = workspace
            .profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .context(alchemy_device_api::UiFailure::new(
                "backend.b041",
                "未知设备配置",
            ))?;
        profile.config = config.clone();
        Ok(())
    })
}

pub fn initialize_workspace(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    if root.join("config.json").exists() {
        load_workspace(root)?;
        return Ok(());
    }
    save_workspace(
        root,
        &WorkspaceConfig {
            format: WORKSPACE_FORMAT.into(),
            version: 1,
            extensions: Default::default(),
            global: GlobalConfig::default(),
            profiles: vec![],
        },
    )
}

#[cfg(test)]
pub(crate) fn test_config(root: &Path) -> Result<AppConfig> {
    let workspace = load_workspace(root)?;
    Ok(workspace
        .profiles
        .first()
        .map(|p| p.config.clone())
        .unwrap_or_else(|| global_as_config(&workspace.global)))
}
#[cfg(test)]
pub(crate) fn test_save_config(root: &Path, config: &AppConfig) -> Result<()> {
    validate(config)?;
    initialize_workspace(root)?;
    edit_workspace(root, |w| {
        w.global = global_from_config(config);
        if let Some(profile) = w.profiles.first_mut() {
            profile.config = config.clone();
        } else {
            w.profiles.push(DeviceProfile::new(
                crate::devices::default_adapter_id().into(),
                "test".into(),
                None,
                config.clone(),
            ));
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_config() {
        let mut c = crate::devices::default_config();
        c.pages.clear();
        assert!(validate(&c).is_err());
        let mut c = crate::devices::default_config();
        c.pages[0].keys[0].key = 18;
        assert!(validate(&c).is_err());
        let mut c = crate::devices::default_config();
        c.device_settings.led_speed = 0;
        assert!(validate(&c).is_err());
        c.device_settings.led_speed = 11;
        assert!(validate(&c).is_err());
    }
    #[test]
    fn atomic_roundtrip() {
        let p = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let mut c = crate::devices::default_config();
        c.pages[0].keys[0].content_mode = Some(KeyContentMode::Text);
        c.pages[0].keys[0].show_text = false;
        c.pages[0].keys[0].text_position = TextPosition::Top;
        test_save_config(&p, &c).unwrap();
        assert_eq!(
            serde_json::to_value(test_config(&p).unwrap()).unwrap(),
            serde_json::to_value(c).unwrap()
        );
        std::fs::remove_file(p.join("config.json")).unwrap();
    }
    #[test]
    fn blocks_paths() {
        assert!(asset(Path::new("."), "../config.json").is_err());
        assert!(color("#FFFFFF").is_ok());
        assert!(color("<svg>").is_err());
    }
}
