//! All M18-specific protocol, firmware and default layout rules.
use alchemy_device_api::*;
use std::collections::BTreeMap;
#[cfg(windows)]
pub mod native_sdk;
pub mod sdk;
pub const ID: &str = "mirabox.m18";
pub fn layout() -> DeviceLayout {
    DeviceLayout {
        input_keys: (1..=18).collect(),
        display_keys: (1..=15).collect(),
        columns: 5,
        key_width: 64,
        key_height: 64,
        background_width: 480,
        background_height: 272,
        led_count: 24,
    }
}
pub fn new_page(name: &str) -> Page {
    Page {
        bindings: (1..=18)
            .map(|key| ControlBinding {
                control_id: key.to_string(),
                enabled: true,
                trigger: InputTrigger::Press,
                action: match key {
                    16 => KeyAction::Previous,
                    17 => KeyAction::Home,
                    18 => KeyAction::Next,
                    _ => KeyAction::None,
                },
            })
            .collect(),
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        background_asset: None,
        keys: (1..=18)
            .map(|key| KeyVisual {
                content_mode: None,
                show_text: true,
                text_position: TextPosition::Center,
                key,
                text: if key <= 15 {
                    format!("{key:02}")
                } else {
                    ["上一页", "首页", "下一页"][(key - 16) as usize].into()
                },
                foreground: "#F8FAFC".into(),
                background: "#252936".into(),
                asset_id: None,
                plugin_icon: None,
                default_icon_asset_id: None,
                widget: None,
            })
            .collect(),
    }
}
pub fn capabilities(firmware: &str) -> DeviceCapabilities {
    let mut out = BTreeMap::new();
    for name in [
        "device_control",
        "key_input",
        "key_images",
        "temporary_background",
        "startup_background",
        "screen_brightness",
        "led_brightness",
        "led_color",
        "individual_leds",
        "key_gif",
        "reconnect",
    ] {
        let unavailable = firmware.contains("V2.M18")
            && matches!(name, "led_brightness" | "led_color" | "individual_leds");
        out.insert(
            name.into(),
            Capability {
                available: !unavailable,
                verification: if unavailable {
                    Verification::Unsupported
                } else {
                    Verification::Unverified
                },
                reason: if unavailable {
                    "官方 M18 V2 固件适配未启用 RGB"
                } else if name == "individual_leds" {
                    "官方通用接口存在，M18 逐灯支持需实机验证"
                } else if name == "led_brightness" {
                    "使用 RGB 比例调光；原生 setLEDBrightness 在当前 M18 上被用户报告会变色，已停用；新方式待目视验证"
                } else {
                    "SDK 接口已封装，尚未实机确认效果"
                }
                .into(),
            },
        );
    }
    out
}

pub struct Adapter;
impl DeviceAdapter for Adapter {
    fn id(&self) -> &'static str {
        ID
    }
    fn layout(&self) -> DeviceLayout {
        layout()
    }
    fn definition(&self) -> DeviceDefinition {
        DeviceDefinition {
            inputs: vec![],
            displays: vec![],
            adapter_id: ID.into(),
            name: "M18".into(),
            layout: layout(),
            supports_pages: true,
            input_names: (1..=18)
                .map(|k| {
                    (
                        k,
                        match k {
                            16 => "上一页".into(),
                            17 => "首页".into(),
                            18 => "下一页".into(),
                            _ => format!("显示键 {k}"),
                        },
                    )
                })
                .collect(),
        }
    }
    fn new_page(&self, name: &str) -> Page {
        new_page(name)
    }
    fn default_config(&self) -> AppConfig {
        let page = new_page("主页");
        AppConfig {
            active_page: page.id.clone(),
            pages: vec![page],
            device_settings: DeviceSettings {
                screen_brightness: 70,
                led_brightness: 100,
                led_color: "#5966F3".into(),
                led_mode: LedMode::Solid,
                led_speed: 5,
                led_colors: vec![[89, 102, 243]; 24],
            },
            ..AppConfig::empty()
        }
    }
    fn run(self: std::sync::Arc<Self>, host: AdapterHost) -> AdapterFuture {
        Box::pin(async move {
            let paths = sdk::SdkPaths::resolve(host.resource.as_deref())?;
            sdk::run(host, paths).await
        })
    }
}
