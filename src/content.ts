import { t } from "./i18n";
import type {
  PluginInfo,
  WidgetDefinition,
  KeyVisual,
  KeyContentMode,
  ApplicationEntry,
} from "./bindings";
import { pluginName, widgetName, presentationName } from "./plugin-labels";

export type ContentChoice = {
  value: string;
  name: string;
  provider: string;
  pluginId: string | null;
  definition: WidgetDefinition | null;
  builtIn: boolean;
  status: string;
  description: string;
};

export function basicContentMode(visual: KeyVisual): KeyContentMode {
  return (
    visual.content_mode ??
    (visual.asset_id || visual.plugin_icon
      ? "pattern"
      : visual.text
        ? "text"
        : "none")
  );
}

export function contentSelection(visual: KeyVisual): string {
  return visual.widget
    ? `${visual.widget.plugin_id}/${visual.widget.widget_id}`
    : `basic:${basicContentMode(visual)}`;
}

export function isGifPattern(visual: KeyVisual): boolean {
  return (
    basicContentMode(visual) === "pattern" &&
    !!visual.asset_id?.endsWith(".gif")
  );
}

export function showsText(visual: KeyVisual): boolean {
  return (
    basicContentMode(visual) !== "none" &&
    visual.show_text !== false &&
    !isGifPattern(visual)
  );
}

export function applyApplicationVisual(
  visual: KeyVisual,
  application: ApplicationEntry,
) {
  const defaultContent =
    !visual.widget &&
    visual.content_mode == null &&
    !visual.asset_id &&
    !visual.plugin_icon &&
    visual.show_text !== false &&
    (visual.text_position ?? "center") === "center" &&
    (!visual.text || visual.text === String(visual.key).padStart(2, "0"));
  if (
    !visual.plugin_icon &&
    (!visual.asset_id || visual.asset_id === visual.default_icon_asset_id)
  ) {
    visual.asset_id = application.icon_asset_id ?? null;
    visual.default_icon_asset_id = application.icon_asset_id ?? null;
    if (defaultContent && application.icon_asset_id) {
      visual.content_mode = "pattern";
      visual.show_text = false;
    }
  }
  if (!visual.text || visual.text === String(visual.key).padStart(2, "0"))
    visual.text = Array.from(application.name).slice(0, 80).join("");
}

export function contentCatalog(plugins: PluginInfo[]): ContentChoice[] {
  const choices: ContentChoice[] = [
    {
      mode: "none",
      name: t("ui.contentNone"),
      description: t("ui.contentNoneDescription"),
    },
    {
      mode: "text",
      name: t("ui.contentText"),
      description: t("ui.contentTextDescription"),
    },
    {
      mode: "pattern",
      name: t("ui.contentPattern"),
      description: t("ui.contentPatternDescription"),
    },
  ].map(({ mode, name, description }) => ({
    value: `basic:${mode}`,
    name,
    provider: t("ui.basicContent"),
    pluginId: null,
    definition: null,
    builtIn: true,
    status: "",
    description,
  }));
  for (const plugin of plugins) {
    for (const widget of plugin.manifest.widgets ?? []) {
      choices.push({
        value: `${plugin.manifest.id}/${widget.id}`,
        name: widgetName(plugin.manifest.id, widget.id, widget.name),
        provider: pluginName(plugin.manifest),
        pluginId: plugin.manifest.id,
        definition: widget,
        builtIn: plugin.built_in,
        status: plugin.status,
        description: widget.presentations?.length
          ? t("ui.displayFormats", {
              arg0: widget.presentations
                .map((p) => presentationName(plugin.manifest.id, p.id, p.name))
                .join(t("app.listSeparator")),
            })
          : t("ui.showOnTheButton", {
              arg0: widgetName(plugin.manifest.id, widget.id, widget.name),
            }),
      });
    }
  }
  return choices;
}

export function contentVisual(
  visual: KeyVisual,
  choice: ContentChoice,
): KeyVisual {
  if (contentSelection(visual) === choice.value) return { ...visual };
  if (choice.value.startsWith("basic:")) {
    const mode = choice.value.slice(6) as KeyContentMode;
    return {
      ...visual,
      widget: null,
      content_mode: mode,
      show_text: mode === "none" ? visual.show_text : mode === "text",
    };
  }
  return {
    ...visual,
    widget:
      choice.definition && choice.pluginId
        ? {
            plugin_id: choice.pluginId,
            widget_id: choice.definition.id,
            style: { ...choice.definition.default_style },
            presentation_id: null,
            source_id: null,
            source_name: null,
          }
        : null,
  };
}

const RECENT_KEY = "desktop-alchemy.recent-content.v1";
export function readRecentContents(): string[] {
  try {
    const values: unknown = JSON.parse(
      localStorage.getItem(RECENT_KEY) ?? "[]",
    );
    return Array.isArray(values)
      ? [
          ...new Set(
            values.filter(
              (v): v is string =>
                typeof v === "string" && v.length > 0 && v.length <= 201,
            ),
          ),
        ].slice(0, 12)
      : [];
  } catch {
    return [];
  }
}
export function writeRecentContents(values: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(values.slice(0, 12)));
  } catch {
    /* Recent choices are optional UI preferences, never page-save state. */
  }
}
