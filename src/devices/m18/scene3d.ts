import { t } from "../../i18n";
import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import modelUrl from "../../assets/devices/m18/m18.glb?url";
import type { Page, ModelAppearance } from "../../bindings";
import {
  animatedLed,
  ledCycleMs,
  ledPreviewFrame,
  type LedPreviewSettings,
} from "./led-preview";

// Static views render on demand; animated LEDs use a visible-only 20 FPS timer.
export class M18Scene {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(34, 1, 0.005, 2);
  private controls: OrbitControls;
  private model?: THREE.Group;
  private environment?: THREE.WebGLRenderTarget;
  private keyGroups = new Map<number, THREE.Object3D>();
  private keyMaterials = new Map<number, THREE.MeshStandardMaterial>();
  private resources = new Set<THREE.Material>();
  private geometries = new Set<THREE.BufferGeometry>();
  private atlas = document.createElement("canvas");
  private atlasTexture: THREE.CanvasTexture;
  private screenMaterial: THREE.MeshBasicMaterial;
  private observer: ResizeObserver;
  private intersection: IntersectionObserver;
  private visible = true;
  private dirty = true;
  private disposed = false;
  private frame = 0;
  private frames = 0;
  private atlasGeneration = 0;
  private lastRenderMs = 0;
  private pointer?: { x: number; y: number; button: number };
  private selectedKey = 0;
  private pressedKeys: number[] = [];
  private raycaster = new THREE.Raycaster();
  private labelCanvas = document.createElement("canvas");
  private labelTexture?: THREE.CanvasTexture;
  private labelMesh?: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>;
  private labelGeneration = 0;
  private ledSettings?: LedPreviewSettings;
  private ledStarted = 0;
  private ledTimer?: ReturnType<typeof setTimeout>;
  private ledPixels = new Uint8Array(24 * 4);
  private ledTexture?: THREE.DataTexture;
  private ledMaterial?: THREE.MeshStandardMaterial;
  private ledParts: THREE.Mesh[] = [];
  private ledGlows: THREE.Mesh[] = [];
  private ledGlowTexture?: THREE.DataTexture;
  private originalLedMaterial?: THREE.Material | THREE.Material[];
  private reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  constructor(
    private host: HTMLElement,
    private select: (key: number) => void,
    private fail: (message: string) => void,
  ) {
    this.renderer = new THREE.WebGLRenderer({
      alpha: true,
      antialias: true,
      powerPreference: "low-power",
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1;
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.domElement.setAttribute("aria-hidden", "true");
    host.append(this.renderer.domElement);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = false;
    this.controls.enablePan = false;
    this.controls.minDistance = 0.13;
    this.controls.maxDistance = 0.48;
    this.controls.minPolarAngle = 0.08;
    this.controls.maxPolarAngle = Math.PI - 0.08;
    this.controls.zoomSpeed = 0.65;
    this.controls.rotateSpeed = 0.65;
    this.resetView();
    this.controls.addEventListener("change", this.invalidate);
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x71798c, 0.9));
    const key = new THREE.DirectionalLight(0xffffff, 2.2);
    key.position.set(-0.15, 0.25, 0.12);
    this.scene.add(key);
    const rim = new THREE.DirectionalLight(0xd7e4ff, 1.2);
    rim.position.set(0.15, 0.1, -0.12);
    this.scene.add(rim);
    const underside = new THREE.DirectionalLight(0xdde3f5, 1.2);
    underside.position.set(-0.1, -0.2, 0.04);
    this.scene.add(underside);
    this.atlas.width = 640;
    this.atlas.height = 384;
    this.atlasTexture = new THREE.CanvasTexture(this.atlas);
    this.atlasTexture.colorSpace = THREE.SRGBColorSpace;
    this.atlasTexture.anisotropy = Math.min(
      4,
      this.renderer.capabilities.getMaxAnisotropy(),
    );
    this.screenMaterial = new THREE.MeshBasicMaterial({
      map: this.atlasTexture,
      toneMapped: false,
    });
    this.resources.add(this.screenMaterial);
    this.observer = new ResizeObserver(this.resize);
    this.observer.observe(host);
    this.intersection = new IntersectionObserver(([entry]) => {
      this.visible = entry.isIntersecting;
      if (this.visible) this.invalidate();
      else this.cancelFrame();
      this.syncLedAnimation();
    });
    this.intersection.observe(host);
    document.addEventListener("visibilitychange", this.visibilityChanged);
    this.reducedMotion.addEventListener("change", this.motionChanged);
    this.renderer.domElement.addEventListener("pointerdown", this.pointerDown);
    this.renderer.domElement.addEventListener("pointerup", this.pointerUp);
    this.renderer.domElement.addEventListener(
      "webglcontextlost",
      this.contextLost,
    );
    this.resize();
  }

  async load() {
    const gltf = await new GLTFLoader().loadAsync(modelUrl);
    if (this.disposed) {
      this.releaseObject(gltf.scene);
      return;
    }
    this.model = gltf.scene;
    const pmrem = new THREE.PMREMGenerator(this.renderer);
    const room = new RoomEnvironment();
    this.environment = pmrem.fromScene(room, 0.04);
    this.scene.environment = this.environment.texture;
    this.scene.environmentIntensity = 0.45;
    room.dispose();
    pmrem.dispose();
    this.model.traverse((object) => {
      const key = Number(object.userData.key);
      if (key > 0) this.keyGroups.set(key, object);
      if (!(object instanceof THREE.Mesh)) return;
      this.geometries.add(object.geometry);
      const materials = Array.isArray(object.material)
        ? object.material
        : [object.material];
      materials.forEach((material) => this.resources.add(material));
      const screen = Number(object.userData.screenKey);
      if (screen > 0) {
        object.geometry = object.geometry.clone();
        const uv = object.geometry.getAttribute("uv");
        const column = (screen - 1) % 5;
        const row = Math.floor((screen - 1) / 5);
        for (let i = 0; i < uv.count; i++) {
          uv.setXY(i, (column + uv.getX(i)) / 5, (2 - row + uv.getY(i)) / 3);
        }
        object.material = this.screenMaterial;
      } else if (object.name.startsWith("cap-")) {
        const material = (
          object.material as THREE.MeshStandardMaterial
        ).clone();
        object.material = material;
        this.resources.add(material);
        this.keyMaterials.set(Number(object.name.slice(4)), material);
      }
    });
    this.scene.add(this.model);
    this.prepareLeds();
    this.invalidate();
  }

  private prepareLeds() {
    if (!this.model) return;
    this.ledTexture = new THREE.DataTexture(this.ledPixels, 24, 1);
    this.ledTexture.colorSpace = THREE.SRGBColorSpace;
    this.ledTexture.minFilter = THREE.LinearFilter;
    this.ledTexture.magFilter = THREE.LinearFilter;
    this.ledTexture.generateMipmaps = false;
    this.ledMaterial = new THREE.MeshStandardMaterial({
      color: 0x08090c,
      roughness: 0.3,
      emissive: 0xffffff,
      emissiveMap: this.ledTexture,
      emissiveIntensity: 6,
    });
    this.resources.add(this.ledMaterial);
    const glowPixels = new Uint8Array(32 * 4);
    for (let i = 0; i < 32; i++) {
      const alpha = Math.round(255 * Math.sin((Math.PI * i) / 31) ** 3);
      glowPixels.set([alpha, alpha, alpha, 255], i * 4);
    }
    this.ledGlowTexture = new THREE.DataTexture(glowPixels, 1, 32);
    this.ledGlowTexture.minFilter = THREE.LinearFilter;
    this.ledGlowTexture.magFilter = THREE.LinearFilter;
    this.ledGlowTexture.needsUpdate = true;
    const glowMaterial = new THREE.MeshBasicMaterial({
      map: this.ledTexture,
      alphaMap: this.ledGlowTexture,
      transparent: true,
      opacity: 0.6,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      toneMapped: false,
    });
    this.resources.add(glowMaterial);
    for (const name of ["perimeter-led", "top-led-left", "top-led-right"]) {
      const part = this.model.getObjectByName(name);
      if (!(part instanceof THREE.Mesh)) continue;
      this.originalLedMaterial ??= part.material;
      part.geometry = part.geometry.clone();
      const positions = part.geometry.getAttribute("position");
      const uv = new Float32Array(positions.count * 2);
      // Approximate the 24 LED positions along the photographed diffuser.
      // The firmware's physical LED-to-position mapping has not been calibrated.
      for (let i = 0; i < positions.count; i++) {
        const x =
          name === "perimeter-led" ? positions.getX(i) : part.position.x;
        const z =
          name === "perimeter-led" ? positions.getZ(i) : part.position.z;
        uv[i * 2] =
          (Math.atan2(x / 0.0584, z / 0.0519) + Math.PI) / (2 * Math.PI);
        uv[i * 2 + 1] = 0.5;
      }
      part.geometry.setAttribute("uv", new THREE.BufferAttribute(uv, 2));
      this.ledParts.push(part);
      let glowGeometry: THREE.BufferGeometry;
      if (name === "perimeter-led") {
        glowGeometry = part.geometry.clone();
        glowGeometry.computeBoundingBox();
        const { min, max } = glowGeometry.boundingBox!;
        const center = (min.y + max.y) / 2;
        const glowPositions = glowGeometry.getAttribute("position");
        const normals = glowGeometry.getAttribute("normal");
        const glowUv = glowGeometry.getAttribute("uv");
        for (let i = 0; i < glowPositions.count; i++) {
          const y = glowPositions.getY(i);
          glowUv.setY(i, (y - min.y) / (max.y - min.y));
          glowPositions.setXYZ(
            i,
            glowPositions.getX(i) + normals.getX(i) * 0.00016,
            center + (y - center) * 4.5,
            glowPositions.getZ(i) + normals.getZ(i) * 0.00016,
          );
        }
      } else {
        glowGeometry = new THREE.PlaneGeometry(0.0145, 0.0036);
        glowGeometry.rotateX(-Math.PI / 2);
        glowGeometry.translate(0, 0.00005, 0);
        const glowUv = glowGeometry.getAttribute("uv");
        for (let i = 0; i < glowUv.count; i++) glowUv.setX(i, uv[0]);
      }
      const glow = new THREE.Mesh(glowGeometry, glowMaterial);
      glow.name = `${name}-glow`;
      glow.visible = false;
      part.add(glow);
      this.ledGlows.push(glow);
    }
    this.updateLeds();
  }

  setLedSettings(settings: LedPreviewSettings) {
    if (this.disposed) return;
    if (this.ledSettings?.led_mode !== settings.led_mode)
      this.ledStarted = performance.now();
    this.ledSettings = {
      ...settings,
      led_colors: settings.led_colors.map((rgb) => [...rgb]),
    };
    this.updateLeds();
    this.syncLedAnimation();
    this.invalidate();
  }

  private updateLeds() {
    if (!this.ledSettings || !this.ledTexture || !this.ledMaterial) return;
    const settings = this.ledSettings;
    const elapsed = this.reducedMotion.matches
      ? ledCycleMs(settings.led_speed) / 4
      : performance.now() - this.ledStarted;
    const colors = ledPreviewFrame(settings, elapsed);
    for (const part of this.ledParts)
      part.material = colors ? this.ledMaterial : this.originalLedMaterial!;
    for (const glow of this.ledGlows)
      glow.visible = !!colors && settings.led_brightness > 0;
    if (!colors) return;
    colors.forEach((rgb, index) =>
      this.ledPixels.set([...rgb, 255], index * 4),
    );
    this.ledTexture.needsUpdate = true;
  }

  private syncLedAnimation() {
    clearTimeout(this.ledTimer);
    this.ledTimer = undefined;
    if (
      this.disposed ||
      !this.visible ||
      document.hidden ||
      this.reducedMotion.matches ||
      !this.ledSettings ||
      !animatedLed(this.ledSettings) ||
      this.ledSettings.led_brightness === 0
    )
      return;
    this.ledTimer = setTimeout(() => {
      this.updateLeds();
      this.invalidate();
      this.syncLedAnimation();
    }, 50);
  }

  private motionChanged = () => {
    this.updateLeds();
    this.syncLedAnimation();
    this.invalidate();
  };

  setPage(page: Page, images: Record<number, string>) {
    if (this.disposed) return;
    const generation = ++this.atlasGeneration;
    const context = this.atlas.getContext("2d")!;
    for (let n = 1; n <= 15; n++) {
      const visual = page.keys.find((key) => key.key === n);
      const x = ((n - 1) % 5) * 128,
        y = Math.floor((n - 1) / 5) * 128;
      context.fillStyle = visual?.background || "#252936";
      context.fillRect(x, y, 128, 128);
      context.save();
      context.beginPath();
      context.rect(x + 3, y + 3, 122, 122);
      context.clip();
      // Keep the background while the shared compositor prepares the key.
      context.restore();
      if (images[n]) {
        const image = new Image();
        image.onload = () => {
          if (this.disposed || generation !== this.atlasGeneration) return;
          context.clearRect(x, y, 128, 128);
          context.drawImage(image, x, y, 128, 128);
          this.atlasTexture.needsUpdate = true;
          this.invalidate();
        };
        image.src = images[n];
      }
    }
    this.atlasTexture.needsUpdate = true;
    this.invalidate();
  }

  setSelection(selected: number, pressed: number[]) {
    if (this.disposed) return;
    this.selectedKey = selected;
    this.pressedKeys = [...pressed];
    for (const [key, group] of this.keyGroups) {
      group.position.y = pressed.includes(key) ? -0.00055 : 0;
      const material = this.keyMaterials.get(key);
      if (!material) continue;
      material.emissive.setHex(
        pressed.includes(key) ? 0x278f72 : key === selected ? 0x5966f3 : 0,
      );
      material.emissiveIntensity =
        key === selected || pressed.includes(key) ? 0.6 : 0;
    }
    this.invalidate();
  }

  setAppearance(appearance: ModelAppearance, imageSource: string) {
    if (this.disposed || !this.model) return;
    const generation = ++this.labelGeneration;
    const original = this.model.getObjectByName("model-label");
    if (
      appearance.type === "default" ||
      (appearance.type === "image" && !imageSource)
    ) {
      if (original) original.visible = true;
      if (this.labelMesh) this.labelMesh.visible = false;
      this.invalidate();
      return;
    }
    if (!this.labelMesh) {
      this.labelCanvas.width = 1024;
      this.labelCanvas.height = 192;
      this.labelTexture = new THREE.CanvasTexture(this.labelCanvas);
      this.labelTexture.colorSpace = THREE.SRGBColorSpace;
      this.labelTexture.anisotropy = Math.min(
        4,
        this.renderer.capabilities.getMaxAnisotropy(),
      );
      const material = new THREE.MeshBasicMaterial({
        map: this.labelTexture,
        transparent: true,
        depthWrite: false,
        toneMapped: false,
      });
      this.labelMesh = new THREE.Mesh(
        new THREE.PlaneGeometry(0.028, 0.005),
        material,
      );
      this.labelMesh.name = "custom-model-label";
      this.labelMesh.rotation.x = -Math.PI / 2;
      this.labelMesh.position.set(0, 0.01889, -0.0444);
      this.model.add(this.labelMesh);
      this.resources.add(material);
    }
    const context = this.labelCanvas.getContext("2d")!;
    const commit = () => {
      if (this.disposed || generation !== this.labelGeneration) return;
      if (original) original.visible = false;
      this.labelMesh!.visible = true;
      this.labelTexture!.needsUpdate = true;
      this.invalidate();
    };
    if (appearance.type === "text") {
      context.clearRect(0, 0, 1024, 192);
      context.fillStyle = "#c2c5cf";
      context.font = '600 138px "Noto Sans SC Variable", sans-serif';
      context.textAlign = "center";
      context.textBaseline = "middle";
      context.fillText(appearance.text, 512, 100, 980);
      commit();
    } else {
      const image = new Image();
      image.onload = () => {
        if (this.disposed || generation !== this.labelGeneration) return;
        context.clearRect(0, 0, 1024, 192);
        const scale = Math.min(1000 / image.width, 178 / image.height);
        const width = image.width * scale,
          height = image.height * scale;
        context.drawImage(
          image,
          (1024 - width) / 2,
          (192 - height) / 2,
          width,
          height,
        );
        commit();
      };
      image.src = imageSource;
    }
  }

  resetView = () => {
    if (this.disposed) return;
    this.setView("top");
  };

  setView(view: "top" | "bottom" | "perspective") {
    if (this.disposed) return;
    if (view === "perspective") this.camera.position.set(0.11, 0.145, 0.17);
    else if (view === "top") this.camera.position.set(0, 0.3, 0.025);
    else this.camera.position.set(0.035, -0.23, 0.12);
    this.controls.target.set(0, 0.008, 0);
    this.controls.update();
    this.invalidate();
  }

  private resize = () => {
    if (this.disposed) return;
    const { width, height } = this.host.getBoundingClientRect();
    if (width < 1 || height < 1) return;
    const ratio = Math.min(window.devicePixelRatio, 1.5);
    if (this.renderer.getPixelRatio() !== ratio)
      this.renderer.setPixelRatio(ratio);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height, false);
    this.invalidate();
  };

  private visibilityChanged = () => {
    if (document.hidden) this.cancelFrame();
    else this.invalidate();
    this.syncLedAnimation();
  };

  private cancelFrame() {
    cancelAnimationFrame(this.frame);
    this.frame = 0;
  }

  private invalidate = () => {
    this.dirty = true;
    if (this.disposed || this.frame || !this.visible || document.hidden) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      if (this.disposed || !this.visible || document.hidden || !this.dirty)
        return;
      this.render();
    });
  };

  private render() {
    const start = performance.now();
    this.renderer.render(this.scene, this.camera);
    this.lastRenderMs = performance.now() - start;
    this.frames++;
    this.dirty = false;
  }

  private pointerDown = (event: PointerEvent) => {
    this.pointer = { x: event.clientX, y: event.clientY, button: event.button };
  };

  private pointerUp = (event: PointerEvent) => {
    const down = this.pointer;
    this.pointer = undefined;
    if (
      !down ||
      down.button !== 0 ||
      Math.hypot(event.clientX - down.x, event.clientY - down.y) > 5 ||
      !this.model
    )
      return;
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.raycaster.setFromCamera(
      new THREE.Vector2(
        ((event.clientX - rect.left) / rect.width) * 2 - 1,
        -((event.clientY - rect.top) / rect.height) * 2 + 1,
      ),
      this.camera,
    );
    const hit = this.raycaster.intersectObject(this.model, true)[0];
    let object: THREE.Object3D | null = hit?.object ?? null;
    while (object) {
      if (Number(object.userData.key) > 0) {
        this.select(Number(object.userData.key));
        return;
      }
      object = object.parent;
    }
  };

  private contextLost = (event: Event) => {
    event.preventDefault();
    this.cancelFrame();
    this.fail(t("ui.3dPreviewPausedRetryOrSwitch"));
  };

  // Internal authoring/inspection surface; not shown as diagnostics in daily UI.
  stats() {
    return {
      frames: this.frames,
      lastRenderCpuMs: this.lastRenderMs,
      calls: this.renderer.info.render.calls,
      triangles: this.renderer.info.render.triangles,
      geometries: this.renderer.info.memory.geometries,
      textures: this.renderer.info.memory.textures,
      pixelRatio: this.renderer.getPixelRatio(),
      drawingBuffer: this.renderer
        .getDrawingBufferSize(new THREE.Vector2())
        .toArray(),
    };
  }

  capturePng(width = 1200, height = 900) {
    const ratio = this.renderer.getPixelRatio();
    const selected = this.selectedKey,
      pressed = this.pressedKeys;
    this.renderer.setPixelRatio(1);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.setSelection(0, []);
    this.render();
    const result = this.renderer.domElement.toDataURL("image/png");
    this.renderer.setPixelRatio(ratio);
    this.resize();
    this.setSelection(selected, pressed);
    return result;
  }

  private releaseObject(root: THREE.Object3D) {
    root.traverse((object) => {
      if (!(object instanceof THREE.Mesh)) return;
      this.geometries.add(object.geometry);
      const materials = Array.isArray(object.material)
        ? object.material
        : [object.material];
      materials.forEach((material) => this.resources.add(material));
    });
    this.geometries.forEach((geometry) => geometry.dispose());
    this.geometries.clear();
    this.resources.forEach((material) => material.dispose());
    this.resources.clear();
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.atlasGeneration++;
    this.labelGeneration++;
    this.cancelFrame();
    clearTimeout(this.ledTimer);
    this.observer.disconnect();
    this.intersection.disconnect();
    document.removeEventListener("visibilitychange", this.visibilityChanged);
    this.reducedMotion.removeEventListener("change", this.motionChanged);
    this.renderer.domElement.removeEventListener(
      "pointerdown",
      this.pointerDown,
    );
    this.renderer.domElement.removeEventListener("pointerup", this.pointerUp);
    this.renderer.domElement.removeEventListener(
      "webglcontextlost",
      this.contextLost,
    );
    this.controls.dispose();
    if (this.model) this.releaseObject(this.model);
    this.atlasTexture.dispose();
    this.labelTexture?.dispose();
    this.ledTexture?.dispose();
    this.ledGlowTexture?.dispose();
    this.screenMaterial.dispose();
    this.environment?.dispose();
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.renderer.domElement.remove();
    this.keyGroups.clear();
    this.keyMaterials.clear();
    this.ledParts = [];
    this.ledGlows = [];
    this.scene.clear();
    this.model = undefined;
  }
}
