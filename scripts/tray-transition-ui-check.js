// Isolated Vite browser only. These fixtures never connect to hardware/native IPC.
async (page) => {
  if (await page.evaluate(() => !!window.__TAURI_INTERNALS__?.metadata))
    throw new Error("Use an isolated browser preview");
  await page.reload();
  await page.setViewportSize({ width: 1180, height: 800 });
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await page.evaluate(async () => {
    const { useDesktopAlchemy } = await import("/src/store.ts");
    const { m18Config, m18Definition } = await import(
      "/scripts/platform-fixtures.ts"
    );
    const w = useDesktopAlchemy();
    await w.setLanguage("en");
    const make = (id) => {
      const config = m18Config();
      const descriptor = {
        id,
        adapter_id: "mirabox.m18",
        layout: m18Definition.layout,
        model: "M18",
        serial: "fixture",
        firmware: "fixture",
        vendor_id: 0,
        product_id: 0,
        capabilities: {},
      };
      return {
        profile_id: id,
        adapter_id: "mirabox.m18",
        name: `M18 ${id.toUpperCase()}`,
        device_id: id,
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
        state: {
          active_page: config.active_page,
          connection: "ready",
          error: null,
          devices: [descriptor],
          selected_device: id,
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
    w.workspace = {
      global: { theme: "dark", managed_plugin_ids: [], disabled_plugins: [] },
      devices: [make("a"), make("b")],
      discovered: [],
      plugins: [],
      logs: [],
    };
    w.workspace.discovered = w.workspace.devices.flatMap(
      (d) => d.state.devices,
    );
    const f = (window.__transition = {
      w,
      fail: false,
      hold: false,
      calls: [],
      settings: {
        a: { mode: "none", duration_ms: 300 },
        b: { mode: "none", duration_ms: 300 },
      },
    });
    w.moduleRequest = async (id, operation, payload) => {
      f.calls.push({ id, operation, payload });
      if (operation === "page_transition.set") {
        if (f.hold) await new Promise((resolve) => (f.release = resolve));
        if (f.fail) throw Error("Fixture disk write failed");
        if (JSON.stringify(payload.expected) !== JSON.stringify(f.settings[id]))
          throw Error("Saved transition changed");
        f.settings[id] = { ...payload.settings };
      }
      return { ...f.settings[id] };
    };
  });
  await page.getByRole("button", { name: "Manage M18 A", exact: true }).click();
  const mode = page.getByRole("combobox", {
    name: "Page transition",
    exact: true,
  });
  const duration = page.getByRole("slider", { name: /Duration/ });
  const direction = page.getByRole("combobox", {
    name: "Slide direction",
    exact: true,
  });
  if (await direction.count()) throw Error("Direction visible for None");
  await mode.click();
  if ((await page.getByRole("option").count()) !== 3)
    throw Error("Transition types were not grouped");
  await page.getByRole("option", { name: "Slide", exact: true }).click();
  await direction.click();
  for (const name of ["Left", "Right", "Up", "Down", "Random"]) {
    if ((await page.getByRole("option", { name, exact: true }).count()) !== 1)
      throw Error(`Missing direction: ${name}`);
  }
  await page.getByRole("option", { name: "Left", exact: true }).click();
  await duration.fill("650");
  await page
    .getByRole("button", { name: "Save transition", exact: true })
    .click();
  await page.waitForFunction(
    () => window.__transition.settings.a.mode === "slide_left",
  );
  await page.evaluate(async () => {
    await window.__transition.w.selectDevice("b");
  });
  if (!(await mode.textContent()).includes("None"))
    throw Error("Slide settings leaked into B");
  await page.evaluate(async () => {
    await window.__transition.w.selectDevice("a");
  });
  await duration.waitFor();
  if (
    !(await mode.textContent()).includes("Slide") ||
    !(await direction.textContent()).includes("Left") ||
    (await duration.inputValue()) !== "650"
  )
    throw Error("Saved slide settings did not reload");
  await direction.click();
  await page.getByRole("option", { name: "Random", exact: true }).click();
  await page
    .getByRole("button", { name: "Save transition", exact: true })
    .click();
  await page.waitForFunction(
    () => window.__transition.settings.a.mode === "slide_random",
  );
  await page.evaluate(async () => {
    await window.__transition.w.selectDevice("b");
    await window.__transition.w.selectDevice("a");
  });
  await direction.waitFor();
  if (!(await direction.textContent()).includes("Random"))
    throw Error("Random direction did not reload");
  await page.locator(".page-transition-settings").screenshot({
    path: "output/playwright/m18-slide-direction.png",
  });
  await mode.click();
  await page.getByRole("option", { name: "Fade in", exact: true }).click();
  if (await direction.count()) throw Error("Direction visible for Fade in");
  await duration.fill("450");
  await page.evaluate(() => (window.__transition.fail = true));
  await page
    .getByRole("button", { name: "Save transition", exact: true })
    .click();
  await page
    .getByRole("alert")
    .filter({ hasText: "Fixture disk write failed" })
    .waitFor();
  await page.evaluate(async () => {
    await window.__transition.w.selectDevice("b");
  });
  if (!(await mode.textContent()).includes("None"))
    throw Error("A draft leaked into B");
  await page.evaluate(async () => {
    await window.__transition.w.selectDevice("a");
  });
  await duration.waitFor();
  if ((await duration.inputValue()) !== "450")
    throw Error("Failed transition draft was lost");
  await page.evaluate(() => {
    window.__transition.fail = false;
    window.__transition.hold = true;
  });
  await page
    .getByRole("button", { name: "Save transition", exact: true })
    .click();
  await page.waitForFunction(() => !!window.__transition.release);
  await page.evaluate(async () => {
    await window.__transition.w.selectDevice("b");
    window.__transition.release();
  });
  if (!(await mode.textContent()).includes("None"))
    throw Error("Late A save changed B");
  await page.evaluate(async () => {
    window.__transition.hold = false;
    await window.__transition.w.selectDevice("a");
  });
  await duration.waitFor();
  if ((await duration.inputValue()) !== "450")
    throw Error("Saved duration was not restored");
  await duration.fill("500");
  await page.evaluate(() => (window.__transition.settings.a.duration_ms = 600));
  await page
    .getByRole("button", { name: "Save transition", exact: true })
    .click();
  await page
    .getByRole("alert")
    .filter({ hasText: "Saved transition changed" })
    .waitFor();
  await page
    .getByRole("button", { name: "Read latest settings", exact: true })
    .click();
  if ((await duration.inputValue()) !== "500")
    throw Error("Reading latest settings discarded the draft");
  await page
    .getByRole("button", { name: "Save transition", exact: true })
    .click();
  await page.evaluate(
    () =>
      (window.__transition.w.workspace.devices[0].state.connection =
        "disconnected"),
  );
  await page.waitForFunction(
    () =>
      document.querySelector('.page-transition-settings [role="combobox"]')
        ?.disabled === true,
  );
  await page.evaluate(
    () =>
      (window.__transition.w.workspace.devices[0].state.connection = "ready"),
  );
  await mode.waitFor({ state: "visible" });
  await mode.click();
  await page.getByRole("option", { name: "Slide", exact: true }).click();
  await direction.waitFor();
  if (!(await direction.textContent()).includes("Random"))
    throw Error("Switching animation type lost the selected direction");
  await page.evaluate(() => window.__transition.w.setLanguage("zh-CN"));
  if (!(await page.getByRole("combobox", { name: "滑动方向", exact: true }).textContent()).includes("随机"))
    throw Error("Chinese direction labels did not update");
  await page.locator(".page-transition-settings").screenshot({
    path: "output/playwright/m18-slide-direction-zh.png",
  });
  await page.screenshot({
    path: "output/playwright/m18-page-transition.png",
    fullPage: true,
  });

  // Test the actual tray component with its restricted event/ready contract.
  await page.addInitScript(() => {
    window.isTauri = true;
    let id = 0;
    const callbacks = {};
    const listeners = {};
    const state = {
      connection: "ready",
      theme: "dark",
      devices: [{ profile_id: "one", name: "M18", connection: "ready" }],
    };
    window.__trayFixture = {
      state,
      emit(next) {
        listeners["tray-state"]?.({ payload: next });
      },
    };
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) {
        callbacks[++id] = callback;
        return id;
      },
      unregisterCallback(callbackId) {
        delete callbacks[callbackId];
      },
      async invoke(command, args = {}) {
        if (command === "ui_language")
          return { preference: "en", locale: "en" };
        if (command === "plugin:event|listen") {
          listeners[args.event] = callbacks[args.handler];
          return args.handler;
        }
        if (command === "tray_panel_ready") return state;
        return null;
      },
    };
  });
  await page.goto(new URL("/tray.html", page.url()).href);
  await page.setViewportSize({ width: 276, height: 374 });
  await page
    .getByRole("status")
    .filter({ hasText: "1 of 1 connected" })
    .waitFor();
  await page.evaluate(() =>
    window.__trayFixture.emit({
      connection: "ready",
      theme: "dark",
      devices: [
        { profile_id: "one", name: "M18", connection: "ready" },
        { profile_id: "two", name: "M18", connection: "disconnected" },
        {
          profile_id: "dock",
          name: "Microsoft Audio Dock",
          connection: "reconnecting",
        },
        ...Array.from({ length: 5 }, (_, i) => ({
          profile_id: `offline-${i}`,
          name: `Offline device ${i}`,
          connection: "disconnected",
        })),
      ],
    }),
  );
  await page
    .getByRole("status")
    .filter({ hasText: "1 of 8 connected" })
    .waitFor();
  await page.getByText("M18 · 2", { exact: true }).waitFor();
  const layout = await page.evaluate(() => {
    const panel = document.querySelector(".tray-panel").getBoundingClientRect();
    const list = document.querySelector(".tray-devices");
    return {
      fits: panel.bottom <= innerHeight && panel.right <= innerWidth,
      scrolls: list.scrollHeight > list.clientHeight,
    };
  });
  if (!layout.fits || !layout.scrolls)
    throw Error("Tray list overflows native panel dimensions");
  await page.screenshot({
    path: "output/playwright/tray-multiple-devices.png",
  });
  await page.getByRole("list", { name: "Device list" }).focus();
  await page.keyboard.press("End");
  await page.evaluate(() =>
    window.__trayFixture.emit({
      connection: "disconnected",
      theme: "light",
      devices: [],
    }),
  );
  await page.getByRole("status").filter({ hasText: "No devices" }).waitFor();
  if (errors.length) throw Error(errors.join("\n"));
  return {
    settings:
      "slide directions and persistence, save failure, draft retention, target isolation, late response, saved-setting conflict, offline",
    tray: "single, multiple, duplicate names, live updates, scroll, empty",
    native: false,
  };
}
