// Browser/model authoring fixtures only. Product defaults come from Rust adapters.
import { initialConfig, newPage } from "../src/devices/models";
import type { DeviceDefinition, AppConfig } from "../src/bindings";
export const m18Definition: DeviceDefinition = {
  adapter_id: "mirabox.m18", name: "M18", supports_pages: true,
  layout: { input_keys: Array.from({length:18},(_,i)=>i+1), display_keys:Array.from({length:15},(_,i)=>i+1),columns:5,key_width:64,key_height:64,background_width:480,background_height:272,led_count:24 },
  input_names: {}, inputs:Array.from({length:18},(_,i)=>({id:String(i+1),name:`按键 ${i+1}`,kind:{type:"button"}})),
  displays:Array.from({length:15},(_,i)=>({id:String(i+1),key:i+1,width:64,height:64,column:i%5,row:Math.floor(i/5)})),
};
export function m18Config(): AppConfig {
  const page=newPage("主页",m18Definition);
  return {...structuredClone(initialConfig),pages:[page],active_page:page.id,device_settings:{screen_brightness:70,led_brightness:100,led_color:"#5966F3",led_mode:"solid",led_speed:5,led_colors:Array.from({length:24},()=>[89,102,243])}};
}
export const initialConfigFixture=m18Config();
export const initialFixture=initialConfigFixture.pages[0];
