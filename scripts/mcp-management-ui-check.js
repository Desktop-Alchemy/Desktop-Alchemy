// Isolated Vite browser only; never inject mock IPC into a native WebView.
async (page) => {
  if (await page.evaluate(() => "__TAURI_INTERNALS__" in window))
    throw new Error("Use a fresh isolated browser preview");
  await page.reload();
  await page.setViewportSize({ width: 1180, height: 800 });
  await page.evaluate(async () => {
    const { useDesktopAlchemy } = await import("/src/store.ts");
    const { initialConfigFixture, m18Definition } = await import(
      "/scripts/platform-fixtures.ts"
    );
    const copy = (value) => JSON.parse(JSON.stringify(value));
    const w = useDesktopAlchemy();
    await w.setLanguage("zh-CN");
    const state = {
      revisions: { pages: "p1", bindings: "b1", scenes: "s1", settings: "d1" },
      active_page: initialConfigFixture.active_page,
      connection: "disconnected",
      error: null,
      devices: [],
      selected_device: null,
      config: copy(initialConfigFixture),
      input_values: {},
      pressed: [],
      last_key_press: null,
      logs: [],
      receipts: [],
      plugins: [],
      sdk_pid: null,
      gif_status: null,
    };
    const profile = {
      profile_id: "mcp-ui",
      adapter_id: "mirabox.m18",
      name: "MCP fixture",
      device_id: null,
      layout: copy(m18Definition.layout),
      definition: copy(m18Definition),
      module_available: true,
      config_error: null,
      module_state: null,
      state,
      scene_status: {
        states: [],
        winner_id: null,
        retained_id: null,
        error: null,
      },
    };
    const workspace = {
      global: { theme: "light", managed_plugin_ids: [], disabled_plugins: [] },
      devices: [profile],
      discovered: [],
      plugins: [],
      logs: [],
    };
    const f = {
      w,
      state,
      workspace,
      calls: [],
      listeners: {},
      callbacks: {},
      next: 0,
    };
    window.__mcpUi = f;
    f.emit = () =>
      f.listeners["workspace-state"]?.({ payload: copy(workspace) });
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) {
        f.callbacks[++f.next] = callback;
        return f.next;
      },
      async invoke(name, args = {}) {
        f.calls.push({ name, args: copy(args) });
        if (name === "plugin:event|listen") {
          f.listeners[args.event] = f.callbacks[args.handler];
          return args.handler;
        }
        if (name === "snapshot") return copy(workspace);
        if (name === "device_snapshot") return copy(state);
        if (name === "library_snapshot")
          return {
            personal_revision: "l1",
            revision: 0,
            folders: [],
            items: [],
          };
        if (name === "applications_snapshot")
          return { revision: "a1", version: 2, items: [] };
        if (name === "app_settings")
          return {
            language: "zh-CN",
            close_behavior: "tray",
            model_appearances: {},
          };
        if (name === "widget_samples") return [];
        if (name === "save_config" || name === "save_scenes") {
          const resource = name === "save_config" ? "pages" : "scenes";
          if (args.expectedRevision !== state.revisions[resource])
            throw {
              message: { key: "errors.savedResourceChanged", args: {} },
              detail: "stale_operation",
            };
          if (resource === "pages")
            state.config.pages = copy(args.config.pages);
          else state.config.scene_settings = copy(args.settings);
          state.revisions[resource] += "x";
          f.emit();
          return { saved: true, revision: state.revisions[resource] };
        }
        return null;
      },
    };
    w.desktop = true;
    await w.init();
    await w.selectDevice("mcp-ui");
    w.draft.pages[0].name = "Local unsaved draft";
    w.edit();
    state.config.pages[0].name = "AI saved page";
    state.revisions.pages = "p2";
    f.emit();
    if (await w.flushSave())
      throw new Error("Stale page draft overwrote AI state");
    if (w.draft.pages[0].name !== "Local unsaved draft")
      throw new Error("Draft lost after conflict");
    if (state.config.pages[0].name !== "AI saved page")
      throw new Error("Saved AI edit lost");
    if (
      f.calls.filter((call) => call.name === "save_config").at(-1).args
        .expectedRevision !== "p1"
    )
      throw new Error(
        "Background snapshot incorrectly advanced the draft base",
      );
  });
  // Enter the offline saved profile through the actual UI before resolving it.
  await page.locator("summary").filter({ hasText: "已保存配置" }).click();
  await page
    .getByRole("button", { name: "打开配置 MCP fixture", exact: true })
    .click();
  await page
    .getByRole("button", { name: "重新载入已保存配置", exact: true })
    .click();
  let dialog = page.getByRole("dialog");
  await page.screenshot({ path: "output/playwright/mcp-draft-recovery.png" });
  if (
    !(await dialog
      .getByRole("button", { name: "取消", exact: true })
      .evaluate((el) => el === document.activeElement))
  )
    throw new Error("Discard confirmation did not focus Cancel");
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  if (
    await page.evaluate(
      () => window.__mcpUi.w.draft.pages[0].name !== "Local unsaved draft",
    )
  )
    throw new Error("Cancelling reload discarded the draft");
  await page
    .getByRole("button", { name: "重新载入已保存配置", exact: true })
    .click();
  dialog = page.getByRole("dialog");
  await dialog
    .getByRole("button", { name: "重新载入已保存配置", exact: true })
    .click();
  await page.waitForFunction(() => !window.__mcpUi.w.dirty);
  await page.evaluate(async () => {
    const { w, state, emit } = window.__mcpUi;
    if (w.draft.pages[0].name !== "AI saved page")
      throw new Error("Reload did not use saved AI state");
    w.draft.pages[0].name = "Reconciled edit";
    w.edit();
    if (!(await w.flushSave())) throw new Error("Reconciled save failed");
    w.draft.pages[0].name = "Next edit";
    w.edit();
    if (!(await w.flushSave()))
      throw new Error("Own successful save did not advance the draft base");
    w.sceneDraft.enabled = false;
    w.editScenes();
    state.config.scene_settings.enabled = true;
    state.revisions.scenes = "s2";
    emit();
    if (await w.flushScenes())
      throw new Error("Stale scenes overwrote AI state");
    if (w.sceneDraft.enabled !== false || !w.sceneSaveError)
      throw new Error("Scene conflict lost its draft/error");
  });
  console.log(
    JSON.stringify({
      ok: true,
      scenarios: [
        "stale page rejection",
        "draft retention",
        "safe discard confirmation",
        "reconciled save",
        "successive saves",
        "scene conflict",
      ],
    }),
  );
}
