"""Extract the named 64x64 social icons from the bundled Figma page export."""

from __future__ import annotations

from pathlib import Path

from PIL import Image, PngImagePlugin


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src-tauri" / "resources" / "icon-library" / "social-media" / "source-export.png"
OUT = SOURCE.parent
X_CENTERS = [190, 318, 446, 574, 702, 830, 958, 1086, 1214, 1342, 1470, 1598, 1726]
ROWS = [
    (
        "color",
        [
            "facebook",
            "x-twitter",
            "instagram",
            "linkedin",
            "google",
            "youtube",
            "apple",
            "snapchat",
            "pinterest",
            "medium",
            "github",
            "threads",
            "whatsapp",
        ],
        212,
    ),
    (
        "color",
        [
            "figma",
            "dribbble",
            "reddit",
            "discord",
            "tiktok",
            "tumblr",
            "telegram",
            "bluesky",
            "signal",
            "vk",
            "spotify",
            "twitch",
            "messenger",
        ],
        388,
    ),
    (
        "negative",
        [
            "facebook",
            "x-twitter",
            "instagram",
            "linkedin",
            "google",
            "youtube",
            "apple",
            "snapchat",
            "pinterest",
            "medium",
            "github",
            "threads",
            "whatsapp",
        ],
        692,
    ),
    (
        "negative",
        [
            "figma",
            "dribbble",
            "reddit",
            "discord",
            "tiktok",
            "tumblr",
            "telegram",
            "bluesky",
            "signal",
            "vk",
            "spotify",
            "twitch",
            "messenger",
        ],
        868,
    ),
]


def main() -> None:
    source = Image.open(SOURCE).convert("RGBA")
    count = 0
    for variant, names, y_center in ROWS:
        for x_center, name in zip(X_CENTERS, names, strict=True):
            output = OUT / variant / f"{name}.png"
            output.parent.mkdir(parents=True, exist_ok=True)
            crop = source.crop((x_center - 32, y_center - 32, x_center + 32, y_center + 32))
            metadata = PngImagePlugin.PngInfo()
            metadata.add_text("Source", "Figma Community: Social Media Icons & Logos")
            metadata.add_text("Creator", "Mads Egmose")
            metadata.add_text("License", "CC BY 4.0 - https://creativecommons.org/licenses/by/4.0/")
            metadata.add_text("Modification", "64x64 crop from the exported overview page")
            crop.save(output, optimize=True, pnginfo=metadata)
            count += 1
    print(f"Created {count} social icon PNGs under {OUT}")


if __name__ == "__main__":
    main()
