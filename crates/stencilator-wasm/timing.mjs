// Times the browser build's free-form stages under Node, whose WebAssembly engine is
// Chrome's, on drawings, text and a noisy photo, and reports the memory it ends up using.
// Usage: node timing.mjs <wasm-pkg-dir> <repo-root>
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [pkg, root] = process.argv.slice(2);
const engine = await import(pathToFileURL(join(pkg, "stencilator_wasm.js")).href);
const wasm = engine.initSync({ module: readFileSync(join(pkg, "stencilator_wasm_bg.wasm")) });
// The built-in fonts, as every shell gives them to the engine.
const fonts = new engine.Fonts();
for (const name of engine.Fonts.builtIn()) fonts.add(name, readFileSync(join(root, "fonts", name)));
const badge = readFileSync(join(root, "fixtures/svg/badge.svg"));
const photo = readFileSync(join(root, "fixtures/reference/plates-photo.png"));
const text = () => {
  const project = engine.Project.text("");
  project.setText("PAINT\nCLUB", 40, 0);
  return project;
};

const cases = [
  ["badge, own size (70 mm)", () => engine.Project.svg("badge.svg", badge), {}],
  ["badge, 200 mm", () => engine.Project.svg("badge.svg", badge), { freeform: { size: { width_mm: 200 } } }],
  ["text, 40 mm", text, {}],
  ["photo, 200 mm, 8 paints", () => engine.Project.picture("photo.png", photo), { colours: { count: 8 }, freeform: { size: { width_mm: 200 } } }],
];
const since = (t) => `${(performance.now() - t).toFixed(0).padStart(5)} ms`;
const hex = (palette, i) => `#${[...palette.slice(3 * i, 3 * i + 3)].map((v) => v.toString(16).padStart(2, "0")).join("")}`;

console.log(`${"".padEnd(24)} ${["artwork", "labels", "plates", "3MF", "merged"].map((s) => s.padStart(8)).join(" ")}   cells`);
for (const [name, make, settings] of cases) {
  const project = make();
  project.setSettings(JSON.stringify({ mode: "freeform", ...settings }));
  const times = [];
  let t = performance.now();
  const artwork = new engine.Artwork(project, fonts);
  times.push(since(t));
  t = performance.now();
  let labels = artwork.labels(project);
  times.push(since(t));
  t = performance.now();
  const plates = labels.plates(project);
  times.push(since(t));
  t = performance.now();
  plates.threeMf(project);
  times.push(since(t));
  // A merge relabels from the cells the artwork labelled last.
  const palette = labels.palette();
  project.setSettings(JSON.stringify({ mode: "freeform", ...settings, stencils: { merges: [[hex(palette, 0), hex(palette, 1)]] } }));
  labels.free();
  t = performance.now();
  labels = artwork.labels(project);
  times.push(since(t));
  console.log(`${name.padEnd(24)} ${times.join(" ")}   ${labels.width} x ${labels.height}`);
  for (const object of [labels, plates, artwork, project]) object.free();
}
console.log(`WebAssembly memory: ${(wasm.memory.buffer.byteLength / 2 ** 20).toFixed(0)} MB`);
