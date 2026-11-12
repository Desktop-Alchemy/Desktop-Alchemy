import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import modelUrl from "../../assets/devices/ms-audio-dock/audio-dock.glb?url";

// Display only. Input state comes from the existing device snapshot.
export class AudioDockScene {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(34, 1, 0.001, 2);
  private controls: OrbitControls;
  private model?: THREE.Group;
  private environment?: THREE.WebGLRenderTarget;
  private teams?: THREE.Object3D;
  private teamsMaterial?: THREE.MeshStandardMaterial;
  private observer: ResizeObserver;
  private intersection: IntersectionObserver;
  private disposed = false;
  private visible = true;
  private dirty = true;
  private frame = 0;
  private frames = 0;
  private pressed = false;
  private focused = false;

  constructor(
    private host: HTMLElement,
    private fail: () => void,
  ) {
    this.renderer = new THREE.WebGLRenderer({
      alpha: true,
      antialias: true,
      powerPreference: "low-power",
    });
    this.renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
    this.renderer.setClearColor(0, 0);
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.1;
    this.renderer.domElement.setAttribute("aria-hidden", "true");
    host.append(this.renderer.domElement);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enablePan = false;
    this.controls.enableDamping = false;
    this.controls.minDistance = 0.2;
    this.controls.maxDistance = 0.55;
    this.controls.rotateSpeed = 0.65;
    this.controls.zoomSpeed = 0.65;
    this.controls.addEventListener("change", this.invalidate);
    this.setView("top");
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x687086, 1.1));
    for (const [color, intensity, x, y, z] of [
      [0xffffff, 2.2, -0.2, 0.3, 0.3],
      [0xd9e6ff, 1.3, 0.2, 0.15, -0.2],
      [0xdde3f5, 0.6, 0, -0.2, 0.1],
    ]) {
      const light = new THREE.DirectionalLight(color, intensity);
      light.position.set(x, y, z);
      this.scene.add(light);
    }
    this.observer = new ResizeObserver(this.resize);
    this.observer.observe(host);
    this.intersection = new IntersectionObserver(([entry]) => {
      this.visible = entry.isIntersecting;
      if (this.visible) this.invalidate();
      else this.cancelFrame();
    });
    this.intersection.observe(host);
    document.addEventListener("visibilitychange", this.visibilityChanged);
    this.renderer.domElement.addEventListener(
      "webglcontextlost",
      this.contextLost,
    );
    this.resize();
  }

  async load() {
    const gltf = await new GLTFLoader().loadAsync(modelUrl);
    if (this.disposed) {
      this.release(gltf.scene);
      return;
    }
    this.model = gltf.scene;
    // Keep the cable in the independent GLB; omit it from the compact app view.
    const cable = this.model.getObjectByName("upstream-usb-c-cable");
    if (cable) cable.visible = false;
    this.teams = this.model.getObjectByName("button-teams");
    const cap = this.model.getObjectByName("teams-keycap");
    if (cap instanceof THREE.Mesh) {
      this.teamsMaterial = (cap.material as THREE.MeshStandardMaterial).clone();
      cap.material = this.teamsMaterial;
    }
    const pmrem = new THREE.PMREMGenerator(this.renderer),
      room = new RoomEnvironment();
    try {
      this.environment = pmrem.fromScene(room, 0.04);
      this.scene.environment = this.environment.texture;
      this.scene.environmentIntensity = 0.5;
    } finally {
      room.dispose();
      pmrem.dispose();
    }
    this.scene.add(this.model);
    this.updateTeams();
    this.invalidate();
  }

  setState(pressed: boolean, focused: boolean) {
    if (this.pressed === pressed && this.focused === focused) return;
    this.pressed = pressed;
    this.focused = focused;
    this.updateTeams();
    this.invalidate();
  }
  private updateTeams() {
    if (this.teams) this.teams.position.y = this.pressed ? -0.00012 : 0;
    this.teamsMaterial?.color.setHex(
      this.pressed ? 0x5966f3 : this.focused ? 0x30374b : 0x222326,
    );
  }
  setView(view: "front" | "top" | "back") {
    this.controls.target.set(0, 0.0405, 0);
    const position: { [key: string]: [number, number, number] } = {
      front: [0.14, 0.145, 0.21],
      top: [0, 0.29, 0.0001],
      back: [-0.12, 0.12, -0.23],
    };
    this.camera.position.set(...position[view]);
    this.controls.update();
    this.invalidate();
  }
  private resize = () => {
    if (this.disposed) return;
    const width = Math.max(1, this.host.clientWidth),
      height = Math.max(1, this.host.clientHeight);
    this.renderer.setSize(width, height);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.invalidate();
  };
  private invalidate = () => {
    this.dirty = true;
    if (this.disposed || !this.visible || document.hidden || this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      if (this.disposed || !this.visible || document.hidden) return;
      this.renderer.render(this.scene, this.camera);
      this.frames++;
      this.dirty = false;
    });
  };
  private cancelFrame() {
    cancelAnimationFrame(this.frame);
    this.frame = 0;
  }
  private visibilityChanged = () => {
    if (document.hidden) this.cancelFrame();
    else if (this.dirty) this.invalidate();
  };
  private contextLost = (event: Event) => {
    event.preventDefault();
    this.cancelFrame();
    this.fail();
  };
  stats() {
    return {
      frames: this.frames,
      calls: this.renderer.info.render.calls,
      pixelRatio: this.renderer.getPixelRatio(),
    };
  }
  capturePng(width = 960, height = 720) {
    const ratio = this.renderer.getPixelRatio();
    this.renderer.setPixelRatio(1);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.render(this.scene, this.camera);
    const data = this.renderer.domElement.toDataURL("image/png");
    this.renderer.setPixelRatio(ratio);
    this.resize();
    return data;
  }
  private release(object: THREE.Object3D) {
    const geometries = new Set<THREE.BufferGeometry>(),
      materials = new Set<THREE.Material>(),
      textures = new Set<THREE.Texture>();
    object.traverse((o) => {
      if (o instanceof THREE.Mesh) {
        geometries.add(o.geometry);
        for (const m of Array.isArray(o.material) ? o.material : [o.material])
          materials.add(m);
      }
    });
    for (const m of materials)
      for (const value of Object.values(m))
        if (value instanceof THREE.Texture) textures.add(value);
    for (const g of geometries) g.dispose();
    for (const m of materials) m.dispose();
    for (const t of textures) {
      if (t.image instanceof ImageBitmap) t.image.close();
      t.dispose();
    }
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.cancelFrame();
    this.observer.disconnect();
    this.intersection.disconnect();
    document.removeEventListener("visibilitychange", this.visibilityChanged);
    this.renderer.domElement.removeEventListener(
      "webglcontextlost",
      this.contextLost,
    );
    this.controls.removeEventListener("change", this.invalidate);
    this.controls.dispose();
    if (this.model) this.release(this.model);
    this.environment?.dispose();
    this.scene.clear();
    this.renderer.dispose();
    if (!this.renderer.getContext().isContextLost())
      this.renderer.forceContextLoss();
    this.renderer.domElement.remove();
    this.model = undefined;
  }
}
