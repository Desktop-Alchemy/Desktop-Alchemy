import { t } from "./i18n";
import type { PluginManifest } from "./bindings";

// Localize presentation metadata by stable IDs. Never rewrite plugin manifests.
export function pluginName(manifest: PluginManifest): string {
  if (manifest.id === "dev.masterain.performance")
    return t("plugins.performance");
  if (manifest.id === "dev.masterain.counter") return t("plugins.counter");
  return manifest.name;
}
export function widgetName(
  pluginId: string,
  widgetId: string,
  original: string,
): string {
  return pluginId === "dev.masterain.performance" &&
    [
      "cpu",
      "memory",
      "disk",
      "network",
      "battery",
      "temperature",
      "fan",
      "power",
      "voltage",
    ].includes(widgetId)
    ? t(`plugins.widgets.${widgetId}`)
    : original;
}
export function presentationName(
  pluginId: string,
  id: string,
  original: string,
): string {
  return pluginId === "dev.masterain.performance" &&
    [
      "percent",
      "available",
      "used",
      "duplex",
      "download",
      "upload",
      "level",
      "remaining",
    ].includes(id)
    ? t(`plugins.presentations.${id}`)
    : original;
}
