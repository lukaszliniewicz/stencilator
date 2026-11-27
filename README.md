# Stencilator

Turn a drawing, a photo or a few words into painting stencils you can
3D-print. You get a stencil for each paint you choose. Lay the first one on
your work, paint through the holes, then swap it for the next colour.

[Open Stencilator in your browser](https://stencilator.liniewicz.eu) or
[download the command-line tool or Claude Desktop bundle](https://github.com/lukaszliniewicz/stencilator/releases).
There’s nothing to install to use the web app, and it never uploads your artwork.

## Make your first stencil

1. **Pick a mode.** Use **Tiles** if you want a grid of squares, especially
   for pixel art. Click **Try the example** or open a PNG, JPEG or WebP.
   Choose **Free-form** if you want the holes to follow the artwork. Click
   **Try the badge**, open an SVG or picture, or choose **Type text**.
2. **Set the size and pick your paints.** In Tiles, choose a tile size and
   fit the grid to your canvas, or make one tile per pixel. In Free-form,
   set the artwork size and enter your print bed dimensions. Untick any
   paint that needs no stencil, such as the colour of the surface itself.
3. **Look at the stencil before exporting.** Grid lines in Tiles hold the
   stencil together. In Free-form, select **Plate**, then **Top**, to see
   loose pieces, thin strips that might break and small holes that might
   close up. Switch to **3D** to see the parts as they will print.
4. **Open Files.** Tiles gives you a chart and an STL for each paint. Print
   the PDF chart at actual size (100%). Free-form gives you a 3MF with the
   whole set or separate STLs. Check each plate at actual size in your
   slicer and lay it flat on the bed.

Use **Save project** to keep the artwork, fonts, paint choices and settings
in a `.stencilator` file. Open it again in the app, on the command line or
through MCP to carry on where you left off.

### Keep the middle of an O in place

The centre of an O is an *island*: there's nothing to hold that part of the
stencil once you lift it off the print bed. Under **Island supports**, try
**Flat** for bars that sit against the work and leave unpainted lines, or
**Raised** for beams that give spray room to reach underneath. In **Top**
view you can move, add or remove supports. For lettering, you can also try
the built-in **Allerta Stencil** font, which has bridges drawn into the letters.

### Line up the colours

Enable **Use a keyed frame** to make a frame that each plate fits into the
same way round. Keep the frame still on the work and seat each plate against
the same corner. The frame comes with the plates in the 3MF or STL ZIP.

Set the coat order in the paint list and check **Painted result** for the
shapes the coats will cover, including gaps from flat bridges. This preview
uses the openings against the work; it doesn't simulate spray angle or bleed.
Under **Plate finishing**, you can widen the spray side of the openings,
overlap neighbouring coats, or mark a plate with its number or an approximate
RAL paint code.

The raised connector dimensions and frame clearance are starting points.
Print and spray a small sample to check strength, fit and coverage before
making a full set.

## Use the command line

Extract the standalone ZIP for Linux x86-64 (glibc 2.35 or newer), macOS
Apple Silicon or Windows x86-64. Run these commands from the extracted
folder. In Windows PowerShell, use `.\stencilator.exe` in place of `./stencilator`.

Make an 80 mm-wide stencil from text, with flat bridges to hold the centres:

```sh
./stencilator freeform --text "HELLO" --width 80 --flat-bridges --3mf hello.3mf --save hello.stencilator
```

Make a chart with up to six paints and a ZIP of stencils from your own picture:

```sh
./stencilator tiles picture.png --canvas 180x180 --tile 5 --line 0.8 --colours 6 --chart chart.pdf --stencils stencils.zip
```

All sizes are in millimetres. Use `--pixels` instead of `--canvas` for
one tile per pixel. Use `--merge` to combine paints and `--skip` to leave
one out. Give `--stencils` a folder name to get individual STL files.

Make a set with a keyed frame from an SVG drawing:

```sh
./stencilator freeform drawing.svg --registration --3mf drawing.3mf
```

Use one solid colour per intended paint in your SVG. See
[how to mark the paints](docs/svg-paints.md) for examples.

Leave out the output options (`--3mf`, `--stencils`, `--chart`, `--save`)
to check a design without writing files. Add `--json` for a report another
program can read. Run `./stencilator tiles --help` or
`./stencilator freeform --help` for all the options.

## With an AI assistant

With Claude Desktop, install the `.mcpb` for your system and choose a folder
for your artwork and exports. Other local MCP clients can run
`stencilator mcp --workspace /path/to/artwork`. Ask your assistant to check a
design, make print files or save an editable project you can open in the app.
The [setup guide](docs/mcp.md) has commands for Claude Code, Codex and OpenCode.

## Development

Build the command from a source checkout with Rust 1.88 or newer:

```sh
cargo build --release --locked -p stencilator-cli
```

The executable is in `target/release/`. The command line, MCP server and
browser app share the Rust engine in `crates/stencilator-core`. `make check`
runs the Rust tests, lints and dependency checks. It needs the
`wasm32-unknown-unknown` Rust target, `cargo-deny` and `cargo-machete`.
`make parity` compares the files written by the native and browser builds.

For the Svelte app, install Node 24, pnpm 11.9.0, `wasm-bindgen-cli` 0.2.130
and binaryen. Run `make wasm`, then `pnpm install` and `pnpm dev` in `app/`.
Run `pnpm check` and `pnpm e2e` there before opening a PR. The first browser
test run needs `pnpm exec playwright install chromium`.

See the [project file guide](docs/projects.md) for the saved JSON format.

## Licence

MIT; see [LICENSE](LICENSE). This software is based in part on the work of the
Independent JPEG Group, through the `jpeg-encoder` crate that writes JPEG charts. The
built-in fonts are under the SIL Open Font License 1.1; see [fonts/](fonts/README.md).
