"""Build the bundled M18 key icons from the Material Design icon set."""

from __future__ import annotations

from pathlib import Path

from PIL import Image, PngImagePlugin


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "src-tauri" / "resources" / "key-icons"
SOURCE = ROOT / "src-tauri" / "resources" / "icon-library" / "material-design-icons" / "png"
SIZE = 256
ICON_SIZE = 160
BACKGROUND = "#F7F3EC"
FOREGROUND = "#463D35"
ACCENT = "#B75B4B"


def metadata(source_name: str, modified: str) -> PngImagePlugin.PngInfo:
    info = PngImagePlugin.PngInfo()
    info.add_text("Source", source_name)
    info.add_text("License", "Apache-2.0")
    info.add_text("Modification", modified)
    return info


ICONS = {
    "previous.png": ("previous.png", FOREGROUND),
    "next.png": ("next.png", FOREGROUND),
    "back.png": ("arrow-back-figma.png", None),
    "home.png": ("home.png", FOREGROUND),
    "close.png": ("close.png", FOREGROUND),
    "play.png": ("play.png", FOREGROUND),
    "pause.png": ("pause.png", FOREGROUND),
    "stop.png": ("stop.png", FOREGROUND),
    "volume-up.png": ("volume-up.png", FOREGROUND),
    "volume-down.png": ("volume-down.png", FOREGROUND),
    "mute.png": ("mute.png", FOREGROUND),
    "microphone.png": ("microphone.png", FOREGROUND),
    "screenshot.png": ("screenshot.png", FOREGROUND),
    "record.png": ("record.png", ACCENT),
    "light.png": ("light.png", FOREGROUND),
    "timer.png": ("timer.png", FOREGROUND),
    "lock.png": ("lock.png", FOREGROUND),
    "settings.png": ("settings.png", FOREGROUND),
    "refresh.png": ("refresh.png", FOREGROUND),
    "switch-apps.png": ("switch-apps.png", FOREGROUND),
    "brightness-up.png": ("brightness-up.png", FOREGROUND),
    "brightness-down.png": ("brightness-down.png", FOREGROUND),
    "music.png": ("music.png", FOREGROUND),
}


def render(source: Path, output: Path, color: str) -> None:
    icon = Image.open(source).convert("RGBA")
    alpha = icon.getchannel("A")
    bounds = alpha.getbbox()
    if bounds is None:
        raise ValueError(f"Icon has no visible pixels: {source}")
    alpha = alpha.crop(bounds)
    width, height = alpha.size
    scale = min(ICON_SIZE / width, ICON_SIZE / height)
    size = (max(1, round(width * scale)), max(1, round(height * scale)))
    alpha = alpha.resize(size, Image.Resampling.LANCZOS)

    glyph = Image.new("RGBA", size, color)
    glyph.putalpha(alpha)
    canvas = Image.new("RGBA", (SIZE, SIZE), BACKGROUND)
    position = ((SIZE - size[0]) // 2, (SIZE - size[1]) // 2)
    canvas.alpha_composite(glyph, position)
    canvas.save(
        output,
        optimize=True,
        pnginfo=metadata(
            "Google Material Design Icons - https://github.com/google/material-design-icons",
            "Recolored, resized, and composited on a flat warm ivory tile for Desktop Alchemy.",
        ),
    )


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, (source_name, color) in ICONS.items():
        source = SOURCE / source_name
        output = OUT / name
        if color is None:
            icon = Image.open(source).convert("RGBA")
            icon.save(
                output,
                optimize=True,
                pnginfo=metadata(
                    "Google Fonts Team, Material Design Icons - Figma Community",
                    "Rasterized, recolored, and composited on a flat warm ivory tile for Desktop Alchemy.",
                ),
            )
        else:
            render(source, output, color)


LABELS = ["上一页", "下一页", "返回", "主页", "关闭", "播放", "暂停", "停止", "音量+", "音量−", "静音", "麦克风", "截图", "录制", "灯光", "计时器", "锁定", "设置", "刷新", "应用切换", "亮度+", "亮度−", "音乐"]


def transparent_icons() -> None:
    entries = []
    for variant, color in [("light", "#F8FAFC"), ("dark", FOREGROUND)]:
        folder = OUT / "transparent" / variant
        folder.mkdir(parents=True, exist_ok=True)
        for (name, (source_name, _)), label in zip(ICONS.items(), LABELS, strict=True):
            source = SOURCE / ("back-alpha.png" if name == "back.png" else source_name)
            image = Image.open(source).convert("RGBA")
            alpha = image.getchannel("A")
            bounds = alpha.getbbox()
            if bounds is None:
                raise ValueError(f"Empty icon: {source}")
            alpha = alpha.crop(bounds)
            width, height = alpha.size
            scale = min(ICON_SIZE / width, ICON_SIZE / height)
            size = (max(1, round(width * scale)), max(1, round(height * scale)))
            glyph = Image.new("RGBA", size, color)
            glyph.putalpha(alpha.resize(size, Image.Resampling.LANCZOS))
            canvas = Image.new("RGBA", (SIZE, SIZE))
            canvas.alpha_composite(glyph, ((SIZE-size[0])//2, (SIZE-size[1])//2))
            canvas.save(folder / name, optimize=True, pnginfo=metadata("Google Material Design Icons", "Recolored and resized as a transparent PNG for Desktop Alchemy."))
            entries.append(f'    ("{variant}", "{label}", include_bytes!("../resources/key-icons/transparent/{variant}/{name}")),')
    module = ROOT / "src-tauri" / "src" / "builtin_transparent.rs"
    module.write_text('// Generated by scripts/generate-key-icons.py.\n' + 'pub const ICONS: &[(&str, &str, &[u8])] = &[\n' + '\n'.join(entries) + '\n];\n', encoding="utf-8")


if __name__ == "__main__":
    main()
    transparent_icons()
