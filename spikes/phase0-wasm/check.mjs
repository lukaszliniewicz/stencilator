// Spike 1: runs the browser build under Node and prints the same lines as the
// native tools, so the two can be compared with diff.
// Usage: node check.mjs <pkg-dir> <repo-root>
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [pkg, root] = process.argv.slice(2);
const wasm = await import(pathToFileURL(join(pkg, "phase0_wasm.js")).href);
wasm.initSync({ module: readFileSync(join(pkg, "phase0_wasm_bg.wasm")) });

// FNV-1a 64, the same digest the native tools print.
const fnv = (bytes) => {
  let h = 0xcbf29ce484222325n;
  for (const b of bytes) h = BigInt.asUintN(64, (h ^ BigInt(b)) * 0x100000001b3n);
  return h.toString(16).padStart(16, "0");
};

const font = readFileSync(join(root, "fixtures/fonts/NotoSans-Bold.ttf"));
for (const name of ["clip-mask-text", "badge"]) {
  const svg = readFileSync(join(root, `fixtures/svg/${name}.svg`), "utf8");
  for (const ppm of [4, 10, 25]) console.log(`render ${name} ${ppm}: ${wasm.render_digest(svg, font, ppm)}`);
}
for (const rotation of [0, 7, 33.3]) {
  const stl = wasm.connector_plate_stl(rotation);
  console.log(`plate ${rotation}: ${stl.length} bytes ${fnv(stl)}`);
}
const badge = readFileSync(join(root, "fixtures/svg/badge.svg"), "utf8");
for (const label of [1, 2]) console.log(`trace badge ${label}: ${wasm.trace_digest(badge, font, 10, label)}`);
for (const file of process.argv.slice(4)) console.log(`image ${file}: ${wasm.image_size(readFileSync(file))}`);
