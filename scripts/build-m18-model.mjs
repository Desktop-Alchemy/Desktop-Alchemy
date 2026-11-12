// Offline source for the M18 UI asset. Dimensions are metres in glTF.
// Photo estimates are declared below; this is a visual model, not manufacturing CAD.
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import * as THREE from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { GLTFExporter } from "three/addons/exporters/GLTFExporter.js";

const mm = (n) => n / 1000;
const assetDirectory = new URL("../src/assets/devices/m18/", import.meta.url);
const root = new THREE.Group();
root.name = "M18";
root.userData = {
  adapterId: "mirabox.m18",
  dimensionsMm: { width: 117, depth: 104, height: 21 },
  axes: "X width; Y up; +Z front; keys numbered left to right from rear row",
  modelRevision: 2,
};

const material = (name, color, roughness, metalness = 0) => {
  const result = new THREE.MeshStandardMaterial({
    color,
    roughness,
    metalness,
  });
  result.name = name;
  return result;
};
const shell = material("Graphite matte casing", 0x1d1f24, 0.68, 0.08);
const panel = material("Inset satin top", 0x25272c, 0.83, 0.02);
const underside = material("Lower casing", 0x15171b, 0.82);
const recess = material("Recess shadow", 0x08090c, 0.9);
const gloss = material("Gloss black keycaps", 0x101116, 0.17, 0.12);
const rubber = material("Rubber feet and physical keys", 0x181a1e, 0.9);
const screen = material("Screen placeholder", 0x243044, 0.35);
const metal = material("USB connector", 0x757982, 0.3, 0.8);
const led = material("LED diffuser", 0x7496ff, 0.3);
led.emissive.setHex(0x6385ff);
led.emissiveIntensity = 1.1;

function mesh(name, geometry, mat, parent = root) {
  const part = new THREE.Mesh(geometry, mat);
  part.name = name;
  parent.add(part);
  return part;
}

// Rounded rectangles run clockwise when seen from above. Shared side vertices
// give smooth corner normals; the top and bottom caps use separate vertices.
function outline(width, depth, radius, segments = 8) {
  const points = [];
  for (const [cx, cz, start] of [
    [width / 2 - radius, depth / 2 - radius, 0],
    [-width / 2 + radius, depth / 2 - radius, Math.PI / 2],
    [-width / 2 + radius, -depth / 2 + radius, Math.PI],
    [width / 2 - radius, -depth / 2 + radius, (3 * Math.PI) / 2],
  ]) {
    for (let i = 0; i <= segments; i++) {
      const a = start + (i / segments) * (Math.PI / 2);
      points.push([cx + Math.cos(a) * radius, cz + Math.sin(a) * radius]);
    }
  }
  return points;
}

function slab(levels, caps = true, omitRear = false) {
  const positions = [],
    indices = [];
  const contours = levels.map(([y, w, d, r]) =>
    outline(mm(w), mm(d), mm(r)).map(([x, z]) => [x, mm(y), z]),
  );
  const n = contours[0].length;
  for (const points of contours) positions.push(...points.flat());
  for (let layer = 0; layer < contours.length - 1; layer++) {
    for (let i = 0; i < n; i++) {
      const j = (i + 1) % n;
      if (
        omitRear &&
        contours[layer][i][2] < mm(-48) &&
        contours[layer][j][2] < mm(-48)
      )
        continue;
      const a = layer * n + i,
        b = layer * n + j;
      indices.push(a, a + n, b, b, a + n, b + n);
    }
  }
  if (caps) {
    for (const layer of [0, contours.length - 1]) {
      const center = positions.length / 3;
      positions.push(0, contours[layer][0][1], 0);
      const start = positions.length / 3;
      positions.push(...contours[layer].flat());
      for (let i = 0; i < n; i++) {
        const a = start + i,
          b = start + ((i + 1) % n);
        indices.push(...(layer === 0 ? [center, a, b] : [center, b, a]));
      }
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute(
    "position",
    new THREE.Float32BufferAttribute(positions, 3),
  );
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

mesh(
  "lower-shell",
  slab([
    [1.5, 113.5, 100.5, 5.4],
    [2.2, 116.4, 103.4, 6],
    [3, 117, 104, 6],
    [8.1, 117, 104, 6],
  ]),
  underside,
);
mesh(
  "perimeter-led",
  slab(
    [
      [8.1, 116.8, 103.8, 5.9],
      [9.45, 116.8, 103.8, 5.9],
    ],
    false,
    true,
  ),
  led,
);
mesh(
  "upper-shell",
  slab([
    [9.45, 117, 104, 6],
    [17.6, 117, 104, 6],
    [18.25, 116.8, 103.8, 5.9],
    [18.55, 116, 103, 5.7],
  ]),
  shell,
);
mesh(
  "top-rim-shadow",
  slab([
    [18.55, 114, 101, 5.3],
    [18.62, 114, 101, 5.3],
  ]),
  recess,
);
mesh(
  "top-panel",
  slab([
    [18.62, 113.5, 100.5, 5.1],
    [18.68, 113.5, 100.5, 5.1],
  ]),
  panel,
);

const wellGeometry = new RoundedBoxGeometry(
  mm(16.2),
  mm(0.5),
  mm(16.2),
  2,
  mm(0.8),
);
const capGeometry = new RoundedBoxGeometry(
  mm(15.5),
  mm(1.8),
  mm(15.5),
  3,
  mm(0.65),
);
const screenGeometry = new THREE.PlaneGeometry(mm(12.4), mm(12.4));
screenGeometry.rotateX(-Math.PI / 2);
for (let i = 0; i < 15; i++) {
  const number = i + 1;
  const group = new THREE.Group();
  group.name = `key-${String(number).padStart(2, "0")}`;
  group.userData = { key: number, kind: "display-key" };
  group.position.set(
    mm(((i % 5) - 2) * 18),
    0,
    mm(-31.5 + Math.floor(i / 5) * 18),
  );
  root.add(group);
  mesh(`well-${number}`, wellGeometry, recess, group).position.y = mm(18.85);
  mesh(`cap-${number}`, capGeometry, gloss, group).position.y = mm(20.05);
  const surface = mesh(
    `screen-${String(number).padStart(2, "0")}`,
    screenGeometry,
    screen,
    group,
  );
  surface.userData = { screenKey: number };
  surface.position.y = mm(21);
}

const physicalRing = new THREE.CylinderGeometry(mm(5.2), mm(5.2), mm(0.25), 40);
// The top is a shallow dish: the centre is 0.45 mm below the rounded rim.
const physicalCap = new THREE.LatheGeometry(
  [
    [0, -0.725],
    [4.55, -0.725],
    [4.9, -0.48],
    [4.95, 0.28],
    [4.8, 0.58],
    [4.55, 0.72],
    [4.1, 0.64],
    [3.2, 0.46],
    [2.1, 0.33],
    [0.9, 0.28],
    [0, 0.27],
  ].map(([radius, y]) => new THREE.Vector2(mm(radius), mm(y))),
  48,
);
for (let i = 0; i < 3; i++) {
  const group = new THREE.Group();
  group.name = `key-${16 + i}`;
  group.userData = { key: 16 + i, kind: "physical-key" };
  group.position.set(mm((i - 1) * 18), 0, mm(32.5));
  root.add(group);
  mesh(`well-${16 + i}`, physicalRing, recess, group).position.y = mm(18.82);
  mesh(`cap-${16 + i}`, physicalCap, rubber, group).position.y = mm(19.55);
}

const indicator = new THREE.CapsuleGeometry(mm(0.45), mm(11.8), 3, 8);
indicator.rotateZ(Math.PI / 2);
for (const [name, x] of [
  ["left", -36],
  ["right", 36],
]) {
  mesh(`top-led-${name}`, indicator, led).position.set(
    mm(x),
    mm(18.7),
    mm(32.5),
  );
}

const foot = new THREE.CylinderGeometry(mm(4.7), mm(4.9), mm(1.5), 32);
for (const x of [-48, 48])
  for (const z of [-42, 42]) {
    mesh(`rubber-foot-${x}-${z}`, foot, rubber).position.set(
      mm(x),
      mm(0.75),
      mm(z),
    );
  }

// Folded underside kickstand and recessed USB-C area, simplified from the photo.
mesh(
  "folded-stand-recess",
  slab([
    [1.38, 66, 43, 5],
    [1.5, 66, 43, 5],
  ]),
  recess,
).position.z = mm(-25);
mesh(
  "folded-kickstand",
  slab([
    [0.9, 62, 39, 4],
    [1.28, 62, 39, 4],
  ]),
  shell,
).position.z = mm(-25);
const hinge = new THREE.CylinderGeometry(mm(1.4), mm(1.4), mm(57), 24);
hinge.rotateZ(Math.PI / 2);
mesh("stand-hinge", hinge, shell).position.set(0, mm(1.6), mm(-7));
const usb = mesh(
  "usb-c-recess",
  new RoundedBoxGeometry(mm(10), mm(3.6), mm(0.4), 2, mm(0.3)),
  recess,
);
usb.position.set(0, mm(5), mm(-51.75));
mesh(
  "usb-c-inner",
  new THREE.BoxGeometry(mm(6), mm(0.6), mm(0.5)),
  metal,
).position.set(0, mm(5), mm(-51.7));

// Small model label; source photos have differing retail brands.
const strokes = {
  M: [
    [
      [0, 0],
      [0, 2.8],
      [1.1, 1.3],
      [2.2, 2.8],
      [2.2, 0],
    ],
  ],
  1: [
    [
      [0.25, 2.15],
      [1, 2.8],
      [1, 0],
    ],
    [
      [0.3, 0],
      [1.7, 0],
    ],
  ],
  8: [
    [
      [0.2, 0.1],
      [0.2, 1.35],
      [1.7, 1.35],
      [1.7, 0.1],
      [0.2, 0.1],
    ],
    [
      [0.2, 1.35],
      [0.2, 2.7],
      [1.7, 2.7],
      [1.7, 1.35],
    ],
  ],
};
const labelMaterial = material("M18 label", 0xb0b1ba, 0.65);
const label = new THREE.Group();
label.name = "model-label";
root.add(label);
for (const [index, character] of [..."M18"].entries()) {
  for (const path of strokes[character]) {
    const curve = new THREE.CatmullRomCurve3(
      path.map(
        ([x, y]) =>
          new THREE.Vector3(mm(x + index * 2.8 - 3.9), mm(18.78), mm(-43 - y)),
      ),
      false,
      "catmullrom",
      0,
    );
    mesh(
      "label-stroke",
      new THREE.TubeGeometry(curve, path.length * 3, mm(0.085), 4, false),
      labelMaterial,
      label,
    );
  }
}

// GLTFExporter uses FileReader for its final Blob even without image textures.
globalThis.FileReader = class {
  readAsArrayBuffer(blob) {
    blob.arrayBuffer().then((buffer) => {
      this.result = buffer;
      this.onloadend?.();
    });
  }
};
root.traverse((object) => {
  if (object.isMesh) object.geometry.normalizeNormals();
});
const binary = await new GLTFExporter().parseAsync(root, {
  binary: true,
  onlyVisible: true,
});
await mkdir(assetDirectory, { recursive: true });
await writeFile(new URL("m18.glb", assetDirectory), Buffer.from(binary));
const bounds = new THREE.Box3()
  .setFromObject(root)
  .getSize(new THREE.Vector3());
let triangles = 0,
  meshes = 0;
const uniqueGeometry = new Set(),
  uniqueMaterials = new Set();
root.traverse((object) => {
  if (!object.isMesh) return;
  meshes++;
  triangles +=
    (object.geometry.index?.count ??
      object.geometry.attributes.position.count) / 3;
  uniqueGeometry.add(object.geometry);
  uniqueMaterials.add(object.material);
});
const metadata = {
  revision: 2,
  dimensionsMm: root.userData.dimensionsMm,
  boundsMmXYZ: bounds.toArray().map((n) => Math.round(n * 100000) / 100),
  axes: root.userData.axes,
  meshes,
  triangles,
  uniqueGeometries: uniqueGeometry.size,
  materials: uniqueMaterials.size,
  bytes: binary.byteLength,
  estimatedPhotoDimensionsMm: {
    keyPitch: 18,
    displayKey: 15.5,
    screen: 12.4,
    physicalKeyDiameter: 9.9,
    physicalKeyDishDepth: 0.45,
    cornerRadius: 6,
  },
  notes: [
    "Overall size supplied by user; local feature dimensions estimated from supplied photos.",
    "Kickstand modelled folded; underside port is simplified; retail logos omitted.",
    "Screens are named screen-01..15 and key groups key-01..18; indices use public left-to-right order.",
  ],
};
await writeFile(
  new URL("model-info.json", assetDirectory),
  JSON.stringify(metadata, null, 2) + "\n",
);
console.log(fileURLToPath(new URL("m18.glb", assetDirectory)));
console.log(JSON.stringify(metadata));
