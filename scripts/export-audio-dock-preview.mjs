import { build } from "vite";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const out = new URL("../output/audio-dock-model/", import.meta.url);
const result = await build({
  configFile: false,
  logLevel: "warn",
  esbuild: { legalComments: "inline" },
  build: {
    write: false,
    lib: {
      entry: fileURLToPath(new URL("audio-dock-preview.ts", import.meta.url)),
      formats: ["iife"],
      name: "AudioDockPreview",
    },
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
});
const js = (Array.isArray(result) ? result[0] : result).output.find(
  (e) => e.type === "chunk",
).code;
const data = (
  await readFile(new URL("microsoft-audio-dock.glb", out))
).toString("base64");
const html = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,"><title>Microsoft Audio Dock · 3D 模型</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#f0f1f3;color:#24262c;font-family:"Segoe UI","Microsoft YaHei UI",sans-serif}header{padding:26px 36px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid #d9dade}h1{font-size:20px;font-weight:600;margin:0}header span{font-size:12px;color:#6b7079}main{max-width:1440px;margin:auto}#viewport{height:calc(100vh - 238px);min-height:360px}canvas{display:block;touch-action:none;cursor:grab}canvas:active{cursor:grabbing}footer{margin:0 36px;padding:20px 0;border-top:1px solid #d9dade}.tools{display:flex;align-items:center;gap:8px;flex-wrap:wrap}button{font:inherit;font-size:12px;padding:9px 15px;border:1px solid #d1d3d8;background:#fff;color:#343842;border-radius:6px;cursor:pointer}button:hover{background:#e6e8ed}button[aria-pressed=true]{border-color:#505b74;background:#e1e5ed}button:focus-visible,input:focus-visible{outline:2px solid #536cd7;outline-offset:3px}label{font-size:12px;margin-left:14px}#download{margin-left:auto}p{font-size:12px;color:#6b7079;line-height:1.7;margin:14px 0 0}a{color:inherit}@media(max-width:700px){header{padding:22px;display:block}header span{display:block;margin-top:8px}footer{margin:0 22px}#viewport{height:55vh;min-height:300px}#download{margin-left:0}label{margin-left:0}}
</style><header><h1>Microsoft Audio Dock</h1><span>独立展示模型 · GLB / glTF 2.0 · Revision 2</span></header><main><div id="viewport" aria-label="Audio Dock 三维模型"></div><footer><div class="tools"><button data-view="front" aria-pressed="true">正面斜视</button><button data-view="back">背面接口</button><button data-view="top">俯视</button><button data-view="bottom">底部</button><label><input id="cable" type="checkbox" checked> 显示线缆</label><button id="download">保存 GLB</button></div><p><span id="status">正在加载…</span><br>机身 167.6 × 80.2 × 81 mm；按官方图片重建，局部结构与缩短线缆为展示近似。<a href="https://support.microsoft.com/en-gb/surface/accessories/certified/set-up-and-use-microsoft-audio-dock">外观参考</a> · 渲染库许可证见 three-MIT.txt，图标来源与许可证见 icons/。</p></footer></main><script>window.__audioDockData="${data}";</script><script>${js.replaceAll("</script", "<\\/script")}</script></html>`;
await writeFile(new URL("audio-dock-3d-preview.html", out), html);
console.log(`Offline preview: ${Buffer.byteLength(html)} bytes`);
