import * as THREE from "three";
import { SVGLoader } from "three/addons/loaders/SVGLoader.js";

// These pinned assets contain only filled paths. Supply SVGLoader with those
// nodes in Node.js; this is deliberately not a general-purpose SVG/DOM parser.
class FilledPathDocument {
  parseFromString(source) {
    const tags = [...source.matchAll(/<([\w:-]+)\b/g)].map((m) => m[1]);
    if (tags.some((tag) => !["svg", "title", "path"].includes(tag)))
      throw new Error("Audio Dock icons must contain only filled SVG paths");
    if (/\b(?:transform|style|stroke|class|filter)\s*=/.test(source))
      throw new Error(
        "Styled/transformed SVGs must be flattened before import",
      );
    const nodes = [...source.matchAll(/<path\b([^>]*)\/?\s*>/g)].map((m) => {
      const attrs = Object.fromEntries(
        [...m[1].matchAll(/([\w:-]+)\s*=\s*(["'])(.*?)\2/g)].map((a) => [
          a[1],
          a[3],
        ]),
      );
      if (!attrs.d) throw new Error("Missing SVG path data");
      return {
        nodeType: 1,
        nodeName: "path",
        childNodes: [],
        hasAttribute: (name) => Object.hasOwn(attrs, name),
        getAttribute: (name) =>
          name === "fill" ? "#000" : (attrs[name] ?? null),
      };
    });
    if (!nodes.length) throw new Error("Empty SVG icon");
    return {
      querySelectorAll: () => [],
      documentElement: {
        nodeType: 1,
        nodeName: "svg",
        childNodes: nodes,
        hasAttribute: () => false,
        getAttribute: () => null,
      },
    };
  }
}

export function iconGeometry(source, { widthMm, heightMm }) {
  const previous = globalThis.DOMParser;
  let paths;
  try {
    globalThis.DOMParser = FilledPathDocument;
    paths = new SVGLoader().parse(source).paths;
  } finally {
    if (previous === undefined) delete globalThis.DOMParser;
    else globalThis.DOMParser = previous;
  }
  const shapes = paths.flatMap((path) => path.toShapes());
  const flat = new THREE.ShapeGeometry(shapes, 32);
  flat.computeBoundingBox();
  const size = flat.boundingBox.getSize(new THREE.Vector3());
  const centre = flat.boundingBox.getCenter(new THREE.Vector3());
  const scale = widthMm ? widthMm / size.x / 1000 : heightMm / size.y / 1000;
  flat.dispose();
  const geometry = new THREE.ExtrudeGeometry(shapes, {
    depth: 0.00004 / scale,
    steps: 1,
    bevelEnabled: false,
    curveSegments: 32,
  });
  geometry.translate(-centre.x, -centre.y, 0);
  geometry.scale(scale, scale, scale);
  // SVG Y points toward the front. The z=0 cap faces upward and the 0.04 mm
  // extrusion extends down into the key surface; no mirrored glyphs or tubes.
  geometry.rotateX(Math.PI / 2);
  geometry.clearGroups();
  return geometry;
}
