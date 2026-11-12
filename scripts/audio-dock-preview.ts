import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";

declare global {
  interface Window {
    __audioDockData: string;
    __audioDockPreview: {
      ready: boolean;
      error?: string;
      view: (name: string) => void;
      capture: () => string;
      stats: unknown;
    };
  }
}
const host = document.querySelector<HTMLElement>("#viewport")!;
const status = document.querySelector<HTMLElement>("#status")!;
async function start() {
  const renderer = new THREE.WebGLRenderer({
    antialias: true,
    alpha: true,
    preserveDrawingBuffer: true,
  });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.setClearColor(0xf0f1f3, 1);
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.05;
  host.append(renderer.domElement);
  const scene = new THREE.Scene();
  const pmrem = new THREE.PMREMGenerator(renderer);
  const room = new RoomEnvironment();
  const env = pmrem.fromScene(room, 0.04);
  scene.environment = env.texture;
  scene.environmentIntensity = 0.5;
  room.dispose();
  pmrem.dispose();
  const hemi = new THREE.HemisphereLight(0xffffff, 0x666976, 0.8);
  scene.add(hemi);
  const key = new THREE.DirectionalLight(0xffffff, 1.8);
  key.position.set(-0.2, 0.3, 0.3);
  scene.add(key);
  const fill = new THREE.DirectionalLight(0xd9e6ff, 1.2);
  fill.position.set(0.2, 0.15, -0.2);
  scene.add(fill);
  const camera = new THREE.PerspectiveCamera(34, 1, 0.001, 5);
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.target.set(-0.012, 0.04, -0.008);
  controls.minDistance = 0.18;
  controls.maxDistance = 0.8;
  controls.enableDamping = false;
  const render = () => renderer.render(scene, camera);
  const resize = () => {
    renderer.setSize(host.clientWidth, host.clientHeight);
    camera.aspect = host.clientWidth / host.clientHeight;
    camera.updateProjectionMatrix();
    render();
  };
  const view = (name: string) => {
    const positions: Record<string, [number, number, number]> = {
      front: [0.22, 0.19, 0.29],
      back: [-0.17, 0.14, -0.31],
      top: [-0.012, 0.4, -0.0079],
      bottom: [0.18, -0.17, 0.24],
    };
    controls.target.set(-0.012, 0.04, -0.008);
    camera.position.set(...(positions[name] ?? positions.front));
    controls.update();
    render();
    document
      .querySelectorAll<HTMLButtonElement>("[data-view]")
      .forEach((b) =>
        b.setAttribute("aria-pressed", String(b.dataset.view === name)),
      );
  };
  window.__audioDockPreview = {
    ready: false,
    view,
    capture: () => {
      render();
      return renderer.domElement.toDataURL("image/png");
    },
    stats: null,
  };
  const bytes = Uint8Array.from(atob(window.__audioDockData), (c) =>
    c.charCodeAt(0),
  );
  const gltf = await new GLTFLoader().parseAsync(bytes.buffer, "");
  scene.add(gltf.scene);
  let meshes = 0,
    triangles = 0;
  gltf.scene.traverse((o) => {
    if (o instanceof THREE.Mesh) {
      meshes++;
      triangles +=
        (o.geometry.index?.count ?? o.geometry.attributes.position.count) / 3;
    }
  });
  window.__audioDockPreview.stats = {
    meshes,
    triangles,
    bounds: new THREE.Box3()
      .setFromObject(gltf.scene)
      .getSize(new THREE.Vector3())
      .toArray(),
  };
  controls.addEventListener("change", render);
  new ResizeObserver(resize).observe(host);
  document
    .querySelectorAll<HTMLButtonElement>("[data-view]")
    .forEach((b) => b.addEventListener("click", () => view(b.dataset.view!)));
  const cable = gltf.scene.getObjectByName("upstream-usb-c-cable");
  document
    .querySelector<HTMLInputElement>("#cable")!
    .addEventListener("change", (e) => {
      if (cable) cable.visible = (e.target as HTMLInputElement).checked;
      render();
    });
  document
    .querySelector<HTMLButtonElement>("#download")!
    .addEventListener("click", () => {
      const url = URL.createObjectURL(
        new Blob([bytes], { type: "model/gltf-binary" }),
      );
      const a = document.createElement("a");
      a.href = url;
      a.download = "microsoft-audio-dock.glb";
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  view("front");
  resize();
  // Decode all embedded PNGs before allowing poster capture.
  await renderer.compileAsync(scene, camera);
  render();
  window.__audioDockPreview.ready = true;
  status.textContent = "拖动旋转 · 滚轮缩放 · 右键平移";
}
start().catch((error) => {
  status.textContent = `加载失败：${error.message}`;
  window.__audioDockPreview = {
    ...window.__audioDockPreview,
    ready: false,
    error: String(error),
  };
});
