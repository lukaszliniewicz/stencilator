# Stencilator

Stencilator turns artwork into painting stencils you can 3D-print. It works in two ways.

In tile mode it lays a grid of square tiles over a picture, reduces it to a handful of
colours, and writes a chart to paint from along with one stencil per colour. Lay a
stencil on the canvas, paint through the holes, move on to the next colour, and the
picture builds up tile by tile.

In free-form mode the openings follow the shapes in the artwork instead: a logo drawn
as an SVG, a picture, or text you type. You get one plate per paint, and a report of
what may not print well, such as the middle of an "O", which falls out of a stencil
unless something holds it.

It is written in Rust so that one engine can run on the command line, in a web browser
and, later, behind an AI assistant, giving the same result everywhere.

## Status

Early, but usable. Both modes work on the command line and in the web app built from
this repository. The [hosted app](https://stencilator.liniewicz.eu) currently serves tile
mode. The app runs entirely in your browser, so your artwork is never uploaded.
There are no prebuilt releases of the command yet.

## Trying it

You need Rust 1.88 or newer. Build the command with

```sh
cargo build --release -p stencilator-cli
```

### Tiles

Run it on a picture:

```sh
target/release/stencilator tiles logo.png --canvas 300x200 --tile 5 --line 0.8 \
    --colours 8 --chart chart.pdf --stencils stencils/
```

This finds the largest grid of 5 mm tiles, with 0.8 mm lines between them, that fits on
a 300 by 200 mm canvas while keeping the picture's proportions. It reduces the picture to
at most eight colours, saves the chart as a vector PDF at its real size, and writes one
STL file per colour into `stencils/`. Leave out the outputs to see what it would make
without writing anything, and add `--json` when another program will read the summary.
`stencilator tiles --help` lists every option.

Pixel art can have one tile per pixel instead:

```sh
target/release/stencilator tiles fixtures/pixel-art/fox.png --pixels --tile 10 \
    --merge 9c3d17=e8772e --stencils stencils/
```

A block of one colour drawn larger, as in pixel art scaled up for the screen, still counts
as one pixel, so this fox makes 22 by 22 tiles. `--merge` gives every tile of one colour to
another, which saves a stencil and a pot of paint, and `--skip` leaves out the stencil of
a colour that needs no painting.

All sizes are in millimetres. They are rounded to whole pixels at the chart's resolution
(300 DPI unless you choose otherwise), so the printed chart and the stencils always line
up. The grid lines become the web that holds each stencil together, so stencils need
lines; below about 0.6 mm the web gets fragile with a 0.4 mm nozzle, and the command
warns you.

### Free-form

Give it an SVG drawing, a picture or some text:

```sh
target/release/stencilator freeform fixtures/svg/badge.svg --3mf badge.3mf
target/release/stencilator freeform --text "PAINT" --text "CLUB" --font-size 40 \
    --stencils plates/
target/release/stencilator freeform photo.jpg --width 250 --colours 4 --3mf photo.3mf
```

An SVG keeps its own size and text its font size, unless you ask for another with
`--width`, `--height` or `--within`; a picture is 200 mm wide unless you say otherwise.
Every plate is the artwork plus a 10 mm border (`--margin`), so all of a design's plates
are the same size and line up by their edges. `--3mf` puts them in one file, side by side,
each named and coloured after its paint, which most slicers open as a single project;
`--stencils` writes one STL per plate instead.

Then it reports what may not print well:

- islands, parts of the plate that fall out because nothing joins them to the rest;
- webs of plate thinner than `--min-web` (0.6 mm), which may break;
- openings narrower than `--min-opening` (0.5 mm), which may close up in printing.

Each comes with where it is, in millimetres from the bottom-left corner of the plate.
The artwork is traced on a grid of 0.1 mm cells (`--cell`), and patches of one paint
smaller than 1 mm² (`--min-patch`) take the paints around them, so a speck in a photo
does not become an opening of its own. Typed text uses the built-in Noto Sans Bold by
default. Try `--font "Allerta Stencil"` for letters with bridges already drawn in, or
give `--font` a font file for text or SVG artwork. Adding a font to an SVG project keeps
its existing fonts unless you supply a file with the same name.

In the web app, pick **Free-form** to use an image, an SVG or typed text. The **Plate**
view highlights islands, thin webs and narrow openings. **Files** offers 3MF, a zip of
STLs, or a single plate. Save a `.stencilator` project to reopen the artwork and all its
settings in the app or on the command line.

### In both modes

- The canvas colour (`--background`, white by default) gets no stencil, since the canvas
  shows through there anyway; `--stencil-canvas-colour` makes one all the same.
  Transparent parts of the artwork take that colour too.
- `--mode greyscale` and `--mode black-white` do what their names say. Artwork with no
  more colours than you ask for, such as pixel art or a logo, keeps its colours exactly.
- `--save design.stencilator` saves a project: the artwork, any fonts given with `--font`,
  and every setting. Give the project instead of a picture to carry on where you left
  off; options you add change only what they name. The project holds the fonts you
  embed, so mind their licences when you share it. Use the `.stencilator` extension
  to open the file in the web app.
- Nothing is written unless everything can be, and no output may replace the input, other
  than a project saved over the one it came from.

## Roadmap

Next are bridges that hold islands in place, so that the middle of an "O" stays put, and
registration marks. A local MCP server follows, so that an AI assistant can prepare
stencils for you. The web app gains each of these as the engine does.

## Development

`make check` runs the engine's checks, as CI does: formatting, Clippy with pedantic
lints treated as errors (for the native build and for WebAssembly), the tests,
`cargo deny` for licences and security advisories, and `cargo machete` for unused
dependencies. It needs the `wasm32-unknown-unknown` target, `cargo-deny` and
`cargo-machete`.

The engine lives in `crates/stencilator-core`. It only computes and never touches files,
the network or the clock, which is what lets the same code run natively and in a browser
with identical results. `crates/stencilator-cli` is the command and
`crates/stencilator-wasm` the browser build; `make parity` checks that the two write the
same bytes, and `make timing` how long the browser build takes over free-form work. `fixtures/` holds reference results, computed independently with NumPy,
Pillow, trimesh and lib3mf, that the tests compare against, and `fonts/` the fonts built
in. `spikes/` holds early experiments with SVG rendering, outline
tracing, 3D meshing and MCP that later work will turn into real code.

The web app in `app/` is written in Svelte and TypeScript. To work on it you also need
Node 22 or newer, pnpm, `wasm-bindgen-cli` 0.2.130 and binaryen. Run `make wasm` once,
then `pnpm install` and `pnpm dev` inside `app/`, where `pnpm check` and `pnpm e2e` run
its own checks.

## Licence

MIT; see [LICENSE](LICENSE). This software is based in part on the work of the
Independent JPEG Group, through the `jpeg-encoder` crate that writes JPEG charts. The
built-in fonts are under the SIL Open Font License 1.1; see [fonts/](fonts/README.md).
