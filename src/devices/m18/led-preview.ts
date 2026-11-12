import type { DeviceSettings } from "../../bindings";

export type LedPreviewSettings = Pick<
  DeviceSettings,
  "led_mode" | "led_color" | "led_brightness" | "led_speed" | "led_colors"
>;
type Rgb = [number, number, number];
const wrap = (value: number) => ((value % 1) + 1) % 1;

export const ledCycleMs = (speed: number) =>
  9000 - (Math.min(10, Math.max(1, speed)) - 1) * 800;

export function animatedLed(settings: LedPreviewSettings) {
  return ["breathing", "color_cycle", "rainbow", "wave", "comet"].includes(
    settings.led_mode,
  );
}

function hueRgb(hue: number): Rgb {
  const h = wrap(hue) * 6;
  const x = 255 * (1 - Math.abs((h % 2) - 1));
  const colors: Rgb[] = [
    [255, x, 0],
    [x, 255, 0],
    [0, 255, x],
    [0, x, 255],
    [x, 0, 255],
    [255, 0, x],
  ];
  return colors[Math.floor(h)].map(Math.round) as Rgb;
}

// Mirrors src-tauri/src/led.rs; the preview has its own clock, not device readback.
export function ledPreviewFrame(
  settings: LedPreviewSettings,
  elapsedMs: number,
  count = 24,
): Rgb[] | null {
  if (settings.led_mode === "device_default") return null;
  const hex = /^#[\da-f]{6}$/i.test(settings.led_color)
    ? settings.led_color.slice(1)
    : "5966f3";
  const primary = [0, 2, 4].map((n) =>
    Number.parseInt(hex.slice(n, n + 2), 16),
  ) as Rgb;
  const phase = wrap(elapsedMs / ledCycleMs(settings.led_speed));
  const brightness = Math.min(255, Math.max(0, settings.led_brightness)) / 255;
  return Array.from({ length: count }, (_, index): Rgb => {
    let color = primary;
    let level = 1;
    const position = index / count;
    switch (settings.led_mode) {
      case "individual":
        color = settings.led_colors[index] ?? [0, 0, 0];
        break;
      case "breathing":
        level = 0.5 - 0.5 * Math.cos(2 * Math.PI * phase);
        break;
      case "color_cycle":
        color = hueRgb(phase);
        break;
      case "rainbow":
        color = hueRgb(position + phase);
        break;
      case "wave":
        level =
          0.08 +
          0.92 * (0.5 + 0.5 * Math.cos(2 * Math.PI * (position - phase)));
        break;
      case "comet": {
        const distance = (((phase * count - index) % count) + count) % count;
        level = distance < 5 ? (1 - distance / 5) ** 2 : 0;
        break;
      }
    }
    return color.map((channel) =>
      Math.round(channel * brightness * level),
    ) as Rgb;
  });
}
