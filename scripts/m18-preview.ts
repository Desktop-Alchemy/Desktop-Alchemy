import { M18Scene } from "../src/devices/m18/scene3d";
import {initialFixture as initial,initialConfigFixture as initialConfig} from "./platform-fixtures";
import type { LedMode } from "../src/bindings";
import type { LedPreviewSettings } from "../src/devices/m18/led-preview";

const host = document.querySelector<HTMLElement>("#viewport")!;
let selected = 1;
const select = (key: number) => {
  selected = key;
  scene.setSelection(key, []);
  document.querySelector("#selected")!.textContent =
    `${key <= 15 ? "显示键" : "实体键"} ${String(key).padStart(2, "0")}`;
  document
    .querySelectorAll<HTMLButtonElement>("[data-key]")
    .forEach((button) => {
      button.setAttribute(
        "aria-pressed",
        String(Number(button.dataset.key) === key),
      );
    });
};
let scene: M18Scene;
const leds: LedPreviewSettings = structuredClone(initialConfig.device_settings);
async function start() {
  try {
    scene = new M18Scene(host, select, (message) => {
      document.querySelector("#status")!.textContent = message;
    });
    await scene.load();
    scene.setPage(initial, {});
    scene.setLedSettings(leds);
    select(1);
    document.querySelector("#status")!.textContent =
      "拖动旋转 · 滚轮缩放 · 点击按键选择";
    (window as unknown as { __m18Scene: M18Scene }).__m18Scene = scene;
  } catch {
    document.querySelector("#status")!.textContent =
      "当前浏览器无法启动 3D 渲染，请查看随附的静态预览图。";
  }
}
document
  .querySelectorAll<HTMLButtonElement>("[data-view]")
  .forEach((button) => {
    button.addEventListener("click", () =>
      scene?.setView(button.dataset.view as "top" | "bottom" | "perspective"),
    );
  });
const keys = document.querySelector("#keys")!;
for (let i = 1; i <= 18; i++) {
  const button = document.createElement("button");
  button.textContent = String(i).padStart(2, "0");
  button.dataset.key = String(i);
  button.title = i <= 15 ? `显示键 ${i}` : `实体键 ${i}`;
  button.addEventListener("click", () => select(i));
  keys.append(button);
}
const modes: [LedMode, string][] = [
  ["solid", "常亮"],
  ["breathing", "呼吸"],
  ["color_cycle", "颜色循环"],
  ["rainbow", "彩虹"],
  ["wave", "波浪"],
  ["comet", "彗星"],
];
for (const [mode, label] of modes) {
  const button = document.createElement("button");
  button.textContent = label;
  button.dataset.ledMode = mode;
  button.setAttribute("aria-pressed", String(mode === leds.led_mode));
  button.addEventListener("click", () => {
    leds.led_mode = mode;
    document
      .querySelectorAll<HTMLButtonElement>("[data-led-mode]")
      .forEach((item) =>
        item.setAttribute(
          "aria-pressed",
          String(item.dataset.ledMode === mode),
        ),
      );
    scene?.setLedSettings(leds);
  });
  document.querySelector("#led-modes")!.append(button);
}
for (const [id, field] of [
  ["led-brightness", "led_brightness"],
  ["led-speed", "led_speed"],
] as const) {
  const input = document.querySelector<HTMLInputElement>(`#${id}`)!;
  input.value = String(leds[field]);
  input.addEventListener("input", () => {
    leds[field] = Number(input.value);
    scene?.setLedSettings(leds);
  });
}
document
  .querySelector<HTMLInputElement>("#led-color")!
  .addEventListener("input", (event) => {
    leds.led_color = (event.target as HTMLInputElement).value;
    scene?.setLedSettings(leds);
  });
host.addEventListener("keydown", (event) => {
  const delta =
    event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
  if (!delta) return;
  event.preventDefault();
  select(((selected - 1 + delta + 18) % 18) + 1);
});
window.addEventListener("pagehide", () => scene?.dispose(), { once: true });
void start();
