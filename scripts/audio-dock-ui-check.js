// Isolated Vite browser only. Never inject IPC into a native WebView.
async (page) => {
  if (await page.evaluate(() => !!window.__TAURI_INTERNALS__?.metadata)) throw Error('Use an isolated browser');
  await page.reload();
  await page.setViewportSize({width:1180,height:800});
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.evaluate(async()=>{
    const {useDesktopAlchemy}=await import('/src/store.ts');
    const {initialConfig}=await import('/src/devices/models.ts');
    const copy=x=>JSON.parse(JSON.stringify(x));
    const layout={input_keys:[1],display_keys:[],columns:0,key_width:0,key_height:0,background_width:0,background_height:0,led_count:0};
    const make=id=>{
      const descriptor={id,adapter_id:'microsoft.audio-dock',layout,model:`Dock ${id.toUpperCase()}`,serial:'fixture',firmware:'1.47.139',vendor_id:0x045e,product_id:0x084d,capabilities:{}};
      return {config_error:null,module_state:null,module_available:true,profile_id:id,adapter_id:descriptor.adapter_id,name:descriptor.model,device_id:id,layout,definition:{adapter_id:descriptor.adapter_id,name:descriptor.model,layout,supports_pages:false,input_names:{1:'Teams 键'},inputs:[{id:'1',name:'Teams 键',kind:{type:'button'}}],displays:[]},scene_status:{states:[],winner_id:null,retained_id:null,error:null},state:{connection:'ready',error:null,devices:[descriptor],selected_device:id,active_page:'',config:{...copy(initialConfig),pages:[],active_page:'',bindings:[{control_id:'1',enabled:false,trigger:{type:'press'},action:{type:'none'}}]},pressed:[],last_key_press:null,logs:[],receipts:[],plugins:[],sdk_pid:null,gif_status:null}};
    };
    const devices=['a','b'].map(make);
    const data={global:{theme:'dark',managed_plugin_ids:[],disabled_plugins:[]},devices,discovered:devices.map(d=>d.state.devices[0]),plugins:[],logs:[]};
    const state=id=>({device_id:id,connection:'ready',error:null,info:{model:'Audio Dock',serial:`fixture-${id}`,hardware_version:'3.1.9',color_code:'black',firmware_version:'1.47.139'},settings:{prompt_tone:true,prompt_volume:id==='a'?39:63,push_to_talk:true,noise_suppression:'standard',active_equalizer:0,equalizer_presets:['Flat','Classic','Jazz','Pop','Rock'].map((name,id)=>({id,name,gains:[0,0,0,0,0],factory:true})),free_equalizer_slots:[5,6,7,8,9,10,11,12,13,14,15]}});
    const f=window.__dockMock={w:useDesktopAlchemy(),data,states:{a:state('a'),b:state('b')},calls:[],listeners:{},callbacks:{},hold:false,fail:false};let next=0;
    await f.w.setLanguage('zh-CN');
    for (const device of devices) {
      device.module_state=copy(f.states[device.device_id]);
      device.state.devices[0].serial=f.states[device.device_id].info.serial;
      device.state.devices[0].firmware=f.states[device.device_id].info.firmware_version;
    }
    f.emit=()=>f.listeners['workspace-state']?.({payload:copy(data)});
    window.__TAURI_INTERNALS__={transformCallback:cb=>{f.callbacks[++next]=cb;return next;},invoke:async(name,args={})=>{
      f.calls.push({name,args:copy(args)});
      if(name==='plugin:event|listen'){f.listeners[args.event]=f.callbacks[args.handler];return args.handler;}
      if(name==='snapshot')return copy(data);
      if(name==='device_snapshot')return copy(devices.find(d=>d.profile_id===args.profileId).state);
      if(name==='app_settings')return {close_behavior:'tray',model_appearances:{}};
      if(name==='library_snapshot')return {revision:0,folders:[],items:[]};
      if(name==='applications_snapshot')return {version:2,items:[{id:'preset-example',name:'预设示例',target:{type:'registered_application',identity:{type:'apps_folder',parsing_name:'Fixture.Preset!App'},arguments:''},icon_asset_id:null}]};
      if(name==='widget_samples')return [];
      if(name==='registered_applications')return [{id:'fixture-app',name:'示例应用',target:{type:'registered_application',identity:{type:'apps_folder',parsing_name:'Hidden.Identity!App'},arguments:''},icon_asset_id:null}];
      if(name==='module_request'&&args.operation==='read'){
        const value=copy(f.states[args.deviceId]);
        const firstRead=f.calls.filter(c=>c.name==='module_request'&&c.args.operation==='read').length===1;
        if(firstRead){
          devices.find(d=>d.device_id===args.deviceId).module_state={...copy(value),connection:'loading',settings:null};
          f.emit();
          await new Promise(resolve=>setTimeout(resolve,100));
          devices.find(d=>d.device_id===args.deviceId).module_state=copy(value);
          f.emit();
        }
        if(f.hold&&args.deviceId==='a')await new Promise(resolve=>{f.release=resolve;});
        return value;
      }
      if(name==='module_request'&&args.operation==='update'){
        if(f.fail)throw Error('模拟读回失败');
        const s=f.states[args.deviceId].settings,u=args.payload;
        if(u.type==='prompt_volume')s.prompt_volume=u.value;
        if(u.type==='prompt_tone')s.prompt_tone=u.enabled;
        if(u.type==='push_to_talk')s.push_to_talk=u.enabled;
        if(u.type==='select_equalizer')s.active_equalizer=u.id;
        if(u.type==='save_equalizer'){
          const id=u.id??s.free_equalizer_slots[0];
          s.equalizer_presets=s.equalizer_presets.filter(p=>p.id!==id).concat([{id,name:u.name,gains:u.gains,factory:false}]);
          s.free_equalizer_slots=s.free_equalizer_slots.filter(v=>v!==id);s.active_equalizer=id;
        }
        if(u.type==='delete_equalizer'){
          s.equalizer_presets=s.equalizer_presets.filter(p=>p.id!==u.id);
          s.free_equalizer_slots.push(u.id);
          if(s.active_equalizer===u.id)s.active_equalizer=0;
        }
        return copy(f.states[args.deviceId]);
      }
      if(name==='save_input_bindings'){devices.find(d=>d.profile_id===args.profileId).state.config.bindings=copy(args.bindings);f.emit();return null;}
      return null;
    }};
    f.w.desktop=true;await f.w.init();
  });

  const check=(condition,message)=>{if(!condition)throw Error(message);};
  await page.getByRole('button',{name:'管理 Dock A',exact:true}).click();
  check(await page.getByRole('button',{name:'硬件测试',exact:true}).count()===0,'Audio Dock exposed a disabled hardware test page');
  const volume=()=>page.getByRole('slider',{name:'提示音音量',exact:true});
  await page.waitForFunction(()=>document.querySelector('[aria-label="提示音音量"]')?.value==='39');
  check(await page.evaluate(()=>window.__dockMock.calls.filter(c=>c.name==='module_request'&&c.args.operation==='read').length===1),'Loading/ready snapshots restarted the settings read');
  await volume().focus();await volume().press('ArrowRight');
  await page.waitForFunction(()=>window.__dockMock.states.a.settings.prompt_volume===40);
  check(await page.evaluate(()=>window.__dockMock.calls.some(c=>c.name==='module_request'&&c.args.deviceId==='a'&&c.args.operation==='update'&&c.args.payload.type==='prompt_volume')),'Settings did not use scoped module RPC');
  await page.getByRole('button',{name:'均衡器',exact:true}).click();
  await page.getByRole('slider',{name:'低频增益',exact:true}).fill('1');
  await page.getByRole('button',{name:'Teams 键',exact:true}).click();
  await page.getByRole('button',{name:'均衡器',exact:true}).click();
  check(await page.getByRole('slider',{name:'低频增益',exact:true}).inputValue()==='1','Subpage change discarded the EQ draft');
  await page.evaluate(async()=>{const f=window.__dockMock;f.hold=true;const id='a';void f.w.moduleRequest(id,'read');});
  await page.waitForFunction(()=>!!window.__dockMock.release);
  await page.evaluate(async()=>{await window.__dockMock.w.selectDevice('b');});
  await page.getByRole('button',{name:'音频设置',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('[aria-label="提示音音量"]')?.value==='63');
  await page.evaluate(()=>{window.__dockMock.hold=false;window.__dockMock.release();});
  check(await volume().inputValue()==='63','Late reply changed the selected device');
  await page.getByRole('button',{name:'Teams 键',exact:true}).click();
  await page.getByRole('switch',{name:'启用 Teams 键映射',exact:true}).click();
  await page.waitForFunction(()=>window.__dockMock.calls.some(c=>c.name==='save_input_bindings'&&c.args.profileId==='b'&&Array.isArray(c.args.bindings)));
  check(await page.evaluate(()=>window.__dockMock.calls.filter(c=>c.name==='save_input_bindings').every(c=>c.args.profileId==='b')),'Bindings were saved to another device');
  await page.getByRole('button',{name:'音频设置',exact:true}).click();
  await page.evaluate(()=>{const f=window.__dockMock;f.data.devices.find(d=>d.profile_id==='b').state.connection='disconnected';f.emit();});
  await page.waitForFunction(()=>document.querySelector('[aria-label="提示音音量"]')?.disabled);
  await page.screenshot({path:'output/playwright/platform-audio-dock.png',fullPage:true});
  check(errors.length===0,errors.join('\n'));
  return {ok:true,mock:true,scenarios:['module disables hardware tests','loading/ready snapshots do not restart reads','module RPC','explicit target','EQ draft retained','late reply isolation','typed button binding','offline settings disabled']};
}
