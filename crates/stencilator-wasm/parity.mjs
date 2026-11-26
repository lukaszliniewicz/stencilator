// Runs the browser build under Node and the command line on the same work, and checks
// that every file they write is identical, byte for byte: tile charts and stencils for
// the reference charts, pixel art with merged and skipped paints, and a photo with a
// 64-bit seed; free-form plates, 3MF files, reports and projects for SVG, text and pictures.
// Usage: node parity.mjs <wasm-pkg-dir> <stencilator-binary> <repo-root>
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { pathToFileURL } from "node:url";

const [pkg, cli, root] = process.argv.slice(2);
const engine = await import(pathToFileURL(join(pkg, "stencilator_wasm.js")).href);
engine.initSync({ module: readFileSync(join(pkg, "stencilator_wasm_bg.wasm")) });

const hex = ([r, g, b]) => [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex").slice(0, 16);
const GRID = [0, 0, 0];

const reference = JSON.parse(readFileSync(join(root, "fixtures/reference/charts.json"), "utf8")).cases;
const cases = [
  ...reference.map((c) => ({ ...c, image: join(root, "fixtures/reference", c.image), merges: [], skip: [] })),
  {
    name: "fox, one tile per pixel",
    image: join(root, "fixtures/pixel-art/fox.png"),
    settings: { sizing: "pixels", tile_mm: 10, line_mm: 0.8, dpi: 300, mode: "colour", n_colours: 16, background: [255, 255, 255] },
    merges: [[[0x9c, 0x3d, 0x17], [0xe8, 0x77, 0x2e]]],
    skip: [[255, 255, 255]],
  },
  {
    name: "photo with a 64-bit seed",
    image: join(root, "fixtures/reference/plates-photo.png"),
    settings: { ...reference[0].settings, n_colours: 7 },
    seed: "18446744073709551615",
    merges: [],
    skip: [],
  },
];
const work = mkdtempSync(join(tmpdir(), "stencilator-parity-"));
let failures = 0;
const number = (colour) => parseInt(hex(colour), 16);
const colour = (rgb) => `#${hex(rgb)}`;

for (const [n, fixture] of cases.entries()) {
  const s = fixture.settings;
  const image = fixture.image;
  const pixels = s.sizing === "pixels";
  const bytes = readFileSync(image);
  let project = engine.Project.picture(basename(image), bytes);
  if (fixture.seed) {
    const saved = new TextDecoder().decode(project.file()).replace(/"seed": \d+/, `"seed": ${fixture.seed}`);
    project.free();
    project = engine.Project.open(new TextEncoder().encode(saved));
  }
  project.setSettings(JSON.stringify({
    mode: "tiles",
    colours: { mode: s.mode, count: s.n_colours, background: colour(s.background) },
    tiles: {
      sizing: s.sizing ?? "canvas", canvas_mm: s.canvas_mm, tile_mm: s.tile_mm, line_mm: s.line_mm,
      dpi: s.dpi, grid_colour: colour(GRID),
    },
    stencils: {
      merges: fixture.merges.map((pair) => pair.map(colour)), skipped: fixture.skip.map(colour),
      stencil_canvas_colour: true,
    },
  }), true);
  const source = new engine.Source(bytes);
  const chart = project.tileChart(source);

  const browser = { "project.stencilator": project.file() };
  for (const format of ["png", "jpeg", "pdf"]) {
    browser[`chart.${format}`] = chart.chartFile(format, number(GRID), true);
    browser[`tiles.${format}`] = chart.chartFile(format, number(GRID), false);
  }
  const stencils = s.line_mm > 0;
  if (stencils) {
    browser["stencils.zip"] = chart.stencilsZip(1.2, Uint32Array.from(fixture.skip.map(number)));
    // The first and last stencils that are not skipped.
    const names = chart.stencilNames();
    const skipped = new Set(fixture.skip.map(hex));
    const painted = names.map((name, i) => [name, i]).filter(([name]) => !skipped.has(name.slice(-10, -4)));
    for (const [name, i] of [painted[0], painted.at(-1)]) browser[`stencils/${name}`] = chart.stencil(i, 1.2);
  }

  const dir = join(work, String(n));
  mkdirSync(dir);
  // The browser is given its skip list in full, so the command line is told not to add
  // the canvas colour to it.
  const common = [
    "tiles", image, "--stencil-canvas-colour",
    ...(pixels ? ["--pixels"] : ["--canvas", `${s.canvas_mm[0]}x${s.canvas_mm[1]}`]),
    "--tile", String(s.tile_mm), "--line", String(s.line_mm), "--dpi", String(s.dpi),
    "--mode", s.mode, "--colours", String(s.n_colours),
    "--background", hex(s.background), "--grid-colour", hex(GRID),
    ...(fixture.seed ? ["--seed", fixture.seed] : []),
    ...fixture.merges.flatMap(([from, to]) => ["--merge", `${hex(from)}=${hex(to)}`]),
    ...fixture.skip.flatMap((colour) => ["--skip", hex(colour)]),
  ];
  for (const format of ["png", "jpeg", "pdf"]) {
    const extra = stencils && format === "png" ? ["--stencils", join(dir, "stencils.zip")] : [];
    const folder = stencils && format === "pdf" ? ["--stencils", join(dir, "stencils")] : [];
    const saved = format === "png" ? ["--save", join(dir, "project.stencilator")] : [];
    execFileSync(cli, [...common, "--chart", join(dir, `chart.${format}`), "--tiles", join(dir, `tiles.${format}`), ...extra, ...folder, ...saved], {
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
  project.free();
}

// Free-form: each case makes a project in the browser build and the same with the
// command line's options.
// The built-in fonts, as every shell gives them to the engine.
const fonts = new engine.Fonts();
for (const name of engine.Fonts.builtIn()) fonts.add(name, readFileSync(join(root, "fonts", name)));
const badge = join(root, "fixtures/svg/badge.svg");
const FOX = readFileSync(join(root, "fixtures/pixel-art/fox.png"));
const freeform = [
  {
    name: "badge drawing",
    project: () => engine.Project.svg("badge.svg", readFileSync(badge)),
    args: [badge],
  },
  {
    name: "badge with flat bridges",
    project: () => engine.Project.svg("badge.svg", readFileSync(badge)),
    settings: { freeform: { bridge_width_mm: 1.2 } },
    args: [badge, "--flat-bridges"],
  },
  {
    name: "badge, every paint skipped",
    project: () => engine.Project.svg("badge.svg", readFileSync(badge)),
    settings: { stencils: { skipped: ["#1d2f5c", "#ef7d2d"] } },
    args: [badge, "--skip", "1d2f5c", "--skip", "ef7d2d"],
  },
  {
    name: "two lines of text",
    project: () => {
      const project = engine.Project.text("");
      project.setText("PAINT\nCLUB", 20, 0x1d2f5c);
      return project;
    },
    args: ["--text", "PAINT", "--text", "CLUB", "--font-size", "20", "--text-colour", "1d2f5c"],
  },
  {
    name: "text in the stencil font",
    project: () => {
      const project = engine.Project.text("");
      project.setText("ABBA 80", 25, 0);
      project.addBuiltInFont("AllertaStencil-Regular.ttf");
      return project;
    },
    args: ["--text", "ABBA 80", "--font-size", "25", "--font", "Allerta Stencil"],
  },
  {
    name: "fox picture, merged",
    project: () => engine.Project.picture("fox.png", FOX),
    // Labelled once without the merge and once with it, so that the second labelling
    // comes from the cells the first one left.
    before: { mode: "freeform", freeform: { size: { width_mm: 110 }, cell_mm: 0.25 } },
    settings: {
      mode: "freeform",
      freeform: { size: { width_mm: 110 }, cell_mm: 0.25 },
      stencils: { merges: [["#9c3d17", "#e8772e"]], skipped: ["#ffffff"], stencil_canvas_colour: true },
    },
    args: [
      join(root, "fixtures/pixel-art/fox.png"),
      "--width", "110", "--cell", "0.25", "--merge", "9c3d17=e8772e", "--skip", "ffffff", "--stencil-canvas-colour",
    ],
  },
];

for (const [n, fixture] of freeform.entries()) {
  const project = fixture.project();
  if (fixture.before) project.setSettings(JSON.stringify(fixture.before));
  const artwork = new engine.Artwork(project, fonts);
  if (fixture.before) artwork.labels(project).free();
  if (fixture.settings) project.setSettings(JSON.stringify(fixture.settings));
  const labels = artwork.labels(project);
  const plates = labels.plates(project);
  const names = plates.names();
  const browser = { "project.stencilator": project.file() };
  if (names.length) {
    Object.assign(browser, {
      "plates.3mf": plates.threeMf(project),
      "plates.zip": plates.zip(project),
      [`plates/${names[0]}`]: plates.stl(0, project),
      [`plates/${names.at(-1)}`]: plates.stl(names.length - 1, project),
    });
  }

  const dir = join(work, `freeform-${n}`);
  mkdirSync(dir);
  // Its warnings are the report's, compared below; a failure still shows them.
  const run = (extra) => execFileSync(cli, ["freeform", ...fixture.args, ...extra], { stdio: ["ignore", "pipe", "pipe"] });
  const printFiles = names.length ? ["--3mf", join(dir, "plates.3mf"), "--stencils", join(dir, "plates.zip")] : [];
  const report = JSON.parse(run([...printFiles, "--save", join(dir, "project.stencilator"), "--json"]));
  if (names.length) run(["--stencils", join(dir, "plates")]);

  // The same paints and plates, and the same report.
  const palette = [...labels.palette()];
  const counts = [...labels.cellCounts()];
  const paints = report.palette.map((p) => [p.colour, p.cells]);
  const own = counts.map((cells, i) => [`#${hex(palette.slice(3 * i, 3 * i + 3))}`, cells]);
  const reports = JSON.stringify(JSON.parse(plates.report())) === JSON.stringify(report.plates);
  const opened = engine.Project.open(readFileSync(join(dir, "project.stencilator")));
  browser["project.stencilator, opened and saved"] = opened.file();
  for (const [what, same] of [["paints", JSON.stringify(paints) === JSON.stringify(own)], ["report", reports]]) {
    if (!same) failures += 1;
    console.log(`${same ? "same" : "DIFFERENT"}  ${fixture.name.padEnd(28)} ${what}`);
  }
  for (const [name, bytes] of Object.entries(browser)) {
    const native = readFileSync(join(dir, name.split(",")[0]));
    const same = Buffer.compare(Buffer.from(bytes), native) === 0;
    if (!same) failures += 1;
    console.log(`${same ? "same" : "DIFFERENT"}  ${fixture.name.padEnd(28)} ${name.padEnd(48)} ${digest(native)}`);
  }
  for (const object of [opened, plates, labels, artwork, project]) object.free();
}

rmSync(work, { recursive: true, force: true });
if (failures > 0) {
  console.error(`${failures} file(s) differ between the browser build and the command line`);
  process.exit(1);
}
console.log("browser build and command line agree on every file");
