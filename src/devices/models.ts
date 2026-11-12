import { t } from "../i18n";
import type { AppConfig, Page, DeviceDefinition } from "../bindings";
export function newPage(name: string, definition?: DeviceDefinition): Page {
  return {
    id: crypto.randomUUID(),
    name,
    background_asset: null,
    bindings: (definition?.inputs ?? [])
      .filter((i) => i.kind.type === "button")
      .map((i) => ({
        control_id: i.id,
        enabled: true,
        trigger: { type: "press" },
        action: { type: "none" },
      })),
    keys: [
      ...new Set([
        ...(definition?.layout.input_keys ?? []),
        ...(definition?.layout.display_keys ?? []),
      ]),
    ].map((key) => ({
      key,
      text: definition?.input_names[key] ?? String(key),
      content_mode: null,
      show_text: true,
      text_position: "center",
      foreground: "#F8FAFC",
      background: "#252936",
      asset_id: null,
      plugin_icon: null,
      default_icon_asset_id: null,
      widget: null,
    })),
  };
}
export const initial = newPage(t("ui.home"));
export const initialConfig: AppConfig = {
  version: 1,
  disabled_capabilities: [],
  bindings: [],
  pages: [],
  active_page: "",
  theme: "system",
  device_settings: {
    screen_brightness: 0,
    led_brightness: 0,
    led_color: "#000000",
    led_mode: "solid",
    led_speed: 5,
    led_colors: [],
  },
  verification: {},
  managed_plugin_ids: [],
  disabled_plugins: [],
  scene_settings: {
    enabled: false,
    paused: false,
    process_interval_ms: 1000,
    fallback: { type: "return" },
    scenes: [],
  },
};

export const emptyVisual = {
  key: 0,
  text: "",
  content_mode: null,
  show_text: true,
  text_position: "center" as const,
  foreground: "#FFFFFF",
  background: "#000000",
  asset_id: null,
  plugin_icon: null,
  default_icon_asset_id: null,
  widget: null,
};
