# Stencilator

Stencilator turns a picture into painting stencils you can 3D-print. You give it an image
and the size of your canvas, and it works out a grid of square tiles, reduces the picture
to a handful of colours, and writes a chart to paint from along with one stencil per
colour. Lay a stencil on the canvas, paint through the holes, move on to the next colour,
and the picture builds up tile by tile.

It is written in Rust so that one engine can run on the command line, in a web browser
and, later, behind an AI assistant, giving the same result everywhere.

## Status

Early, but usable from the command line. Tile mode, the square grid described above, is
finished. The rest is still to come (see the [roadmap](#roadmap)), and there are no
prebuilt releases yet.

## Trying it

You need Rust 1.88 or newer. Build the command with

```sh
cargo build --release -p stencilator-cli
```

and run it on an image:

```sh
target/release/stencilator build logo.png --canvas 300x200 --tile 5 --line 0.8 \
    --colours 8 --chart chart.pdf --stencils stencils/
```

This finds the largest grid of 5 mm tiles, with 0.8 mm lines between them, that fits on
a 300 by 200 mm canvas while keeping the picture's proportions. It reduces the picture to
at most eight colours, saves the chart as a vector PDF at its real size, and writes one
STL file per colour into `stencils/`. Leave out `--chart`, `--tiles` and `--stencils` to
see what it would make without writing anything, and add `--json` when another program
will read the summary. `stencilator build --help` lists every option.

All sizes are in millimetres. They are rounded to whole pixels at the chart's resolution
(300 DPI unless you choose otherwise), so the printed chart and the stencils always line
up.

A few things worth knowing:

- The grid lines become the web that holds each stencil together, so stencils need
  lines. Below about 0.6 mm the web gets fragile with a 0.4 mm nozzle, and the command
  warns you.
- Transparent parts of the image take the background colour (`--background`, white by
  default).
- `--mode greyscale` and `--mode black-white` do what their names say. An image with no
  more colours than you ask for, such as pixel art, keeps its colours exactly.

## Roadmap

Free-form mode comes next: stencils whose openings follow the shapes in the artwork
rather than a grid, made from SVG files, images or typed text. That brings the problem
every stencil maker knows, the middle of an "O" falling out, so connectors that hold such
islands in place follow, together with a frame and registration marks so the plates line
up. After that comes a web app that runs entirely in your browser, with nothing uploaded
anywhere, and a local MCP server so that an AI assistant can prepare stencils for you.

## Development

`make check` runs everything CI runs: formatting, Clippy with pedantic lints treated as
errors (for the native build and for WebAssembly), the tests, `cargo deny` for licences
and security advisories, and `cargo machete` for unused dependencies. It needs the
`wasm32-unknown-unknown` target, `cargo-deny` and `cargo-machete`.

The engine lives in `crates/stencilator-core`. It only computes and never touches files,
the network or the clock, which is what lets the same code run natively and in a browser
with identical results. `crates/stencilator-cli` is the command, and
`crates/stencilator-wasm` will become the browser build. `fixtures/` holds reference
results, computed independently, that the tests compare against. `spikes/` holds early
experiments with SVG rendering, outline tracing, 3D meshing and MCP that later work will
turn into real code.

## Licence

MIT; see [LICENSE](LICENSE). This software is based in part on the work of the
Independent JPEG Group, through the `jpeg-encoder` crate that writes JPEG charts.
