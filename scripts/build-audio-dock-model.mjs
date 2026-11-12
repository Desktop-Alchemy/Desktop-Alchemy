// Independent display asset; dimensions in metres, local features estimated from photos.
// Run: node scripts/build-audio-dock-model.mjs
import { mkdir, writeFile, copyFile, readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { iconGeometry } from "./audio-dock-icon-geometry.mjs";
import { deflateSync } from "node:zlib";
import * as THREE from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { GLTFExporter } from "three/addons/exporters/GLTFExporter.js";

const out = new URL("../output/audio-dock-model/", import.meta.url);
const mm = (n) => n / 1000;
const root = new THREE.Group();
root.name = "Microsoft-Audio-Dock";
const sources = [
  "https://news.microsoft.com/wp-content/uploads/prod/sites/646/2022/10/Microsoft-Audio-Dock-Fact-Sheet.pdf",
  "https://support.microsoft.com/en-gb/surface/accessories/certified/set-up-and-use-microsoft-audio-dock",
];
root.userData = {
  revision: 2,
  dimensionsMm: { width: 167.6, depth: 80.2, height: 81 },
  axes: "Metres; +Y up; +Z front; origin at centre of underside",
  purpose: "Photo-reconstructed display model; not manufacturing CAD",
  sources,
};
const mat = (name, color, roughness = 0.7, metalness = 0) => {
  const m = new THREE.MeshStandardMaterial({ color, roughness, metalness });
  m.name = name;
  return m;
};
const fabric = mat("Woven charcoal fabric", 0xffffff, 0.98);
const shell = mat("Matte graphite polymer", 0x222326, 0.77);
const panel = mat("Satin control panel", 0x292a2c, 0.62);
const rubber = mat("Rubber base and cable", 0x191a1b, 0.95);
const dark = mat("Port interior", 0x08090a, 0.92);
const print = mat("Silver grey SVG legends", 0x9ba7aa, 0.62, 0.08);
const metal = mat("Connector metal", 0x939699, 0.33, 0.82);
const contacts = mat("Connector contacts", 0xb69a60, 0.4, 0.7);
const parts = {};
function group(name, parent = root) {
  const g = new THREE.Group();
  g.name = name;
  parent.add(g);
  return g;
}
function mesh(name, geo, material, parent = root) {
  const m = new THREE.Mesh(geo, material);
  m.name = name;
  parent.add(m);
  return m;
}
function box(name, w, h, d, r, material, x, y, z, parent = root) {
  const m = mesh(
    name,
    new RoundedBoxGeometry(mm(w), mm(h), mm(d), 3, mm(r)),
    material,
    parent,
  );
  m.position.set(mm(x), mm(y), mm(z));
  return m;
}
// Clockwise in XZ viewed from +Y; straight edges are subdivided for even UVs.
function outline(w, d, r) {
  const p = [];
  const centres = [
    [w / 2 - r, d / 2 - r, 0],
    [-w / 2 + r, d / 2 - r, Math.PI / 2],
    [-w / 2 + r, -d / 2 + r, Math.PI],
    [w / 2 - r, -d / 2 + r, Math.PI * 1.5],
  ];
  for (let j = 0; j < 4; j++) {
    const [x, z, a] = centres[j];
    for (let i = 0; i <= 20; i++) {
      const t = a + ((i / 20) * Math.PI) / 2;
      p.push([x + r * Math.cos(t), z + r * Math.sin(t)]);
    }
    const next = centres[(j + 1) % 4];
    const start = p.at(-1),
      end = [
        next[0] + r * Math.cos(a + Math.PI / 2),
        next[1] + r * Math.sin(a + Math.PI / 2),
      ];
    for (let i = 1; i < 24; i++)
      p.push([
        THREE.MathUtils.lerp(start[0], end[0], i / 24),
        THREE.MathUtils.lerp(start[1], end[1], i / 24),
      ]);
  }
  return p.filter(
    (v, i) =>
      Math.hypot(
        v[0] - p[(i + p.length - 1) % p.length][0],
        v[1] - p[(i + p.length - 1) % p.length][1],
      ) > 1e-7,
  );
}
function slab(levels, { caps = true, portOpening = false } = {}) {
  const positions = [],
    uvs = [],
    indices = [];
  const contours = levels.map(([y, w, d, r]) =>
    outline(w, d, r).map(([x, z]) => [x, y, z]),
  );
  const n = contours[0].length;
  for (let l = 0; l < contours.length; l++) {
    const ring = contours[l];
    let u = 0;
    // Duplicate first point to give the wrap seam independent UVs.
    for (let i = 0; i <= n; i++) {
      if (i)
        u += Math.hypot(
          ring[i % n][0] - ring[i - 1][0],
          ring[i % n][2] - ring[i - 1][2],
        );
      positions.push(...ring[i % n].map(mm));
      uvs.push(u / 6.4, ring[i % n][1] / 6.4);
    }
  }
  const stride = n + 1;
  for (let l = 0; l < levels.length - 1; l++)
    for (let i = 0; i < n; i++) {
      const p = contours[l][i],
        q = contours[l][(i + 1) % n];
      if (
        portOpening &&
        levels[l][0] >= 12 &&
        levels[l + 1][0] <= 27 &&
        p[2] < -40 &&
        q[2] < -40
      )
        continue;
      const a = l * stride + i,
        b = a + 1;
      indices.push(a, a + stride, b, b, a + stride, b + stride);
    }
  if (caps)
    for (const l of [0, levels.length - 1]) {
      const c = positions.length / 3;
      positions.push(0, mm(levels[l][0]), 0);
      uvs.push(0, 0);
      const start = positions.length / 3;
      for (const [x, y, z] of contours[l]) {
        positions.push(mm(x), mm(y), mm(z));
        uvs.push(x / 6.4, z / 6.4);
      }
      for (let i = 0; i < n; i++) {
        const a = start + i,
          b = start + ((i + 1) % n);
        indices.push(...(l === 0 ? [c, a, b] : [c, b, a]));
      }
    }
  const g = new THREE.BufferGeometry();
  g.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  g.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  g.setIndex(indices);
  g.computeVertexNormals();
  // Smooth the duplicated side seam.
  const normals = g.attributes.normal;
  for (let l = 0; l < levels.length; l++) {
    const a = l * stride,
      b = a + n;
    const v = new THREE.Vector3()
      .fromBufferAttribute(normals, a)
      .add(new THREE.Vector3().fromBufferAttribute(normals, b))
      .normalize();
    normals.setXYZ(a, v.x, v.y, v.z);
    normals.setXYZ(b, v.x, v.y, v.z);
  }
  return g;
}
mesh(
  "fabric-enclosure",
  slab(
    [
      [7.5, 154, 66.6, 33.3],
      [8.5, 158, 70.6, 35.3],
      [10, 162, 74.6, 37.3],
      [12, 165.6, 78.2, 39.1],
      [16, 167.6, 80.2, 40.1],
      [27, 167.6, 80.2, 40.1],
      [63, 167.6, 80.2, 40.1],
      [68, 167.1, 79.7, 39.85],
      [72, 165.8, 78.4, 39.2],
      [75, 163.4, 76, 38],
      [77.5, 159.4, 72, 36],
      [79, 154, 66.6, 33.3],
      [80.15, 146, 58.6, 29.3],
    ],
    { portOpening: true },
  ),
  fabric,
);
mesh(
  "cable-storage-waist",
  slab([
    [2, 148, 60.6, 30.3],
    [7.5, 148, 60.6, 30.3],
  ]),
  dark,
);
mesh(
  "lower-rubber-rim",
  slab([
    [0.6, 148, 60.6, 30.3],
    [1.2, 154, 66.6, 33.3],
    [2.5, 157, 69.6, 34.8],
    [3.2, 156, 68.6, 34.3],
  ]),
  rubber,
);
mesh(
  "underside-foot",
  slab([
    [0, 137, 49.6, 24.8],
    [0.8, 139, 51.6, 25.8],
  ]),
  rubber,
);
mesh(
  "top-panel-seam",
  slab([
    [80.12, 127.5, 39.5, 19.75],
    [80.35, 127.5, 39.5, 19.75],
  ]),
  dark,
);
mesh(
  "top-control-panel",
  slab([
    [80.3, 126.8, 38.8, 19.4],
    [80.55, 126.2, 38.2, 19.1],
  ]),
  panel,
);
function circle(name, r, h, material, x, y, z, parent = root) {
  const m = mesh(
    name,
    new THREE.CylinderGeometry(mm(r), mm(r), mm(h), 64),
    material,
    parent,
  );
  m.position.set(mm(x), mm(y), mm(z));
  return m;
}
function line(name, points, r = 0.26, material = print, parent = root) {
  const curve = new THREE.CatmullRomCurve3(
    points.map((p) => new THREE.Vector3(...p.map(mm))),
    false,
    "centripetal",
  );
  return mesh(
    name,
    new THREE.TubeGeometry(
      curve,
      Math.max(2, points.length * 5),
      mm(r),
      6,
      false,
    ),
    material,
    parent,
  );
}
for (const [id, x] of [
  ["teams", -44],
  ["media", -18],
  ["volume-down", 0],
  ["volume-up", 18],
  ["mute", 44],
]) {
  const g = group(`button-${id}`);
  g.userData = { control: id };
  parts[id] = g;
  if (id === "teams" || id === "mute") {
    circle(`${id}-shadow-ring`, 10.9, 0.15, dark, x, 80.61, 0, g);
    circle(`${id}-keycap`, 10.55, 0.35, shell, x, 80.785, 0, g);
  }
}
// Use downloaded SVG contours rather than approximating printed symbols with tubes.
const iconDirectory = new URL("./assets/audio-dock-icons/", import.meta.url);
const iconSources = JSON.parse(
  await readFile(new URL("SOURCES.json", iconDirectory), "utf8"),
);
for (const [file, control, x, y, size] of [
  ["teams.svg", "teams", -44, 81, { widthMm: 7.8 }],
  ["play-pause.svg", "media", -18, 80.6, { widthMm: 7.8 }],
  ["minus.svg", "volume-down", 0, 80.6, { widthMm: 6.2 }],
  ["plus.svg", "volume-up", 18, 80.6, { widthMm: 6.2 }],
  ["microphone.svg", "mute", 44, 81, { heightMm: 7.2 }],
]) {
  const source = iconSources.icons.find((icon) => icon.file === file);
  const svg = await readFile(new URL(file, iconDirectory));
  if (createHash("sha256").update(svg).digest("hex") !== source.sha256)
    throw new Error(`Icon source changed: ${file}`);
  const legend = mesh(
    `legend-${control}`,
    iconGeometry(svg.toString("utf8"), size),
    print,
    parts[control],
  );
  legend.position.set(mm(x), mm(y), 0);
  legend.userData = { source: source.source, license: source.license };
}
root.userData.iconSources = iconSources.icons;
// Two subtle acoustic pickup openings near the front shoulder.
for (const x of [-61, 61]) {
  const m = circle(
    `microphone-opening-${x < 0 ? "left" : "right"}`,
    0.7,
    0.1,
    dark,
    x,
    80.12,
    24,
  );
  m.quaternion.setFromUnitVectors(
    new THREE.Vector3(0, 1, 0),
    new THREE.Vector3(Math.sign(x) * 0.16, 0.96, 0.23).normalize(),
  );
}

// Rear panel: a shaped plane with real holes and shallow interior wells.
const io = group("rear-io");
io.rotation.y = Math.PI;
io.position.set(0, mm(19.5), mm(-40.13));
const rounded = (w, h, r) => {
  const s = new THREE.Shape(),
    x = -w / 2,
    y = -h / 2;
  s.moveTo(x + r, y);
  s.lineTo(x + w - r, y);
  s.quadraticCurveTo(x + w, y, x + w, y + r);
  s.lineTo(x + w, y + h - r);
  s.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  s.lineTo(x + r, y + h);
  s.quadraticCurveTo(x, y + h, x, y + h - r);
  s.lineTo(x, y + r);
  s.quadraticCurveTo(x, y, x + r, y);
  return s;
};
const portDefs = [
  { id: "power", x: -39, w: 5.8, h: 5.8, r: 2.9 },
  { id: "hdmi", x: -22, w: 14.5, h: 5.3, r: 0.5 },
  { id: "usb-c-display", x: -3, w: 8.7, h: 3.3, r: 1.65 },
  { id: "usb-c-data", x: 13, w: 8.7, h: 3.3, r: 1.65 },
  { id: "usb-a", x: 33, w: 13, h: 5.5, r: 0.6 },
];
const ioShape = rounded(mm(100), mm(15.5), mm(7.75));
for (const p of portDefs) {
  // The rear view reverses X relative to the front view.
  const hole = rounded(mm(p.w), mm(p.h), mm(p.r));
  ioShape.holes.push(
    new THREE.Path(
      hole.getPoints(16).map((v) => new THREE.Vector2(v.x + mm(p.x), v.y)),
    ),
  );
}
mesh("rear-port-panel", new THREE.ShapeGeometry(ioShape, 16), shell, io);
for (const p of portDefs) {
  const g = group(`port-${p.id}`, io);
  g.position.x = mm(p.x);
  const profile = rounded(mm(p.w), mm(p.h), mm(p.r));
  const back = mesh(
    `${p.id}-well`,
    new THREE.ShapeGeometry(profile, 16),
    dark,
    g,
  );
  back.position.z = mm(-3);
  // Extruded annular collar, the black cavity remains open.
  const rim = rounded(mm(p.w + 0.65), mm(p.h + 0.65), mm(p.r + 0.325));
  rim.holes.push(profile);
  const collar = mesh(
    `${p.id}-metal-rim`,
    new THREE.ExtrudeGeometry(rim, {
      depth: mm(1.6),
      bevelEnabled: false,
      curveSegments: 16,
    }),
    metal,
    g,
  );
  collar.position.z = mm(-1.55);
  if (p.id === "power") {
    const pin = mesh(
      "power-centre-pin",
      new THREE.CylinderGeometry(mm(0.65), mm(0.65), mm(1.4), 16),
      metal,
      g,
    );
    pin.rotation.x = Math.PI / 2;
    pin.position.z = mm(-1.8);
  } else {
    const isA = p.id === "usb-a",
      isHdmi = p.id === "hdmi";
    box(
      `${p.id}-tongue`,
      p.w - 2,
      isA ? 1.7 : 0.65,
      1.6,
      0.18,
      isA ? dark : shell,
      0,
      isA ? 0.7 : 0,
      -1.3,
      g,
    );
    const count = isA ? 4 : isHdmi ? 10 : 8;
    for (let i = 0; i < count; i++)
      box(
        `${p.id}-contact-${i + 1}`,
        0.3,
        0.12,
        0.7,
        0.03,
        contacts,
        ((i - (count - 1) / 2) * (p.w - 3)) / count,
        isA ? -0.28 : 0.42,
        -0.85,
        g,
      );
  }
  // Accessibility marks below the ports, visible in the official rear diagram.
  if (p.id === "hdmi")
    box("hdmi-touch-line", 6, 0.6, 0.35, 0.15, shell, 0, -5.6, 0.1, g);
  if (p.id.startsWith("usb"))
    for (const dx of p.id === "usb-a" ? [-2, 0, 2] : [0]) {
      const dot = mesh(
        "usb-touch-dot",
        new THREE.SphereGeometry(mm(0.48), 12, 8),
        shell,
        g,
      );
      dot.position.set(mm(dx), mm(-5.5), mm(0.04));
    }
}
// Short visible upstream lead, deliberately separate from the 1:1 enclosure bounds.
const cable = group("upstream-usb-c-cable");
cable.userData = {
  note: "Display pose; shortened cable, not the full 1 m lead",
};
line(
  "upstream-flex",
  [
    [-57, 5, -27],
    [-61, 4, -43],
    [-72, 3, -62],
    [-90, 3, -71],
    [-106, 3, -72],
  ],
  1.7,
  rubber,
  cable,
);
const plug = group("usb-c-upstream-plug", cable);
plug.position.set(mm(-116), mm(3), mm(-72));
box("plug-overmould", 20, 5.7, 9.7, 1.8, rubber, 0, 0, 0, plug);
box("plug-strain-relief", 5, 4.3, 5.5, 1.3, rubber, 11, 0, 0, plug);
box("plug-metal-shell", 6.6, 2.5, 8.2, 1.1, metal, -13, 0, 0, plug);
box("plug-dark-tip", 0.08, 1.55, 6.7, 0.35, dark, -16.32, 0, 0, plug);

// PNG encoding keeps the GLB self-contained without a canvas or extra dependencies.
function crc32(buf) {
  let crc = 0xffffffff;
  for (const value of buf) {
    crc ^= value;
    for (let k = 0; k < 8; k++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function png(pixels, w, h) {
  const chunk = (name, data) => {
    const type = Buffer.from(name),
      length = Buffer.alloc(4),
      crc = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    crc.writeUInt32BE(crc32(Buffer.concat([type, data])));
    return Buffer.concat([length, type, data, crc]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(w);
  header.writeUInt32BE(h, 4);
  header[8] = 8;
  header[9] = 2;
  const rows = [];
  for (let y = 0; y < h; y++)
    rows.push(Buffer.from([0]), pixels.subarray(y * w * 3, (y + 1) * w * 3));
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(Buffer.concat(rows))),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}
const ts = 128,
  heights = new Float32Array(ts * ts),
  col = Buffer.alloc(ts * ts * 3),
  norm = Buffer.alloc(ts * ts * 3);
for (let y = 0; y < ts; y++)
  for (let x = 0; x < ts; x++) {
    const parity = (Math.floor(x / 16) + Math.floor(y / 16)) % 2;
    const across = (parity ? x : y) % 16,
      along = (parity ? y : x) % 16;
    const ridge = Math.pow(Math.sin((Math.PI * (across + 0.5)) / 16), 1.7);
    const end = 0.7 + 0.3 * Math.sin((Math.PI * (along + 0.5)) / 16);
    const thread = ridge * end;
    const fibre =
      1.4 * Math.sin(((parity ? x : y) * Math.PI) / 2) +
      0.6 * Math.sin(x * 7 + y * 11);
    heights[y * ts + x] = thread;
    const c = Math.round(31 + thread * 24 + fibre);
    col.fill(c, (y * ts + x) * 3, (y * ts + x) * 3 + 3);
  }
for (let y = 0; y < ts; y++)
  for (let x = 0; x < ts; x++) {
    const h = (a, b) => heights[((b + ts) % ts) * ts + ((a + ts) % ts)];
    const n = new THREE.Vector3(
      -(h(x + 1, y) - h(x - 1, y)) * 1.6,
      -(h(x, y + 1) - h(x, y - 1)) * 1.6,
      1,
    ).normalize();
    norm[(y * ts + x) * 3] = Math.round((n.x * 0.5 + 0.5) * 255);
    norm[(y * ts + x) * 3 + 1] = Math.round((n.y * 0.5 + 0.5) * 255);
    norm[(y * ts + x) * 3 + 2] = Math.round((n.z * 0.5 + 0.5) * 255);
  }
globalThis.FileReader = class {
  readAsArrayBuffer(blob) {
    blob.arrayBuffer().then((b) => {
      this.result = b;
      this.onloadend?.();
    });
  }
};
root.traverse((o) => {
  if (!o.isMesh) return;
  o.geometry.normalizeNormals();
  if (o.material === fabric) o.geometry.computeTangents();
  else o.geometry.deleteAttribute("uv");
});
const exported = Buffer.from(
  await new GLTFExporter().parseAsync(root, {
    binary: true,
    onlyVisible: true,
  }),
);
const jsonLen = exported.readUInt32LE(12),
  json = JSON.parse(exported.subarray(20, 20 + jsonLen).toString());
const binStart = 20 + jsonLen,
  binLen = exported.readUInt32LE(binStart);
let bin = exported.subarray(binStart + 8, binStart + 8 + binLen);
json.images = [];
json.textures = [];
json.samplers = [
  { magFilter: 9729, minFilter: 9987, wrapS: 10497, wrapT: 10497 },
];
for (const [name, pixels] of [
  ["Woven charcoal colour", col],
  ["Woven thread normal", norm],
]) {
  const data = png(pixels, ts, ts),
    offset = bin.length;
  json.bufferViews.push({
    buffer: 0,
    byteOffset: offset,
    byteLength: data.length,
  });
  json.images.push({
    name,
    mimeType: "image/png",
    bufferView: json.bufferViews.length - 1,
  });
  json.textures.push({ sampler: 0, source: json.images.length - 1 });
  bin = Buffer.concat([bin, data, Buffer.alloc((4 - (data.length % 4)) % 4)]);
}
const fabricJson = json.materials.find((m) => m.name === fabric.name);
fabricJson.pbrMetallicRoughness.baseColorTexture = { index: 0 };
fabricJson.normalTexture = { index: 1, scale: 0.65 };
json.buffers[0].byteLength = bin.length;
const j = Buffer.from(JSON.stringify(json)),
  padded = Buffer.concat([j, Buffer.alloc((4 - (j.length % 4)) % 4, 32)]);
const header = Buffer.alloc(20);
header.write("glTF");
header.writeUInt32LE(2, 4);
header.writeUInt32LE(28 + padded.length + bin.length, 8);
header.writeUInt32LE(padded.length, 12);
header.write("JSON", 16);
const binHeader = Buffer.alloc(8);
binHeader.writeUInt32LE(bin.length);
binHeader.write("BIN\0", 4);
const glb = Buffer.concat([header, padded, binHeader, bin]);
const bounds = new THREE.Box3()
  .setFromObject(root)
  .getSize(new THREE.Vector3());
let triangles = 0,
  meshes = 0;
root.traverse((o) => {
  if (o.isMesh) {
    meshes++;
    triangles +=
      (o.geometry.index?.count ?? o.geometry.attributes.position.count) / 3;
  }
});
const metadata = {
  ...root.userData,
  bytes: glb.length,
  meshes,
  triangles,
  boundsIncludingCableMmXYZ: bounds
    .toArray()
    .map((v) => Math.round(v * 1e5) / 100),
  estimates: [
    "Shoulder radii, control sizes/spacing, acoustic openings, underside and connector details estimated from official images.",
    "Cable is a shortened display pose; no power adapter. No internal speaker components or engineering tolerances.",
    "Buttons and IO have named groups. PBR colour and normal textures are embedded PNGs; no external textures or extensions required.",
  ],
};
await mkdir(out, { recursive: true });
await writeFile(new URL("microsoft-audio-dock.glb", out), glb);
await writeFile(
  new URL("model-info.json", out),
  JSON.stringify(metadata, null, 2) + "\n",
);
await copyFile(
  new URL("../licenses/three-MIT.txt", import.meta.url),
  new URL("three-MIT.txt", out),
);
await writeFile(
  new URL("README.txt", out),
  `Microsoft Audio Dock — independent display model, Revision 2\n\n` +
    `microsoft-audio-dock.glb: glTF 2.0, metres, +Y up, +Z front. Embedded PBR colour and normal PNGs.\n` +
    `Enclosure: 167.6 x 80.2 x 81 mm. Cable extends beyond these bounds.\n` +
    `Open audio-dock-3d-preview.html in a WebGL browser for an offline preview.\n` +
    `Drag to orbit; wheel to zoom; right-drag to pan. The cable visibility toggle only affects the preview.\n` +
    `Named button groups, rear IO and cable remain editable in a glTF-capable editor.\n\n` +
    `Display reconstruction from official photos, with estimated feature dimensions.\n` +
    `Shortened upstream cable; no power adapter, internal components or manufacturing tolerances.\n` +
    `This is an unofficial model. Microsoft and Teams marks belong to their respective owner.\n` +
    `Sources:\n${sources.join("\n")}\n\n` +
    `Regenerate from repository root:\nnode scripts/build-audio-dock-model.mjs\nnode scripts/export-audio-dock-preview.mjs\n` +
    `Three.js renderer license: three-MIT.txt. Icon licenses and original SVGs: icons/.\n`,
);
console.log(JSON.stringify(metadata, null, 2));

await mkdir(new URL("icons/", out), { recursive: true });
for (const file of [
  "SOURCES.json",
  ...iconSources.icons.map((icon) => icon.file),
]) {
  await copyFile(new URL(file, iconDirectory), new URL(`icons/${file}`, out));
}
await copyFile(
  new URL("../licenses/audio-dock-phosphor-MIT.txt", import.meta.url),
  new URL("icons/phosphor-MIT.txt", out),
);
await copyFile(
  new URL("../licenses/audio-dock-simple-icons-CC0.txt", import.meta.url),
  new URL("icons/simple-icons-CC0.txt", out),
);
// The client consumes the same GLB; the transparent poster is rendered separately
// by scripts/render-audio-dock-poster.js using the production scene.
const clientAssets = new URL(
  "../src/assets/devices/ms-audio-dock/",
  import.meta.url,
);
await mkdir(clientAssets, { recursive: true });
await copyFile(
  new URL("microsoft-audio-dock.glb", out),
  new URL("audio-dock.glb", clientAssets),
);
await copyFile(
  new URL("model-info.json", out),
  new URL("model-info.json", clientAssets),
);
