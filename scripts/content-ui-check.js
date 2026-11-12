// Isolated Vite browser only. Mock IPC never opens the SDK or user data.
async (page) => {
  if (await page.evaluate(() => "__TAURI_INTERNALS__" in window))
    throw new Error("Use a fresh isolated browser preview");
  await page.reload();
  await page.setViewportSize({ width: 1440, height: 1000 });
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  const check = (condition, message) => {
    if (!condition) throw Error(message);
  };
  await page.evaluate(async () => {
    const { useDesktopAlchemy } = await import("/src/store.ts");
    const { m18Config, m18Definition } = await import(
      "/scripts/platform-fixtures.ts"
    );
    const copy = (value) => JSON.parse(JSON.stringify(value));
    const w = useDesktopAlchemy();
    await w.setLanguage("zh-CN");
    localStorage.setItem(
      "desktop-alchemy.recent-content.v1",
      '["","example.widget/value"]',
    );
    const { readRecentContents, applyApplicationVisual } = await import("/src/content.ts");
    if (readRecentContents().join() !== "example.widget/value")
      throw Error("Legacy recent content was not removed");
    const defaultVisual = m18Config().pages[0].keys[0];
    defaultVisual.text = "01";
    const preset = { id: "fixture", name: "Application", target: { type: "uri", uri: "https://example.com" }, icon_asset_id: "a".repeat(64) + ".png" };
    applyApplicationVisual(defaultVisual, preset);
    if (defaultVisual.content_mode !== "pattern" || defaultVisual.show_text || defaultVisual.text !== "Application") throw Error("Application preset did not default to an icon without text");
    defaultVisual.content_mode = "text";
    defaultVisual.text_position = "bottom";
    applyApplicationVisual(defaultVisual, preset);
    if (defaultVisual.content_mode !== "text" || defaultVisual.text_position !== "bottom" || defaultVisual.show_text) throw Error("Application preset overwrote display preferences");
    const make = (id) => {
      const config = m18Config();
      const visual = config.pages[0].keys[0];
      visual.text = id === "a" ? "Chrome" : "Other device";
      visual.asset_id = "a".repeat(64) + ".png";
      delete visual.content_mode;
      delete visual.show_text;
      delete visual.text_position;
      return {
        profile_id: id,
        adapter_id: "mirabox.m18",
        name: `M18 ${id.toUpperCase()}`,
        device_id: `device-${id}`,
        layout: copy(m18Definition.layout),
        definition: copy(m18Definition),
        module_available: true,
        config_error: null,
        module_state: null,
        scene_status: {
          states: [],
          winner_id: null,
          retained_id: null,
          error: null,
        },
        state: {
          active_page: config.active_page,
          connection: "ready",
          error: null,
          devices: [
            {
              adapter_id: "mirabox.m18",
              layout: copy(m18Definition.layout),
              id: `device-${id}`,
              model: "M18",
              serial: `fixture-${id}`,
              firmware: "test",
              vendor_id: 0,
              product_id: 0,
              capabilities: {},
            },
          ],
          selected_device: `device-${id}`,
          config,
          input_values: {},
          pressed: [],
          logs: [],
          receipts: [],
          plugins: [],
          sdk_pid: null,
          gif_status: null,
        },
      };
    };
    const data = {
      global: { theme: "light", managed_plugin_ids: [], disabled_plugins: [] },
      devices: [make("a"), make("b")],
      discovered: [],
      plugins: [],
      logs: [],
    };
    data.discovered = data.devices.flatMap((d) => d.state.devices);
    const fixture = (window.__contentFixture = {
      w,
      data,
      calls: [],
      listeners: {},
      callbacks: {},
      failSave: false,
    });
    fixture.emit = () =>
      fixture.listeners["workspace-state"]?.({ payload: copy(data) });
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const context = canvas.getContext("2d");
    context.fillStyle = "#2563eb";
    context.fillRect(0, 0, 64, 64);
    fixture.preview = canvas.toDataURL();
    let callbackId = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) {
        fixture.callbacks[++callbackId] = callback;
        return callbackId;
      },
      async invoke(name, args = {}) {
        fixture.calls.push({ name, args: copy(args) });
        if (name === "plugin:event|listen") {
          fixture.listeners[args.event] = fixture.callbacks[args.handler];
          return args.handler;
        }
        if (name === "snapshot") return copy(data);
        if (name === "device_snapshot")
          return copy(
            data.devices.find((d) => d.profile_id === args.profileId).state,
          );
        if (name === "app_settings")
          return { close_behavior: "tray", model_appearances: {} };
        if (name === "library_snapshot")
          return { revision: 0, folders: [], items: [] };
        if (name === "applications_snapshot") return { version: 2, items: [] };
        if (name === "widget_samples") return [];
        if (name === "preview_key" || name === "preview_asset")
          return fixture.preview;
        if (name === "save_config") {
          if (fixture.failSave) throw Error("Simulated save failure");
          data.devices.find(
            (d) => d.profile_id === args.profileId,
          ).state.config.pages = copy(args.config.pages);
          fixture.emit();
        }
        return null;
      },
    };
    w.desktop = true;
    await w.init();
  });
  await page.getByRole("button", { name: "管理 M18 A", exact: true }).click();
  await page.getByRole("button", { name: "页面", exact: true }).click();
  const text = page.getByRole("textbox", { name: "文字内容", exact: true });
  const toggle = page.getByRole("switch", { name: "显示文字", exact: true });
  check(
    (await toggle.getAttribute("aria-checked")) === "true",
    "Legacy text was hidden",
  );
  check((await text.inputValue()) === "Chrome", "Legacy label changed");
  const choose = async (name) => {
    await page
      .getByRole("button", { name: "选择显示内容", exact: true })
      .click();
    await page.getByRole("option", { name, exact: true }).click();
    await page.getByRole("button", { name: "使用此内容", exact: true }).click();
  };
  await page.getByRole("button", { name: "选择显示内容", exact: true }).click();
  check(
    (await page.getByRole("option").count()) === 3,
    "Basic catalog was not split",
  );
  await page.getByRole("option", { name: "无", exact: true }).click();
  await page.getByRole("button", { name: "取消", exact: true }).click();
  check(
    (await toggle.getAttribute("aria-checked")) === "true",
    "Browsing/cancel mutated the key",
  );
  await toggle.click();
  check(
    (await text.inputValue()) === "Chrome" && (await text.isEditable()),
    "Hidden text was cleared or locked",
  );
  check(
    await page.getByRole("radio", { name: "顶部", exact: true }).isDisabled(),
    "Hidden text position remained enabled",
  );
  await text.fill("Retained label");
  await toggle.click();
  for (const position of ["顶部", "中央", "底部"])
    await page.getByRole("radio", { name: position, exact: true }).check();
  await page.waitForFunction(() =>
    window.__contentFixture.calls.some(
      (call) =>
        call.name === "save_config" &&
        call.args.config.pages[0].keys[0].text_position === "bottom",
    ),
  );
  await choose("文字");
  check(
    (await page
      .getByRole("button", { name: "导入素材", exact: true })
      .count()) === 0,
    "Text mode showed image controls",
  );
  check(
    await page.evaluate(() => !!window.__contentFixture.w.visual.asset_id),
    "Text mode removed the image",
  );
  await choose("无");
  check(
    (await text.count()) === 0 && (await toggle.count()) === 0,
    "None mode showed text controls",
  );
  await choose("图案");
  check(
    (await toggle.getAttribute("aria-checked")) === "false",
    "New Pattern mode did not default to image only",
  );
  check(
    (await text.inputValue()) === "Retained label",
    "Mode changes lost retained text",
  );
  await toggle.click();
  await page.getByRole("radio", { name: "底部", exact: true }).check();
  await choose("图案");
  check(
    (await toggle.getAttribute("aria-checked")) === "true",
    "Reselecting content reset visibility",
  );
  await page.screenshot({
    path: "output/playwright/content-pattern-text.png",
    fullPage: true,
  });
  await page.evaluate(() => {
    const w = window.__contentFixture.w;
    w.visual.asset_id = "b".repeat(64) + ".gif";
    w.edit();
  });
  check(await toggle.isDisabled(), "GIF allowed a text overlay");
  check(await toggle.getAttribute("aria-checked") === "false", "GIF switch incorrectly claimed a visible overlay");
  await page.getByText("GIF 不支持叠加文字。", { exact: true }).waitFor();
  await choose("文字");
  check(await toggle.isEnabled(), "Hidden GIF disabled pure Text mode");
  await page.evaluate(() => {
    const f = window.__contentFixture;
    f.failSave = true;
  });
  await text.fill("Unsaved label");
  check(
    await page.evaluate(
      async () => !(await window.__contentFixture.w.flushSave()),
    ),
    "Save failure was reported as success",
  );
  await page.evaluate(async () => {
    const w = window.__contentFixture.w;
    await w.selectDevice("b");
  });
  check(
    await page.evaluate(
      () => window.__contentFixture.w.visual.text === "Other device",
    ),
    "Device switch leaked the draft",
  );
  await page.evaluate(async () => {
    const w = window.__contentFixture.w;
    await w.selectDevice("a");
  });
  check(
    (await text.inputValue()) === "Unsaved label",
    "Failed draft was lost on device switch",
  );
  await page.evaluate(async () => {
    const f = window.__contentFixture;
    f.failSave = false;
    await f.w.flushSave();
    for (const notification of [...f.w.notifications]) f.w.dismissNotification(notification.id);
  });
  const previewsBefore3D = await page.evaluate(() => window.__contentFixture.calls.filter((call) => call.name === "preview_key").length);
  await page.getByRole("button", { name: "3D", exact: true }).click();
  await page.locator(".m18-model-preview canvas").waitFor();
  await page.waitForFunction((before) => {
    const calls = window.__contentFixture.calls.filter((call) => call.name === "preview_key");
    return calls.length > before && calls.slice(before).some((call) => call.args.visual.content_mode === "text" && call.args.visual.text_position === "bottom");
  }, previewsBefore3D);
  await page.screenshot({
    path: "output/playwright/content-3d.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "2D", exact: true }).click();
  await page.evaluate(() => {
    const w = window.__contentFixture.w;
    w.selectedKey = 2;
  });
  check(
    (await text.inputValue()) !== "Unsaved label",
    "Key selection reused another key's label",
  );
  await page.evaluate(() => {
    window.__contentFixture.w.selectedKey = 1;
  });
  await toggle.click();
  await page.setViewportSize({ width: 900, height: 720 });
  await page.screenshot({
    path: "output/playwright/content-narrow.png",
    fullPage: true,
  });
  check(errors.length === 0, errors.join("\n"));
  return {
    ok: true,
    mock: true,
    scenarios: [
      "legacy appearance",
      "picker cancel",
      "retained hidden text",
      "three positions",
      "three modes",
      "same-selection preservation",
      "application preset defaults",
      "GIF restrictions",
      "failed saves",
      "profile/key isolation",
      "2D/3D preview requests",
      "narrow layout",
    ],
  };
}
