// Isolated Vite browser only. Never run against a native WebView or user data.
async (page) => {
  if (await page.evaluate(() => "__TAURI_INTERNALS__" in window))
    throw new Error("Use an isolated browser preview");
  await page.reload();
  await page.setViewportSize({ width: 1180, height: 800 });
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await page.evaluate(async () => {
    const { useDesktopAlchemy } = await import("/src/store.ts");
    const {initialConfigFixture:initialConfig,m18Definition}=await import("/scripts/platform-fixtures.ts");
    const layout=m18Definition.layout;
    const w = useDesktopAlchemy();
    await w.setLanguage('zh-CN');
    const copy = (value) => JSON.parse(JSON.stringify(value));
    const make = (id) => ({
      profile_id: id,
      adapter_id: "mirabox.m18",
      name: `M18 ${id.toUpperCase()}`,
      device_id: `device-${id}`,
      layout: copy(layout),
      definition:copy(m18Definition),module_available:true,config_error:null,module_state:null,
      scene_status: {
        states: [],
        winner_id: null,
        retained_id: null,
        error: null,
      },
      state: {
        active_page: initialConfig.active_page,
        connection: "ready",
        error: null,
        devices: [
          {
            adapter_id: "mirabox.m18",
            layout: copy(layout),
            id: `device-${id}`,
            model: "M18",
            serial: id,
            firmware: "test",
            vendor_id: 0,
            product_id: 0,
            capabilities: {},
          },
        ],
        selected_device: `device-${id}`,
        config: copy(initialConfig),
        input_values:{},pressed: [],
        logs: [],
        receipts: [],
        plugins: [],
        sdk_pid: null,
        gif_status: null,
      },
    });
    const data = {
      global: { theme: "dark", managed_plugin_ids: [], disabled_plugins: [] },
      devices: [make("a"), make("b")],
      discovered: [],
      plugins: [],
      logs: [],
    };
    data.discovered = data.devices.flatMap(d => d.state.devices);
    window.__multi = {
      w,
      data,
      calls: [],
      listeners: {},
      callbacks: {},
      failA: false,
      holdA: false,
      holdPreviewA: false,
      previewReleases: [],
    };
    const f = window.__multi;
    f.emit = () => f.listeners["workspace-state"]?.({ payload: copy(data) });
    const png = (color) => {
      const c = document.createElement("canvas");
      c.width = 64;
      c.height = 64;
      c.getContext("2d").fillStyle = color;
      c.getContext("2d").fillRect(0, 0, 64, 64);
      return c.toDataURL();
    };
    f.red = png("#aa3333");
    f.green = png("#338833");
    let callbackId = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: (callback) => {
        f.callbacks[++callbackId] = callback;
        return callbackId;
      },
      invoke: async (name, args = {}) => {
        f.calls.push({ name, args: copy(args) });
        if (name === "plugin:event|listen") {
          f.listeners[args.event] = f.callbacks[args.handler];
          return args.handler;
        }
        if (name === "snapshot") return copy(data);
        if (name === "device_snapshot")
          return copy(
            data.devices.find((d) => d.profile_id === args.profileId).state,
          );
        if (name === "app_settings") return { close_behavior: "tray" };
        if (name === "library_snapshot")
          return { revision: 0, folders: [], items: [] };
        if (name === "applications_snapshot") return { version: 1, items: [] };
        if (name === "widget_samples") return [];
        if(name==="create_page"){const {newPage}=await import("/src/devices/models.ts");return newPage(args.name,data.devices.find(d=>d.profile_id===args.profileId).definition);}
        if (name === "save_config") {
          if (args.profileId === "a" && f.holdA)
            await new Promise((resolve) => {
              f.release = resolve;
            });
          if (args.profileId === "a" && f.failA)
            throw new Error("模拟磁盘写入失败");
          data.devices.find(
            (d) => d.profile_id === args.profileId,
          ).state.config.pages = copy(args.config.pages);
          f.emit();
          return null;
        }
        if (name === "save_scenes") {
          data.devices.find(
            (d) => d.profile_id === args.profileId,
          ).state.config.scene_settings = copy(args.settings);
          f.emit();
          return null;
        }
        if (name === "preview_key") {
          if (args.profileId === "a" && f.holdPreviewA)
            await new Promise((resolve) => f.previewReleases.push(resolve));
          return args.profileId === "a" ? f.red : f.green;
        }
        if (name === "hardware") {
          const device = data.devices.find(
            (d) => d.device_id === args.deviceId,
          );
          if (!device || device.state.connection !== "ready")
            throw new Error("设备已离线");
          return "test-receipt";
        }
        return null;
      },
    };
    w.desktop = true;
    await w.init();
  });
  const check = (value, message) => {
    if (!value) throw new Error(message);
  };
  const selector = { selectOption: value => page.evaluate(async id => { await window.__multi.w.selectDevice(id); }, value) };
  check(
    (await page.locator('.device-card').count()) === 2,
    "Device list is incomplete",
  );
  await page.getByRole('button',{name:'管理 M18 A',exact:true}).click();
  await page.getByRole('button',{name:'硬件测试',exact:true}).click();
  check(await page.getByRole('heading',{name:'硬件测试',exact:true}).count()===1,'M18 did not mount its hardware test page');
  await page.getByRole("button", { name: "页面", exact: true }).click();
  const name = page.getByRole("textbox", { name: "页面名称", exact: true });
  await page.evaluate(() => {
    window.__multi.holdA = true;
  });
  await name.fill("A 保存中的页面");
  await page.waitForFunction(
    () => typeof window.__multi.release === "function",
  );
  await page.evaluate(() => {
    window.__multi.switch = window.__multi.w.selectDevice("b");
  });
  check(
    (await page.evaluate(() => window.__multi.w.selectedProfileId)) === "a",
    "Switched before the pending save settled",
  );
  await page.evaluate(() => {
    window.__multi.holdA = false;
    window.__multi.release();
  });
  await page.waitForFunction(() => window.__multi.w.selectedProfileId === "b");
  check((await name.inputValue()) !== "A 保存中的页面", "A's save reached B");
  await selector.selectOption("a");
  await page.evaluate(() => {
    window.__multi.failA = true;
  });
  await name.fill("A 未保存草稿");
  await page.waitForFunction(() => !!window.__multi.w.saveError);
  await selector.selectOption("b");
  await selector.selectOption("a");
  check(
    (await name.inputValue()) === "A 未保存草稿",
    "Failed draft was lost during device switching",
  );
  check(
    await page.evaluate(() => !!window.__multi.w.saveError),
    "Save error was lost during device switching",
  );
  await page.evaluate(async () => {
    window.__multi.failA = false;
    await window.__multi.w.flushSave();
  });
  await selector.selectOption("b");
  await page.evaluate(async () => {
    await window.__multi.w.hardware({ type: "screen_brightness", value: 45 });
  });
  check(
    (await page.evaluate(
      () =>
        window.__multi.calls.filter((c) => c.name === "hardware").at(-1).args
          .deviceId,
    )) === "device-b",
    "Hardware operation followed another target",
  );
  await page.getByRole('button',{name:'场景',exact:true}).click();
  const automation=page.getByRole('switch',{name:'启用场景自动化',exact:true});
  await automation.focus();await automation.press('Space');
  await page.waitForFunction(()=>window.__multi.calls.some(c=>c.name==='save_scenes'&&c.args.profileId==='b'&&c.args.settings.enabled));
  check(await automation.getAttribute('aria-checked')==='true','Scene switch keyboard toggle failed');
  await page.getByRole('button',{name:'页面',exact:true}).click();
  await page.evaluate(() => {
    window.__multi.data.devices[0].state.pressed = [3];
    window.__multi.emit();
  });
  check(
    (await page.evaluate(() => window.__multi.w.state.pressed.length)) === 0,
    "Background input contaminated the selected device",
  );
  await page.evaluate(() => {
    window.__multi.holdPreviewA = true;
  });
  await selector.selectOption("a");
  await page.waitForFunction(() => window.__multi.previewReleases.length > 0);
  await selector.selectOption("b");
  await page.waitForFunction(
    () => document.querySelectorAll(".deck-key img").length === 15,
  );
  await page.evaluate(() => {
    window.__multi.holdPreviewA = false;
    window.__multi.previewReleases.splice(0).forEach((resolve) => resolve());
  });
  await page.waitForTimeout(200);
  check(
    await page.evaluate(() =>
      [...document.querySelectorAll(".deck-key img")].every(
        (img) => img.src === window.__multi.green,
      ),
    ),
    "Delayed preview replaced another device's image",
  );
  await page.evaluate(() => {
    const f = window.__multi;
    const d = JSON.parse(JSON.stringify(f.data.devices[0]));
    d.profile_id = "light";
    d.adapter_id = "test.light";
    d.name = "Light";
    d.device_id = "light";
    d.definition.supports_pages=false;d.definition.inputs=[];d.definition.displays=[];
    d.layout.input_keys = [];
    d.layout.display_keys = [];
    d.state.config.pages = [];
    d.state.config.active_page = "";
    d.state.active_page = "";
    d.state.selected_device = null;
    d.state.devices = [];
    f.data.devices.push(d);
    f.emit();
  });
  await selector.selectOption("light");
  check(await page.getByRole('button',{name:'硬件测试',exact:true}).count()===0,'A module without tests inherited a hardware test page');
  check(
    (await page.locator(".deck-key").count()) === 0,
    "A screenless device was forced into the M18 editor",
  );
  await page.evaluate(async () => {
    const {deviceViews}=await import('/src/devices/index.ts');
    const {markRaw,h}=await import('/node_modules/.vite/deps/vue.js');
    deviceViews['test.light']={
      controls:markRaw({render:()=>h('h1','Light controls')}),
      tests:markRaw({render:()=>h('h1','Light self-test')}),
    };
  });
  await selector.selectOption("b");
  await selector.selectOption("light");
  await page.getByRole('button',{name:'硬件测试',exact:true}).click();
  check(await page.getByRole('heading',{name:'Light self-test',exact:true}).count()===1,'A module-specific test page was not mounted');
  check(await page.getByRole('heading',{name:'Light controls',exact:true}).count()===0,'The host forced device controls into a module test page');
  await selector.selectOption("b");
  await page.evaluate(() => {
    const f = window.__multi;
    f.data.devices[1].state.connection = "disconnected";
    f.data.devices[1].state.selected_device = null;
    f.data.devices[1].state.devices = [];
    f.emit();
  });
  check(
    (await page.evaluate(() => window.__multi.w.selectedProfileId)) === "b",
    "Disconnect changed the editing target",
  );
  check(
    await page.evaluate(() => !window.__multi.w.ready),
    "Disconnected device is still ready",
  );
  await page.screenshot({
    path: "output/playwright/multi-device-dark.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 900, height: 650 });
  await page.evaluate(() => {
    window.__multi.data.global.theme = "light";
    window.__multi.emit();
  });
  await page.screenshot({
    path: "output/playwright/multi-device-light-900.png",
    fullPage: true,
  });
  check(errors.length === 0, errors.join("\n"));
  console.log(
    JSON.stringify({
      ok: true,
      scenarios: [
        "explicit targets",
        "pending save",
        "failed draft",
        "background input",
        "late preview",
        "screenless device",
        "tests disabled by default",
        "module-specific tests",
        "offline selection",
        "dark/light",
      ],
      calls: await page.evaluate(() => window.__multi.calls.length),
    }),
  );
}
