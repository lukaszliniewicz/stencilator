// Runs the browser build under Node and the command line on the reference charts, and
// checks that every file they write is identical, byte for byte.
// Usage: node parity.mjs <wasm-pkg-dir> <stencilator-binary> <repo-root>
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [pkg, cli, root] = process.argv.slice(2);
const engine = await import(pathToFileURL(join(pkg, "stencilator_wasm.js")).href);
engine.initSync({ module: readFileSync(join(pkg, "stencilator_wasm_bg.wasm")) });

const hex = ([r, g, b]) => [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex").slice(0, 16);
const GRID = [0, 0, 0];

const cases = JSON.parse(readFileSync(join(root, "fixtures/reference/charts.json"), "utf8")).cases;
const work = mkdtempSync(join(tmpdir(), "stencilator-parity-"));
let failures = 0;

for (const [n, fixture] of cases.entries()) {
  const s = fixture.settings;
  const image = join(root, "fixtures/reference", fixture.image);

  const settings = new engine.Settings();
  settings.canvasWidthMm = s.canvas_mm[0];
  settings.canvasHeightMm = s.canvas_mm[1];
  settings.tileMm = s.tile_mm;
  settings.lineMm = s.line_mm;
  settings.dpi = s.dpi;
  settings.mode = s.mode;
  settings.colours = s.n_colours;
  settings.background = parseInt(hex(s.background), 16);
  const source = new engine.Source(readFileSync(image));
  const chart = source.chart(settings);

  const browser = {};
  for (const format of ["png", "jpeg", "pdf"]) {
    browser[`chart.${format}`] = chart.chartFile(format, parseInt(hex(GRID), 16), true);
    browser[`tiles.${format}`] = chart.chartFile(format, parseInt(hex(GRID), 16), false);
  }
  const stencils = s.line_mm > 0;
  if (stencils) {
    browser["stencils.zip"] = chart.stencilsZip(1.2);
    const names = chart.stencilNames();
    browser[`stencils/${names[0]}`] = chart.stencil(0, 1.2);
    browser[`stencils/${names.at(-1)}`] = chart.stencil(names.length - 1, 1.2);
  }

  const dir = join(work, String(n));
  mkdirSync(dir);
  const common = [
    "build", image,
    "--canvas", `${s.canvas_mm[0]}x${s.canvas_mm[1]}`,
    "--tile", String(s.tile_mm), "--line", String(s.line_mm), "--dpi", String(s.dpi),
    "--mode", s.mode, "--colours", String(s.n_colours),
    "--background", hex(s.background), "--grid-colour", hex(GRID),
  ];
  for (const format of ["png", "jpeg", "pdf"]) {
    const extra = stencils && format === "png" ? ["--stencils", join(dir, "stencils.zip")] : [];
    const folder = stencils && format === "pdf" ? ["--stencils", join(dir, "stencils")] : [];
    execFileSync(cli, [...common, "--chart", join(dir, `chart.${format}`), "--tiles", join(dir, `tiles.${format}`), ...extra, ...folder], {
      stdio: ["ignore", "ignore", "inherit"],
    });
  }

  for (const [name, bytes] of Object.entries(browser)) {
    const native = readFileSync(join(dir, name));
    const same = Buffer.compare(Buffer.from(bytes), native) === 0;
    if (!same) failures += 1;
    console.log(`${same ? "same" : "DIFFERENT"}  ${fixture.name.padEnd(28)} ${name.padEnd(48)} ${digest(native)}`);
  }
  chart.free();
  source.free();
  settings.free();
}

rmSync(work, { recursive: true, force: true });
if (failures > 0) {
  console.error(`${failures} file(s) differ between the browser build and the command line`);
  process.exit(1);
}
console.log("browser build and command line agree on every file");
