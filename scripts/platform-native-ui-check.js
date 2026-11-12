// Real Tauri diagnostics instance only, with an explicit isolated root and hardware disabled.
async (page) => {
  const safe = await page.evaluate(async () => {
    const { invoke } = await import('/node_modules/.vite/deps/@tauri-apps_api_core.js');
    const s = await invoke('snapshot');
    return window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label === 'main'
      && s.devices.length === 2 && s.devices.every(d=>!d.config_error && d.state.sdk_pid === null && d.state.devices.length === 0)
      && s.logs.some(l=>l.message==='原生 UI 隔离测试：设备连接已禁用');
  });
  if (!safe) throw new Error('Requires isolated diagnostics instance');
  await page.evaluate(async () => {
    const {invoke}=await import('/node_modules/.vite/deps/@tauri-apps_api_core.js');
    await invoke('set_ui_language',{preference:'zh-CN'});
  });
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  for (const id of ['a','b']) {
    await page.getByRole('button',{name:'设备',exact:true}).click();
    const summary=page.locator('.saved-profiles summary');
    if(!await page.locator('.saved-profiles').evaluate(e=>e.open)) await summary.click();
    await page.getByRole('button',{name:`打开配置 M18 ${id.toUpperCase()}`,exact:true}).click();
    await page.getByRole('button',{name:'页面',exact:true}).click();
    await page.getByRole('textbox',{name:'页面名称',exact:true}).fill(`平台原生 ${id.toUpperCase()}`);
    await page.waitForFunction(async (id) => {
      const {invoke}=await import('/node_modules/.vite/deps/@tauri-apps_api_core.js');
      const s=await invoke('device_snapshot',{profileId:id});
      return s.config.pages[0].name===`平台原生 ${id.toUpperCase()}`;
    }, id);
  }
  const result=await page.evaluate(async () => {
    const {invoke}=await import('/node_modules/.vite/deps/@tauri-apps_api_core.js');
    const s=await invoke('snapshot');
    for(const d of s.devices) if(d.state.config.pages[0].name!==`平台原生 ${d.profile_id.toUpperCase()}`) throw new Error('Native save crossed devices');
    const page=await invoke('create_page',{profileId:'a',name:'后端模板'});
    if(page.keys.length!==18||page.bindings.length!==18) throw new Error('Backend template missing controls');
    let rejected=0;
    for(const args of [{command:{type:'firmware'}},{deviceId:'unknown',command:{type:'firmware'}},{deviceId:'test-a',command:{type:'firmware'}}]) {
      try{await invoke('hardware',args);}catch{rejected++;}
    }
    if(rejected!==3) throw new Error('Invalid target accepted');
    try{await invoke('module_request',{deviceId:'test-a',operation:'unknown',payload:null});throw new Error('Unknown operation accepted');}
    catch(error){if(String(error).includes('accepted'))throw error;}
    return {ok:true,transport:'real Tauri',hardware:false,profiles:2,saves:2,backendPageFactory:true};
  });
  await page.getByText('所有更改已保存',{exact:true}).waitFor();
  await page.screenshot({path:'output/playwright/platform-native.png',fullPage:true});
  if(errors.length)throw new Error(errors.join('\n'));
  return result;
}
