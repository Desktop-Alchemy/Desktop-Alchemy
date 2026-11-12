import { createI18n } from "vue-i18n";
import { reactive, watchEffect } from "vue";
import en from "./locales/en.json";
import zh from "./locales/zh-CN.json";
import type {
  LanguagePreference,
  UiLanguageState,
  UiMessage,
} from "./bindings";

export const i18n = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages: { en, "zh-CN": zh },
  missingWarn: import.meta.env.DEV,
  fallbackWarn: import.meta.env.DEV,
});

/** Also usable in stores and pure presentation helpers; reads locale reactively. */
export function t(key: string, args: Record<string, unknown> = {}): string {
  // Keep catalogs as ordinary nested JSON: translation tools edit each plural
  // branch as a string instead of having to understand Vue's pipe syntax.
  const hasBranch = (category: string) =>
    i18n.global.te(`${key}.${category}`, language.locale);
  if (typeof args.count === "number" && hasBranch("other")) {
    const category =
      args.count === 0 && hasBranch("zero")
        ? "zero"
        : new Intl.PluralRules(language.locale).select(args.count);
    return i18n.global.t(
      `${key}.${hasBranch(category) ? category : "other"}`,
      args,
    );
  }
  return i18n.global.t(key, args);
}
export const language = reactive<UiLanguageState>({
  preference: "system",
  locale: "en",
});
export function localeForTag(tag: string | undefined): "zh-CN" | "en" {
  return tag?.split(/[-_]/)[0]?.toLowerCase() === "zh" ? "zh-CN" : "en";
}
export function applyLanguage(next: UiLanguageState) {
  language.preference = next.preference;
  language.locale = next.locale;
  i18n.global.locale.value = next.locale;
}
export function browserLanguage(
  preference: LanguagePreference = "system",
): UiLanguageState {
  return {
    preference,
    locale:
      preference === "system" ? localeForTag(navigator.language) : preference,
  };
}
export function observeDocumentLanguage(tray = false) {
  return watchEffect(() => {
    document.documentElement.lang = language.locale;
    document.title = tray ? t("app.trayTitle") : "Desktop Alchemy";
  });
}
export function messageText(message: UiMessage): string {
  return i18n.global.te(message.key, language.locale) ||
    i18n.global.te(message.key, "en")
    ? t(message.key, message.args)
    : t("errors.operationFailed");
}
export function errorText(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) {
    const message = error.message;
    if (message && typeof message === "object" && "key" in message)
      return messageText(message as UiMessage);
    if (error instanceof Error) return error.message;
  }
  return t("errors.operationFailed");
}
export function errorDetail(error: unknown): string {
  if (typeof error === "object" && error && "detail" in error)
    return String(error.detail);
  return error instanceof Error
    ? (error.stack ?? error.message)
    : String(error);
}
