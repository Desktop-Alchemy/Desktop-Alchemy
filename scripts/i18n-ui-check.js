// Isolated Vite preview only. Native acceptance is a separate, hardware-free run.
async function i18nUiCheck(page) {
  if (await page.evaluate(() => !!window.__TAURI_INTERNALS__?.metadata))
    throw new Error("Do not run browser fixtures in a native WebView");
  await page.reload();
  await page.setViewportSize({ width: 1180, height: 800 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  const messages = await page.evaluate(async () => {
    const { i18n, applyLanguage, localeForTag, t } = await import(
      "/src/i18n.ts"
    );
    const warnings = [];
    const warn = console.warn;
    console.warn = (...args) => {
      warnings.push(args.join(" "));
    };
    function walk(value, prefix = "") {
      for (const [key, message] of Object.entries(value)) {
        const name = prefix ? `${prefix}.${key}` : key;
        if (typeof message !== "string") {
          walk(message, name);
          continue;
        }
        const args = Object.fromEntries(
          [...message.matchAll(/\{(\w+)\}/g)].map((m) => [
            m[1],
            m[1] === "count" ? 2 : "test",
          ]),
        );
        if (t(name, args) === name)
          throw new Error(`Untranslated key: ${name}`);
      }
    }
    try {
      for (const locale of ["en", "zh-CN"]) {
        applyLanguage({ preference: locale, locale });
        walk(i18n.global.messages.value[locale]);
        const counts = [0, 1, 2].map((count) =>
          t("phrases.appCount", { count }),
        );
        const expected =
          locale === "en"
            ? ["No apps", "1 app", "2 apps"]
            : ["暂无应用", "1 个应用", "2 个应用"];
        if (JSON.stringify(counts) !== JSON.stringify(expected))
          throw new Error(`Plural branches failed for ${locale}`);
      }
    } finally {
      console.warn = warn;
    }
    if (warnings.length) throw new Error(warnings.join("\n"));
    for (const tag of ["zh-CN", "zh-TW", "zh-Hans", "zh-Hant", "ZH_hk"])
      if (localeForTag(tag) !== "zh-CN")
        throw new Error(`Chinese locale: ${tag}`);
    for (const tag of ["en-US", "ja-JP", "", undefined])
      if (localeForTag(tag) !== "en")
        throw new Error(`English fallback: ${tag}`);
    applyLanguage({ preference: "en", locale: "en" });
    const { useDesktopAlchemy } = await import("/src/store.ts");
    const { m18Config, m18Definition } = await import(
      "/scripts/platform-fixtures.ts"
    );
    const w = useDesktopAlchemy();
    await w.setLanguage("en");
    const config = m18Config();
    config.pages[0].name = "用户页面 — Keep this name";
    const state = {
      active_page: config.active_page,
      connection: "disconnected",
      error: null,
      devices: [],
      selected_device: null,
      config,
      input_values: {},
      pressed: [],
      logs: [],
      receipts: [],
      plugins: [],
      sdk_pid: null,
      gif_status: null,
    };
    const profile = {
      profile_id: "i18n",
      adapter_id: "mirabox.m18",
      name: "M18 fixture",
      device_id: null,
      layout: m18Definition.layout,
      definition: m18Definition,
      module_available: true,
      config_error: null,
      module_state: null,
      scene_status: {
        states: [],
        winner_id: null,
        retained_id: null,
        error: null,
      },
      state,
    };
    w.workspace = {
      global: { theme: "dark", managed_plugin_ids: [], disabled_plugins: [] },
      devices: [profile],
      discovered: [],
      plugins: [],
      logs: [],
    };
    await w.selectDevice("i18n");
    w.page.name = "未保存草稿 — Keep this draft";
    w.edit();
    w.sceneDraft.scenes.push({
      id: "scene",
      name: "用户场景",
      page_id: config.active_page,
      enabled: false,
      trigger: { type: "continuous" },
      condition: { type: "all", children: [] },
      enter_delay_ms: 300,
      exit_delay_ms: 1000,
    });
    w.editScenes();
    window.__i18nFixture = w;
    return { locales: 2, warnings: warnings.length };
  });
  await page.getByRole("button", { name: "App settings", exact: true }).click();
  await page.getByRole("combobox", { name: "Language", exact: true }).click();
  await page.getByRole("option", { name: "中文", exact: true }).click();
  await page.getByRole("dialog", { name: "应用设置", exact: true }).waitFor();
  if ((await page.locator("html").getAttribute("lang")) !== "zh-CN")
    throw new Error("Chinese html.lang");
  await page.getByRole("combobox", { name: "语言", exact: true }).click();
  await page.getByRole("option", { name: "English", exact: true }).click();
  await page
    .getByRole("dialog", { name: "App settings", exact: true })
    .waitFor();
  const state = await page.evaluate(async () => {
    const w = window.__i18nFixture;
    if (
      w.page.name !== "未保存草稿 — Keep this draft" ||
      !w.dirty ||
      !w.sceneDirty ||
      w.sceneDraft.scenes[0].name !== "用户场景"
    )
      throw new Error("Language switch changed drafts");
    // Exercise the failed-save UI without a native transport or user data.
    w.desktop = true;
    window.__TAURI_INTERNALS__ = {
      invoke: async () => {
        throw {
          message: { key: "errors.operationFailed", args: {} },
          detail: "fixture: disk write denied",
        };
      },
    };
    await w.setLanguage("zh-CN");
    const unchanged =
      w.language.locale === "en" && w.language.preference === "en";
    w.desktop = false;
    delete window.__TAURI_INTERNALS__;
    if (!unchanged) throw new Error("Failed save changed the language");
    return { draftRetained: true, failedSaveRetainedLanguage: unchanged };
  });
  await page
    .getByText(
      "Could not save the language preference. The previous language is still active.",
      { exact: true },
    )
    .waitFor();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.getByRole("button", { name: "Extensions", exact: true }).click();
  await page
    .getByRole("heading", { name: "Local extensions", exact: true })
    .waitFor();
  await page.screenshot({
    path: "output/playwright/i18n-english.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 900, height: 650 });
  await page.getByRole("button", { name: "App settings", exact: true }).click();
  const overflow = await page
    .locator("dialog[open]")
    .evaluate((el) => el.scrollWidth > el.clientWidth + 1);
  if (overflow) throw new Error("English settings overflow at 900×650");
  await page.screenshot({
    path: "output/playwright/i18n-settings-900.png",
    fullPage: true,
  });
  if (errors.length) throw new Error(errors.join("\n"));
  return { ...messages, ...state, englishLayout: true, native: false };
}
