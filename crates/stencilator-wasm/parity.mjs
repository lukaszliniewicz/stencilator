// Runs the browser build under Node and the command line on the same work, and checks
// that every file they write is identical, byte for byte: tile charts and stencils for
// the reference charts and a pixel-art case with merged and skipped paints, and free-form
// plates, 3MF files, reports and projects for an SVG drawing, typed text and a picture.
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
];
const work = mkdtempSync(join(tmpdir(), "stencilator-parity-"));
let failures = 0;
const number = (colour) => parseInt(hex(colour), 16);

for (const [n, fixture] of cases.entries()) {
  const s = fixture.settings;
  const image = fixture.image;
  const pixels = s.sizing === "pixels";

  const settings = new engine.Settings();
  if (pixels) settings.sizing = "pixels";
  else {
    settings.canvasWidthMm = s.canvas_mm[0];
    settings.canvasHeightMm = s.canvas_mm[1];
  }
  settings.tileMm = s.tile_mm;
  settings.lineMm = s.line_mm;
  settings.dpi = s.dpi;
  settings.mode = s.mode;
  settings.colours = s.n_colours;
  settings.background = number(s.background);
  const source = new engine.Source(readFileSync(image));
  const chart = source.chart(settings, Uint32Array.from(fixture.merges.flat().map(number)));

  const browser = {};
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
    ...fixture.merges.flatMap(([from, to]) => ["--merge", `${hex(from)}=${hex(to)}`]),
    ...fixture.skip.flatMap((colour) => ["--skip", hex(colour)]),
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

// Free-form: each case makes a project in the browser build and the same with the
// command line's options.
// The built-in fonts, as every shell gives them to the engine.
const fonts = new engine.Fonts();
for (const name of engine.Fonts.builtIn()) fonts.add(name, readFileSync(join(root, "fonts", name)));
const FOX = readFileSync(join(root, "fixtures/pixel-art/fox.png"));
const freeform = [
  {
    name: "badge drawing",
    project: () => engine.Project.svg("badge.svg", readFileSync(join(root, "fixtures/svg/badge.svg"))),
    args: [join(root, "fixtures/svg/badge.svg")],
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
  if (fixture.before) {
    artwork.labels(project).free();
    project.setSettings(JSON.stringify(fixture.settings));
  }
  const labels = artwork.labels(project);
  const plates = labels.plates(project);
  const names = plates.names();
  const browser = {
    "plates.3mf": plates.threeMf(project),
    "plates.zip": plates.zip(project),
    "project.stencilator": project.file(),
    [`plates/${names[0]}`]: plates.stl(0, project),
    [`plates/${names.at(-1)}`]: plates.stl(names.length - 1, project),
  };

  const dir = join(work, `freeform-${n}`);
  mkdirSync(dir);
  // Its warnings are the report's, compared below; a failure still shows them.
  const run = (extra) => execFileSync(cli, ["freeform", ...fixture.args, ...extra], { stdio: ["ignore", "pipe", "pipe"] });
  const report = JSON.parse(run([
    "--3mf", join(dir, "plates.3mf"), "--stencils", join(dir, "plates.zip"),
    "--save", join(dir, "project.stencilator"), "--json",
  ]));
  run(["--stencils", join(dir, "plates")]);

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
