import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import ts from "typescript";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { parse } = createRequire(require.resolve("vue"))("@vue/compiler-sfc");
const { baseParse } = createRequire(require.resolve("vue"))(
  "@vue/compiler-dom",
);
const root = path.resolve(import.meta.dirname, "..");
function flatten(value, prefix = "", result = {}) {
  assert(
    value !== null && typeof value === "object" && !Array.isArray(value),
    `Translation catalog must contain only nested objects and string values: ${prefix}`,
  );
  for (const [key, text] of Object.entries(value)) {
    assert(/^[A-Za-z0-9_]+$/.test(key), `Invalid translation key: ${key}`);
    const name = prefix ? `${prefix}.${key}` : key;
    if (typeof text === "string") result[name] = text;
    else flatten(text, name, result);
  }
  return result;
}
const en = flatten(
  JSON.parse(fs.readFileSync(path.join(root, "src/locales/en.json"), "utf8")),
);
const zh = flatten(
  JSON.parse(
    fs.readFileSync(path.join(root, "src/locales/zh-CN.json"), "utf8"),
  ),
);
assert.deepEqual(
  Object.keys(en).sort(),
  Object.keys(zh).sort(),
  "Language resource keys differ",
);
const placeholders = (text) =>
  [...new Set([...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].sort();
for (const key of Object.keys(en)) {
  assert(en[key].trim() && zh[key].trim(), `Empty translation: ${key}`);
  assert(
    !en[key].includes("|") && !zh[key].includes("|"),
    `Use separate JSON plural branches instead of pipe syntax: ${key}`,
  );
  assert.deepEqual(
    placeholders(en[key]),
    placeholders(zh[key]),
    `Interpolation mismatch: ${key}`,
  );
}
let sources = 0;
function checkScript(source, file) {
  const ast = ts.createSourceFile(
    file + ".ts",
    source,
    ts.ScriptTarget.Latest,
    true,
  );
  function walk(node) {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node))
      assert(
        !/[\u3400-\u9fff]/.test(node.text),
        `Hardcoded UI text in ${file}: ${node.text}`,
      );
    if (
      ts.isCallExpression(node) &&
      node.expression.getText(ast) === "t" &&
      ts.isStringLiteral(node.arguments[0])
    )
      assert(
        Object.hasOwn(en, node.arguments[0].text) ||
          Object.hasOwn(en, `${node.arguments[0].text}.other`),
        `Missing translation in ${file}: ${node.arguments[0].text}`,
      );
    ts.forEachChild(node, walk);
  }
  walk(ast);
}
function checkTemplate(node, file) {
  if (node.type === 2)
    assert(
      !/[\u3400-\u9fff]/.test(node.content),
      `Hardcoded template text in ${file}: ${node.content}`,
    );
  if (node.type === 5) checkScript(node.content.content, file);
  for (const prop of node.props ?? []) {
    if (prop.type === 6 && prop.value)
      assert(
        !/[\u3400-\u9fff]/.test(prop.value.content),
        `Hardcoded attribute in ${file}`,
      );
    if (prop.type === 7 && prop.exp) checkScript(prop.exp.content, file);
  }
  node.children?.forEach((child) => checkTemplate(child, file));
}
function visit(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!["assets", "locales"].includes(entry.name)) visit(file);
      continue;
    }
    if (!/\.(ts|vue)$/.test(file) || entry.name === "bindings.ts") continue;
    sources++;
    const source = fs.readFileSync(file, "utf8");
    if (entry.name.endsWith(".ts")) checkScript(source, file);
    else {
      const { descriptor } = parse(source);
      if (descriptor.scriptSetup)
        checkScript(descriptor.scriptSetup.content, file);
      if (descriptor.template)
        checkTemplate(baseParse(descriptor.template.content), file);
    }
  }
}
visit(path.join(root, "src"));
console.log(
  `i18n: ${Object.keys(en).length} bilingual messages and ${sources} UI sources checked`,
);
