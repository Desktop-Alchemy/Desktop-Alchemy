// Real diagnostics instance only. No fixture IPC and no physical device ownership.
async function i18nNativeUiCheck(page) {
  const before = await page.evaluate(async () => {
    const { invoke } = await import(
      "/node_modules/.vite/deps/@tauri-apps_api_core.js"
    );
    const snapshot = await invoke("snapshot");
    if (
      window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label !== "main" ||
      !snapshot.logs.some(
        (l) => l.message === "原生 UI 隔离测试：设备连接已禁用",
      ) ||
      snapshot.devices.some(
        (d) => d.state.sdk_pid !== null || d.state.devices.length,
      )
    )
      throw new Error(
        "Requires an isolated diagnostics client with hardware disabled",
      );
    return invoke("set_ui_language", { preference: "system" });
  });
  const tray = page
    .context()
    .pages()
    .find((p) => p.url().endsWith("/tray.html"));
  if (!tray) throw new Error("Native tray WebView missing");
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  tray.on("pageerror", (e) => errors.push(String(e)));
  await page.getByRole("button", { name: /^(App settings|应用设置)$/ }).click();
  await page.getByRole("combobox", { name: /^(Language|语言)$/ }).click();
  await page.getByRole("option", { name: "中文", exact: true }).click();
  await page.getByRole("dialog", { name: "应用设置", exact: true }).waitFor();
  await tray.waitForFunction(() => document.documentElement.lang === "zh-CN");
  await tray
    .getByRole("button", { name: "显示窗口", exact: true })
    .waitFor({ state: "attached" });
  async function fallbackLabels() {
    return page.evaluate(async () => {
      const { listen, emit } = await import(
        "/node_modules/.vite/deps/@tauri-apps_api_event.js"
      );
      return new Promise(async (resolve, reject) => {
        const timer = setTimeout(() => {
          stop?.();
          reject(Error("Native fallback menu timed out"));
        }, 5000);
        let stop;
        stop = await listen("diagnostics-tray-menu", (e) => {
          clearTimeout(timer);
          stop();
          resolve(e.payload);
        });
        await emit("diagnostics-tray-fallback");
      });
    });
  }
  if (
    JSON.stringify(await fallbackLabels()) !==
    JSON.stringify(["显示窗口", "退出"])
  )
    throw new Error("Chinese native fallback menu");
  const chinese = await page.evaluate(async () => {
    const { invoke } = await import(
      "/node_modules/.vite/deps/@tauri-apps_api_core.js"
    );
    return (await invoke("app_settings")).language;
  });
  if (chinese !== "zh-CN")
    throw new Error("Chinese preference was not persisted");
  await page.getByRole("combobox", { name: "语言", exact: true }).click();
  await page.getByRole("option", { name: "English", exact: true }).click();
  await tray.waitForFunction(() => document.documentElement.lang === "en");
  await tray
    .getByRole("button", { name: "Show window", exact: true })
    .waitFor({ state: "attached" });
  if (
    JSON.stringify(await fallbackLabels()) !==
    JSON.stringify(["Show window", "Quit"])
  )
    throw new Error("English native fallback menu");
  await tray.evaluate(async () => {
    const { invoke } = await import(
      "/node_modules/.vite/deps/@tauri-apps_api_core.js"
    );
    await invoke("tray_panel_ready");
  });
  const backend = await page.evaluate(async () => {
    const { invoke } = await import(
      "/node_modules/.vite/deps/@tauri-apps_api_core.js"
    );
    const { messageText } = await import("/src/i18n.ts");
    let message;
    try {
      await invoke("set_theme", { theme: "invalid-fixture" });
    } catch (e) {
      message = e;
    }
    if (
      !message?.message?.key ||
      !message.detail ||
      /[\u3400-\u9fff]/.test(messageText(message.message))
    )
      throw new Error("Backend rejection is not localized");
    const original = await invoke("app_settings");
    await invoke("set_model_appearance", {
      profileId: "missing-fixture",
      appearance: { type: "default" },
    }).catch(() => {});
    await invoke("set_app_settings", {
      settings: { ...original, close_behavior: "exit", language: "zh-CN" },
    });
    if ((await invoke("app_settings")).language !== "en")
      throw new Error("Another settings field overwrote language");
    await invoke("set_app_settings", { settings: original });
    return { structuredBackendError: true, fieldIsolation: true };
  });
  const permission = await tray.evaluate(async () => {
    const { invoke } = await import(
      "/node_modules/.vite/deps/@tauri-apps_api_core.js"
    );
    const state = await invoke("ui_language");
    let denied = 0;
    for (const [command, args] of [
      ["app_settings", {}],
      ["set_ui_language", { preference: "zh-CN" }],
      ["mcp_token", {}],
    ]) {
      try {
        await invoke(command, args);
      } catch {
        denied++;
      }
    }
    return { denied, state };
  });
  if (permission.denied !== 3 || permission.state.locale !== "en")
    throw new Error("Tray permission boundary changed");
  await page.getByRole("combobox", { name: "Language", exact: true }).click();
  await page
    .getByRole("option", { name: "Follow system", exact: true })
    .click();
  const system = await page.evaluate(async () => {
    const { invoke } = await import(
      "/node_modules/.vite/deps/@tauri-apps_api_core.js"
    );
    return invoke("ui_language");
  });
  if (system.preference !== "system" || system.locale !== before.locale)
    throw new Error("System locale changed unexpectedly");
  await page.getByRole("combobox", { name: /^(Language|语言)$/ }).click();
  await page.getByRole("option", { name: "English", exact: true }).click();
  await page.screenshot({
    path: "output/playwright/i18n-native-settings.png",
    fullPage: true,
  });
  if (errors.length) throw new Error(errors.join("\n"));
  return {
    native: true,
    hardware: false,
    systemLocale: system.locale,
    persisted: true,
    traySynced: true,
    nativeFallbackMenus: true,
    trayRestricted: true,
    ...backend,
  };
}
