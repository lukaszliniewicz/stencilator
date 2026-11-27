# Stencilator with an MCP client

Stencilator's MCP server runs on your computer over stdio. Choose a folder for
the designs and exports you want the assistant to work with. Paths in tool
requests are relative to that folder, and the server follows links only within
it. Export creates a new file in an existing folder and refuses to replace a
file, including the source. The server does its processing locally; your MCP
client receives the check results and export paths.

## Set up

Build the command with `cargo build --release --locked -p stencilator-cli`, or
use a matching prebuilt binary. Use absolute paths for the executable and
workspace when configuring the client.

In Claude Code:

```sh
claude mcp add --transport stdio stencilator -- \
    /absolute/path/to/stencilator mcp --workspace /absolute/path/to/workspace
claude mcp list
```

For Claude Desktop, open the `.mcpb` for your operating system and architecture
and choose an existing workspace folder when asked. This installs the binary
with its tool configuration. You can also configure a binary by hand under
`mcpServers`:

```json
{
  "mcpServers": {
    "stencilator": {
      "command": "/absolute/path/to/stencilator",
      "args": ["mcp", "--workspace", "/absolute/path/to/workspace"]
    }
  }
}
```

On Windows the command ends in `stencilator.exe`; double each backslash in JSON
paths.

## Check, then export

`check_stencil` builds the design without writing files. It returns the same
dimensions, palette and printability report as the CLI's `--json`. The report
can flag loose islands, fragile webs, narrow openings and a plate or frame that
doesn't fit the chosen bed. A particularly long report is shortened in the
tool result and marked `report_truncated`.

For example, these are the arguments to `check_stencil` for a 30 mm-wide ring
of text with flat bridges and a keyed frame:

```json
{
  "design": {
    "source": {"kind": "text", "text": "O"},
    "settings": {
      "freeform": {
        "size": {"width_mm": 30},
        "bridge_width_mm": 1.2,
        "registration_clearance_mm": 0.3
      },
      "stencils": {"bed_mm": [180, 180]}
    }
  }
}
```

Use the same `design` with `export_stencil`, plus a new `"path"`, for example
`"ring.3mf"`. Export once per file:

| Extension | What it contains |
| --- | --- |
| `.stencilator` | Editable project, with its artwork embedded, for the app or CLI |
| `.json` | Full check report, including any locations omitted from the tool result |
| `.zip` | One STL per paint, plus the registration frame if present |
| `.3mf` | Free-form print set with named, coloured plates and any frame |
| `.pdf`, `.png`, `.jpg`, `.jpeg` | Tile-mode paint chart with grid lines |

For a saved design, use `"source": {"kind": "project", "path": "ring.stencilator"}`.
For a PNG, JPEG, WebP, SVG or `.stencilator` file, use `kind: "file"`. A picture
starts in tile mode and a drawing or text in free-form mode. To make free-form
plates from a picture set `"settings": {"mode": "freeform"}`. To make tiles
follow pixel art use `"settings": {"tiles": {"sizing": "pixels"}}`.

For SVG supplied directly, use `kind: "svg"` and put the complete drawing in
`svg`. Use [solid paint colours](svg-paints.md) to keep the intended palette.
Source files and editable projects can be up to 128 MiB, inline SVG up to
1 MiB and typed text up to 16 KiB. Text can specify `font_mm`, `colour` and
`font` (`noto-sans-bold` or `allerta-stencil`).

Settings use the [project file's](projects.md) names and units. Omitted fields
keep their saved values or the default for new artwork. Arrays such as `skipped`
and `paint_order` replace the whole list if supplied. Use `null` to clear an
optional value such as `bridge_width_mm` or `registration_clearance_mm`. To
switch from flat to raised supports, clear the flat width and set
`"raised_connectors": {}` for the trial dimensions. To keep a custom flat width
for later when doing this, set `inactive_bridge_width_mm` to that width.

## Build a bundle

The [package workflow](https://github.com/lukaszliniewicz/stencilator/blob/main/.github/workflows/package.yml) builds and tests on
Linux x86-64, macOS Apple Silicon and Windows x86-64. It makes a standalone ZIP,
an MCP bundle and SHA-256 checksums as build artifacts. Both archives include
the bundled font licences and a third-party licence report.

To package a locally built Linux binary, run this from the repository root
with Python 3.11 or later and `cargo-about` 0.9.3:

```sh
cargo build --release --locked -p stencilator-cli
cargo about generate --locked --fail --manifest-path crates/stencilator-cli/Cargo.toml \
    --config tools/package-licenses.toml -o target/third-party-licenses.txt tools/package-licenses.hbs
python3 tools/package_release.py --target x86_64-unknown-linux-gnu \
    --binary target/release/stencilator --notices target/third-party-licenses.txt \
    --output target/packages
```
