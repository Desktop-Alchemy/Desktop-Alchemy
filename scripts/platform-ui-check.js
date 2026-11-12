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

  const check=(condition,message)=>{if(!condition)throw Error(message);};
  await page.evaluate(async()=>{
    const f=window.__multi;const {newPage,initialConfig}=await import('/src/devices/models.ts');
    const layout={input_keys:Array.from({length:32},(_,i)=>i+1),display_keys:Array.from({length:32},(_,i)=>i+1),columns:8,key_width:80,key_height:80,background_width:0,background_height:0,led_count:0};
    const definition={adapter_id:'example.simulator',name:'Platform Simulator',supports_pages:true,layout,input_names:{},inputs:layout.input_keys.map(key=>({id:String(key),name:`按键 ${key}`,kind:{type:'button'}})).concat([{id:'knob',name:'旋钮',kind:{type:'rotary'}},{id:'slider',name:'滑杆',kind:{type:'analog',min:0,max:100,unit:'%'}}]),displays:layout.display_keys.map((key,i)=>({id:String(key),key,width:key===32?160:80,height:80,column:i%8,row:Math.floor(i/8)}))};
    for(const d of f.data.devices){const page=newPage('主页',definition);d.adapter_id=definition.adapter_id;d.layout=layout;d.definition=definition;d.state.config={...JSON.parse(JSON.stringify(initialConfig)),pages:[page],active_page:page.id};d.state.active_page=page.id;d.state.devices[0].adapter_id=definition.adapter_id;d.state.devices[0].layout=layout;}
    f.emit();await f.w.selectDevice('b');await f.w.selectDevice('a');
  });
  await page.getByRole('button',{name:'管理 M18 A',exact:true}).click();
  await page.getByRole('button',{name:'页面',exact:true}).click();
  check(await page.locator('.generic-device-grid button').count()===32,'Generic display definition was ignored');
  const rotary=page.locator('.input-control').filter({has:page.getByText('旋钮',{exact:true})});
  await rotary.getByRole('button',{name:'添加动作',exact:true}).click();
  await rotary.getByRole('combobox').first().selectOption('counterclockwise');
  const analog=page.locator('.input-control').filter({has:page.getByText('滑杆',{exact:true})});
  await analog.getByRole('button',{name:'添加动作',exact:true}).click();
  await analog.getByRole('spinbutton',{name:'阈值',exact:true}).fill('0.7');
  await analog.getByRole('spinbutton',{name:'阈值',exact:true}).press('Tab');
  await page.waitForFunction(()=>window.__multi.calls.some(c=>c.name==='save_config'&&c.args.config.pages[0].bindings.some(b=>b.control_id==='slider'&&b.trigger.threshold===0.7)));
  await page.getByRole('button',{name:'创建页面',exact:true}).click();
  await page.waitForFunction(()=>window.__multi.w.draft.pages.length===2);
  check(await page.evaluate(()=>window.__multi.calls.some(c=>c.name==='create_page'&&c.args.profileId==='a')),'Page defaults did not come from backend');
  check(await page.evaluate(()=>window.__multi.w.page.keys.length===32),'New generic page has an empty layout');
  await page.getByRole('heading',{name:'页面编辑器',exact:true}).scrollIntoViewIfNeeded();
  await page.screenshot({path:'output/playwright/platform-generic-edit.png',fullPage:true});
  await page.evaluate(()=>{const f=window.__multi;f.data.devices[0].config_error='模块配置版本不支持';f.emit();});
  await page.getByRole('alert').filter({hasText:'配置只读'}).waitFor();
  check(await page.locator('.input-control button').first().isDisabled(),'Unavailable profile remains editable');
  await page.screenshot({path:'output/playwright/platform-generic.png',fullPage:true});
  check(errors.length===0,errors.join('\n'));
  return {ok:true,mock:true,scenarios:['32 displays','rotary binding','analog threshold','backend page factory','read-only unsupported profile']};
}
