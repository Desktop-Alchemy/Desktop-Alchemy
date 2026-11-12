import { t } from "./i18n";
import type {
  Scene,
  SceneCondition,
  SceneSettings,
  Page,
  DayFilter,
} from "./bindings";

export function newScene(pageId: string): Scene {
  return {
    id: crypto.randomUUID(),
    name: t("ui.newScene"),
    enabled: false,
    page_id: pageId,
    trigger: { type: "continuous" },
    condition: { type: "all", children: [] },
    enter_delay_ms: 300,
    exit_delay_ms: 1000,
  };
}
export function conditionSize(c: SceneCondition): number {
  return (
    1 +
    (c.type === "all" || c.type === "any"
      ? c.children.reduce((n, c) => n + conditionSize(c), 0)
      : c.type === "not"
        ? conditionSize(c.child)
        : 0)
  );
}
export function conditionSummary(c: SceneCondition): string {
  switch (c.type) {
    case "all":
      return c.children.length
        ? t("ui.allMatch2", {
            arg0: c.children.map(conditionSummary).join(t("app.listSeparator")),
          })
        : t("ui.noAdditionalConditions");
    case "any":
      return t("ui.anyMatch2", {
        arg0: c.children.map(conditionSummary).join(t("app.listSeparator")),
      });
    case "not":
      return t("ui.doesNotMatch", { arg0: conditionSummary(c.child) });
    case "process":
      return t("ui.isRunning", {
        arg0: c.matcher.value.split(/[\\/]/).at(-1) || t("ui.program"),
      });
    case "foreground":
      return t("ui.isInTheForeground", {
        arg0: c.matcher.value.split(/[\\/]/).at(-1) || t("ui.program"),
      });
    case "media_playing":
      return t("ui.systemMediaIsPlaying");
    case "time_range":
      return `${c.start}–${c.end}${c.start > c.end ? t("ui.crossesMidnight") : ""}`;
  }
}
export const sceneStatusLabels: Record<string, string> = {
  get active() {
    return t("ui.active");
  },
  get covered() {
    return t("ui.overriddenByHigherPriority");
  },
  get unmatched() {
    return t("ui.notMatched");
  },
  get unavailable() {
    return t("ui.unavailable2");
  },
  get disabled() {
    return t("ui.disabled");
  },
};
const validTime = (v: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(v);
function validDays(d: DayFilter): boolean {
  if (d.type === "every_day") return true;
  if (d.type === "weekdays")
    return (
      d.days.length > 0 &&
      d.days.length <= 7 &&
      new Set(d.days).size === d.days.length &&
      d.days.every((v) => Number.isInteger(v) && v >= 1 && v <= 7)
    );
  return (
    /^\d{4}-\d{2}-\d{2}$/.test(d.date) &&
    !isNaN(Date.parse(d.date)) &&
    new Date(d.date).toISOString().slice(0, 10) === d.date
  );
}
export function validateSceneSettings(s: SceneSettings, pages: Page[]): string {
  const exists = (id: string) => pages.some((p) => p.id === id);
  if (
    !Number.isInteger(s.process_interval_ms) ||
    s.process_interval_ms < 500 ||
    s.process_interval_ms > 10000
  )
    return t("ui.processScanIntervalMustBe500");
  if (s.scenes.length > 200) return t("ui.upTo200ScenesAreSupported");
  if (s.fallback.type === "page" && !exists(s.fallback.page_id))
    return t("ui.chooseADefaultPage");
  const ids = new Set<string>();
  function condition(c: SceneCondition, depth: number): string {
    if (depth > 8) return t("ui.conditionsMayBeNestedUpTo");
    if (c.type === "all" || c.type === "any") {
      if (c.type === "any" && !c.children.length)
        return t("ui.anAnyMatchGroupCannotBe");
      return c.children.map((c) => condition(c, depth + 1)).find(Boolean) || "";
    }
    if (c.type === "not") return condition(c.child, depth + 1);
    if (c.type === "time_range")
      return validDays(c.days) &&
        validTime(c.start) &&
        validTime(c.end) &&
        c.start !== c.end
        ? ""
        : t("ui.invalidDateOrTimeRange");
    if (c.type === "process" || c.type === "foreground") {
      const v = c.matcher.value;
      if (!v || v !== v.trim() || v.length > 32768 || /[\x00-\x1f\x7f]/.test(v))
        return t("ui.enterAProgramMatchValue");
      if (c.matcher.type === "name")
        return /^[^\\/*?]+\.exe$/i.test(v)
          ? ""
          : t("ui.enterTheFullExeProcessName");
      return /^(?:[a-z]:[\\/]|\\\\[^\\]+\\[^\\]+[\\/]).+\.exe$/i.test(v)
        ? ""
        : t("ui.chooseAnAbsolutePathToAn");
    }
    return "";
  }
  for (const scene of s.scenes) {
    if (!scene.id || scene.id.length > 128 || ids.has(scene.id))
      return t("ui.invalidSceneId");
    ids.add(scene.id);
    if (
      !scene.name.trim() ||
      [...scene.name].length > 120 ||
      /[\x00-\x1f\x7f]/.test(scene.name)
    )
      return t("ui.sceneNameMustBe1120");
    if (scene.enabled && !exists(scene.page_id))
      return t("ui.theTargetPageForDoesNot", { arg0: scene.name });
    if (
      [scene.enter_delay_ms, scene.exit_delay_ms].some(
        (v) => !Number.isInteger(v) || v < 0 || v > 60000,
      )
    )
      return t("ui.sceneDelayMustBe060000");
    if (
      scene.trigger.type === "scheduled" &&
      (!validDays(scene.trigger.schedule.days) ||
        !validTime(scene.trigger.schedule.time))
    )
      return t("ui.invalidTriggerDateOrTime");
    if (conditionSize(scene.condition) > 128)
      return t("ui.eachSceneSupportsUpTo128");
    const error = condition(scene.condition, 1);
    if (error) return t("app.sceneValidation", { name: scene.name, error });
  }
  return "";
}
