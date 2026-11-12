import { build } from "vite";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
const result = await build({
  configFile: false,
  logLevel: "warn",
  esbuild: { legalComments: "inline" },
  build: {
    write: false,
    lib: {
      entry: resolve("scripts/m18-preview.ts"),
      formats: ["iife"],
      name: "M18Prototype",
    },
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
});
const bundle = (Array.isArray(result) ? result[0] : result).output.find(
  (entry) => entry.type === "chunk",
).code;
const license = await readFile("licenses/three-MIT.txt", "utf8");
const html = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='8' fill='%235966f3'/%3E%3Cpath d='M8 8h6v6H8zm10 0h6v6h-6zM8 18h6v6H8zm10 0h6v6h-6z' fill='white'/%3E%3C/svg%3E"><title>M18 3D 样机 · Desktop Alchemy</title>
<style>
:root{font-family:"Segoe UI Variable","Segoe UI","Microsoft YaHei UI",sans-serif;color:#171e30;background:#f3f5f9;font-synthesis:none}*{box-sizing:border-box}body{margin:0;padding:28px}header{max-width:1320px;margin:0 auto 24px;display:flex;justify-content:space-between;align-items:baseline;gap:20px}header strong{font-size:16px}header span{font-size:12px;color:#5e687d}main{max-width:1320px;margin:auto;display:grid;grid-template-columns:minmax(0,1fr) 270px;gap:20px}.stage,aside{background:#fff;border:1px solid #dadde7;border-radius:20px}.stage{overflow:hidden}#viewport{height:min(68vh,650px);min-height:430px;background:radial-gradient(ellipse at 50% 60%,#edeff5,transparent 70%)}canvas{display:block;width:100%;height:100%;cursor:grab;touch-action:none}canvas:active{cursor:grabbing}.bar{padding:15px 22px;display:flex;align-items:center;justify-content:space-between;gap:12px;border-top:1px solid #dadde7;color:#5e687d;font-size:12px}aside{padding:24px}h1{font-size:29px;margin:3px 0 9px;letter-spacing:-.7px}p{font-size:13px;line-height:1.8;color:#5e687d;margin:0 0 20px}.tag{font-size:10px;color:#4e5adb;font-weight:600;letter-spacing:1px}.dimensions{display:block;border-bottom:1px solid #dadde7;padding-bottom:22px;margin-bottom:22px;font-size:14px;font-variant-numeric:tabular-nums}h2{font-size:12px;margin:22px 0 12px}button{font:inherit;font-size:12px;border:1px solid #dadde7;border-radius:8px;background:#fff;padding:7px 10px;color:#5e687d;cursor:pointer}button:hover{background:#edeff5}button:focus-visible,#viewport:focus-visible{outline:2px solid #5966f3;outline-offset:2px}[aria-pressed=true]{color:#4e5adb;background:#eef0ff;border-color:#aab1ff}#keys{display:grid;grid-template-columns:repeat(6,1fr);gap:6px}#keys button{padding:7px 0}#selected{font-size:13px;color:#4e5adb;font-weight:600}.views{display:flex;gap:6px;flex-wrap:wrap}.note{font-size:11px;margin-top:24px;padding-top:18px;border-top:1px solid #dadde7}details{max-width:1320px;margin:20px auto;color:#5e687d;font-size:10px}pre{white-space:pre-wrap}@media(max-width:900px){body{padding:16px}main{grid-template-columns:1fr}aside{display:block}#viewport{height:57vh;min-height:320px}.bar{flex-wrap:wrap}header{align-items:start}header span{text-align:right}}
</style><header><strong>Desktop Alchemy</strong><span>M18 · 3D 视觉模型 · Revision 2</span></header><main><section class="stage"><div id="viewport" tabindex="0" role="group" aria-label="M18 三维模型，拖动旋转，方向键选择按键"></div><div class="bar"><span id="status">正在加载模型…</span><div class="views"><button data-view="perspective">斜视</button><button data-view="top">俯视</button><button data-view="bottom">底部</button></div></div></section><aside><span class="tag">M18</span><h1>设备模型</h1><p>根据实拍照片与商品图制作，实体键表面采用浅凹曲面。</p><strong class="dimensions">117 × 104 × 21 mm</strong><span id="selected">显示键 01</span><h2>选择按键</h2><div id="keys"></div><p class="note">整体尺寸采用提供的数据；按键间距、倒角和底部结构按照片估算。支架处于折叠状态，顶部使用通用 M18 标记。静态灯效下模型保持静止时停止绘制。</p></aside></main><details><summary>渲染库许可证 · Three.js MIT</summary><pre>${license.replaceAll("&", "&amp;").replaceAll("<", "&lt;")}</pre></details><script>${bundle.replaceAll("</script", "<\\/script")}</script></html>`;
await mkdir("output/m18-model", { recursive: true });
const ledControls = `<h2>灯效预览</h2><div id="led-modes" class="views" role="group" aria-label="灯效"></div><div style="display:grid;gap:10px;margin-top:14px;font-size:12px;color:#5e687d"><label style="display:flex;align-items:center;gap:10px">LED 亮度<input id="led-brightness" type="range" min="0" max="255" style="min-width:0;flex:1;accent-color:#5966f3"></label><label style="display:flex;align-items:center;gap:10px">灯效速度<input id="led-speed" type="range" min="1" max="10" style="min-width:0;flex:1;accent-color:#5966f3"></label><label style="display:flex;align-items:center;gap:10px">主色<input id="led-color" type="color" value="#5966f3"></label></div><p style="font-size:11px;margin-top:12px">软件灯效预览；各灯位置按模型估算。</p>`;
const previewHtml = html.replace(
  '<p class="note">',
  `${ledControls}<p class="note">`,
);
await writeFile("output/m18-model/m18-3d-preview.html", previewHtml);
await copyFile("src/assets/devices/m18/m18.glb", "output/m18-model/m18.glb");
await copyFile(
  "src/assets/devices/m18/m18-poster.png",
  "output/m18-model/m18-product.png",
);
console.log(`Offline preview: ${Buffer.byteLength(previewHtml)} bytes`);
