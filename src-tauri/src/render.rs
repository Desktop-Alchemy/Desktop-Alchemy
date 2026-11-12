use crate::{
    model::{KeyVisual, TextPosition},
    storage,
};
use anyhow::{Context, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn key(root: &Path, visual: &KeyVisual) -> Result<PathBuf> {
    let layout = crate::devices::default_layout();
    key_sized(root, visual, layout.key_width, layout.key_height)
}
pub fn key_sized(root: &Path, visual: &KeyVisual, width: u16, height: u16) -> Result<PathBuf> {
    if visual.plugin_icon.is_some() {
        anyhow::bail!(alchemy_device_api::UiFailure::new(
            "backend.b204",
            "插件图标须由核心解析并使用内存合成"
        ));
    }
    storage::color(&visual.background)?;
    storage::color(&visual.foreground)?;
    let id = format!(
        "render-content-v4-{width}x{height}-{:x}.png",
        Sha256::digest(serde_json::to_vec(visual)?)
    );
    let dir = root.join("cache");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(id);
    if file.exists() {
        return Ok(file);
    }
    storage::publish_image(
        root,
        &file,
        &key_png_sized(root, visual, None, width, height)?,
    )?;
    Ok(file)
}
/// Missing installation-owned images deliberately compose only background and text.
pub fn key_png(root: &Path, visual: &KeyVisual, plugin_image: Option<&[u8]>) -> Result<Vec<u8>> {
    let layout = crate::devices::default_layout();
    key_png_sized(
        root,
        visual,
        plugin_image,
        layout.key_width,
        layout.key_height,
    )
}
pub fn key_png_sized(
    root: &Path,
    visual: &KeyVisual,
    plugin_image: Option<&[u8]>,
    width: u16,
    height: u16,
) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        anyhow::bail!(alchemy_device_api::UiFailure::new(
            "backend.b205",
            "设备没有按键图像能力"
        ))
    }
    storage::color(&visual.background)?;
    storage::color(&visual.foreground)?;
    if visual.text.chars().count() > 80
        || (visual.plugin_icon.is_some() && visual.asset_id.is_some())
    {
        anyhow::bail!(alchemy_device_api::UiFailure::new(
            "backend.b206",
            "按键图像或文字无效"
        ));
    }
    let bytes = if !visual.renders_image() {
        None
    } else if visual.plugin_icon.is_some() {
        plugin_image.map(Vec::from)
    } else if let Some(id) = &visual.asset_id {
        Some(std::fs::read(storage::asset(root, id)?)?)
    } else {
        None
    };
    let picture = if let Some(bytes) = bytes {
        let img = resize_image(image::load_from_memory(&bytes)?, 64, 64, true);
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, image::ImageFormat::Png)?;
        format!(
            "<image width='64' height='64' href='data:image/png;base64,{}'/>",
            base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
        )
    } else {
        String::new()
    };
    let chars: Vec<char> = if visual.renders_text() {
        visual.text.chars().collect()
    } else {
        Vec::new()
    };
    let lines: Vec<String> = chars
        .chunks(7)
        .take(4)
        .map(|c| c.iter().collect())
        .collect();
    let first_baseline = match visual.text_position {
        TextPosition::Top => 13,
        TextPosition::Center => 36 - ((lines.len() as i32 - 1) * 13) / 2,
        TextPosition::Bottom => 59 - (lines.len().saturating_sub(1) as i32 * 13),
    };
    let text=lines.iter().enumerate().map(|(i,l)|format!("<text x='32' y='{}' text-anchor='middle' font-family='Microsoft YaHei UI,Segoe UI, sans-serif' font-size='11' font-weight='600' fill='{}' stroke='#000000' stroke-width='0.25' paint-order='stroke'>{}</text>",first_baseline + i as i32 * 13,visual.foreground,escape(l))).collect::<String>();
    let svg=format!("<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{width}' height='{height}' viewBox='0 0 64 64'><rect width='64' height='64' fill='{}'/>{picture}{text}</svg>",visual.background);
    Ok(rasterize(&svg)?.encode_png()?)
}

/// Filter premultiplied RGBA to avoid dark borders from transparent padding.
pub fn resize_image(
    image: image::DynamicImage,
    width: u32,
    height: u32,
    fill: bool,
) -> image::DynamicImage {
    let mut rgba = image.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let image = image::DynamicImage::ImageRgba8(rgba);
    let mut scaled = if fill {
        image.resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
    } else {
        image.resize(width, height, image::imageops::FilterType::Lanczos3)
    }
    .to_rgba8();
    for pixel in scaled.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        if alpha > 0 {
            for channel in &mut pixel.0[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        } else {
            pixel.0[..3].fill(0);
        }
    }
    image::DynamicImage::ImageRgba8(scaled)
}

fn rasterize(svg: &str) -> Result<resvg::tiny_skia::Pixmap> {
    static OPTIONS: OnceLock<resvg::usvg::Options> = OnceLock::new();
    let opts = OPTIONS.get_or_init(|| {
        let mut o = resvg::usvg::Options::default();
        o.fontdb_mut().load_system_fonts();
        o
    });
    let tree = resvg::usvg::Tree::from_str(svg, opts)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(
        tree.size().to_int_size().width(),
        tree.size().to_int_size().height(),
    )
    .context(alchemy_device_api::UiFailure::new(
        "backend.b207",
        "画布初始化失败",
    ))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
pub fn blank_background(root: &Path) -> Result<PathBuf> {
    let layout = crate::devices::default_layout();
    blank_background_sized(root, layout.background_width, layout.background_height)
}
pub fn blank_background_sized(root: &Path, width: u16, height: u16) -> Result<PathBuf> {
    if width == 0 || height == 0 {
        anyhow::bail!(alchemy_device_api::UiFailure::new(
            "backend.b208",
            "设备没有背景能力"
        ))
    }
    let dir = root.join("cache");
    std::fs::create_dir_all(&dir)?;
    let p = dir.join(format!("background-{width}x{height}.png"));
    if !p.exists() {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(
            u32::from(width),
            u32::from(height),
            image::Rgb([17, 20, 29]),
        )
        .write_to(&mut encoded, image::ImageFormat::Png)?;
        storage::publish_image(root, &p, encoded.get_ref())?;
    }
    Ok(p)
}

/// Bright and dark patches use the same imported assets in preview and on device.
pub fn brightness_page(root: &Path) -> Result<crate::model::Page> {
    #[cfg(feature = "device-m18")]
    {
        crate::devices::diagnostic::brightness_page(root)
    }
    #[cfg(not(feature = "device-m18"))]
    {
        let _ = root;
        anyhow::bail!(alchemy_device_api::UiFailure::new(
            "backend.b209",
            "未启用设备硬件测试模块"
        ))
    }
}

/// Runtime images stay in memory; they never enter the persistent asset cache.
pub fn widget(
    binding: &crate::model::WidgetBinding,
    data: &crate::model::WidgetData,
) -> Result<Vec<u8>> {
    crate::widgets::validate_binding(binding)?;
    let style = &binding.style;
    fn line(text: &str, y: u8, size: f64, color: &str) -> String {
        fitted_line(text, y, size, color, 32, 56.0)
    }
    fn fitted_line(text: &str, y: u8, size: f64, color: &str, x: u8, max_width: f64) -> String {
        // Conservative widths for Latin digits and full-width CJK, fitted before rasterization.
        let width: f64 = text
            .chars()
            .map(|c| if c.is_ascii() { 0.62 } else { 1.0 })
            .sum();
        let size = size.min(max_width / width.max(1.0));
        format!("<text x='{x}' y='{y}' text-anchor='middle' font-family='Microsoft YaHei UI,Segoe UI,sans-serif' font-size='{size}' font-weight='600' fill='{color}'>{}</text>", escape(text))
    }
    let title = line(&data.title, 13, 10.0, &style.foreground);
    let value = if data.valid && data.secondary_value.is_some() {
        let arrows = format!("<path d='M7 20v10m-3-4 3 4 3-4M7 47V37m-3 4 3-4 3 4' fill='none' stroke='{}' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/>", style.accent);
        format!(
            "{arrows}{}{}",
            fitted_line(&data.value, 29, 15.0, &style.foreground, 36, 46.0),
            fitted_line(
                data.secondary_value.as_deref().unwrap_or(""),
                46,
                15.0,
                &style.foreground,
                36,
                46.0
            )
        )
    } else {
        line(
            if data.valid { &data.value } else { "—" },
            36,
            23.0,
            &style.foreground,
        )
    };
    let detail = if data.valid && data.secondary_value.is_none() {
        line(
            data.detail.as_deref().unwrap_or(""),
            49,
            8.0,
            &style.foreground,
        )
    } else {
        String::new()
    };
    let progress = if style.show_progress && data.valid {
        data.progress.map(|v| format!("<rect x='5' y='55' width='54' height='4' rx='2' fill='{}' opacity='.18'/><rect x='5' y='55' width='{}' height='4' rx='2' fill='{}'/>", style.foreground, v.clamp(0.0, 1.0) * 54.0, style.accent)).unwrap_or_default()
    } else {
        String::new()
    };
    let svg = format!("<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='{}'/>{title}{value}{detail}{progress}</svg>", style.background);
    Ok(rasterize(&svg)?.encode_png()?)
}

pub fn widget_sized(
    binding: &crate::model::WidgetBinding,
    data: &crate::model::WidgetData,
    width: u16,
    height: u16,
) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        anyhow::bail!(alchemy_device_api::UiFailure::new(
            "backend.b205",
            "设备没有按键图像能力"
        ))
    }
    let png = widget(binding, data)?;
    let image = resize_image(
        image::load_from_memory(&png)?,
        width.into(),
        height.into(),
        true,
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(bytes.into_inner())
}

#[cfg(test)]
mod transparency_tests {
    use super::*;
    use crate::model::{KeyContentMode, PluginIconRef};

    #[test]
    fn modes_and_visibility_compose_only_requested_layers_and_retain_content() {
        let mut visual = crate::devices::default_page("layers").keys.remove(0);
        visual.background = "#0000FF".into();
        visual.text = "ABC".into();
        visual.plugin_icon = Some(PluginIconRef {
            plugin_id: "example.layers".into(),
            icon_id: "red".into(),
        });
        let mut source = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(64, 64, image::Rgb([255, 0, 0]))
            .write_to(&mut source, image::ImageFormat::Png)
            .unwrap();
        let render = |v: &KeyVisual| {
            image::load_from_memory(&key_png(Path::new("."), v, Some(source.get_ref())).unwrap())
                .unwrap()
                .to_rgb8()
        };
        visual.content_mode = Some(KeyContentMode::None);
        assert!(render(&visual).pixels().all(|p| p.0 == [0, 0, 255]));
        visual.content_mode = Some(KeyContentMode::Text);
        let text = render(&visual);
        assert_eq!(text.get_pixel(0, 0).0, [0, 0, 255]);
        assert!(text.pixels().any(|p| p.0 != [0, 0, 255]));
        visual.content_mode = Some(KeyContentMode::Pattern);
        visual.show_text = false;
        let picture = render(&visual);
        assert!(picture.pixels().all(|p| p.0 == [255, 0, 0]));
        visual.show_text = true;
        assert_ne!(render(&visual), picture);
        visual.show_text = false;
        assert_eq!(render(&visual), picture);
        assert_eq!(visual.text, "ABC");
        assert!(visual.plugin_icon.is_some());
    }

    #[test]
    fn text_positions_align_the_whole_block_inside_the_key() {
        let mut visual = crate::devices::default_page("position").keys.remove(0);
        visual.content_mode = Some(KeyContentMode::Text);
        visual.background = "#000000".into();
        visual.text = "AB".into();
        let mut bounds = Vec::new();
        for position in [
            TextPosition::Top,
            TextPosition::Center,
            TextPosition::Bottom,
        ] {
            visual.text_position = position;
            let png = key_png(Path::new("."), &visual, None).unwrap();
            let image = image::load_from_memory(&png).unwrap().to_rgb8();
            let rows: Vec<_> = image
                .enumerate_pixels()
                .filter(|(_, _, p)| p.0 != [0, 0, 0])
                .map(|(_, y, _)| y)
                .collect();
            bounds.push((*rows.iter().min().unwrap(), *rows.iter().max().unwrap()));
        }
        assert!(bounds[0].1 < bounds[1].0 && bounds[1].1 < bounds[2].0);
        visual.text = "ABCDEFGHIJKLMNOPQRSTUVWXYZAB".into();
        for position in [
            TextPosition::Top,
            TextPosition::Center,
            TextPosition::Bottom,
        ] {
            visual.text_position = position;
            let image = image::load_from_memory(&key_png(Path::new("."), &visual, None).unwrap())
                .unwrap()
                .to_rgb8();
            assert!((0..64)
                .all(|x| image.get_pixel(x, 0).0 == [0, 0, 0]
                    && image.get_pixel(x, 63).0 == [0, 0, 0]));
        }
    }

    #[test]
    fn transparent_padding_does_not_darkly_outline_white_glyphs() {
        let mut source = image::RgbaImage::new(129, 129);
        for (x, _, pixel) in source.enumerate_pixels_mut() {
            *pixel = if x < 64 {
                image::Rgba([255, 255, 255, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            };
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        source
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let mut visual = crate::devices::default_page("alpha").keys.remove(0);
        visual.text.clear();
        visual.background = "#FFFFFF".into();
        visual.plugin_icon = Some(crate::model::PluginIconRef {
            plugin_id: "example.alpha".into(),
            icon_id: "white".into(),
        });
        let png = key_png(Path::new("."), &visual, Some(bytes.get_ref())).unwrap();
        assert!(image::load_from_memory(&png)
            .unwrap()
            .to_rgb8()
            .pixels()
            .all(|p| p.0.iter().all(|v| *v >= 253)));
    }
}
