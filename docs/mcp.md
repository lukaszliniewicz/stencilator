# Stencilator with an MCP client

Stencilator's MCP server runs on your computer over stdio. Choose an existing
workspace folder for the designs and exports, and keep it in place while the
server runs. Tool paths are relative to that folder. Exports go into existing
folders under new file names. The workspace limits what the tools can open;
it is not an operating-system sandbox. Stencilator processes the artwork
locally. Your MCP client receives reports and the paths to the files it asked
Stencilator to create.

## Set up

Download the matching standalone ZIP or Claude Desktop `.mcpb` from the
[releases page](https://github.com/lukaszliniewicz/stencilator/releases). You
can also build the command with `cargo build --release --locked -p stencilator-cli`
or [build a bundle locally](#build-a-bundle). Use absolute paths for the executable
and workspace when configuring the client. Quote paths that contain spaces.

### Claude Code

```sh
claude mcp add --transport stdio stencilator -- \
    "/absolute/path/to/stencilator" mcp --workspace "/absolute/path/to/workspace"
claude mcp list
```

### Claude Desktop

In **Settings → Desktop app**, select **Manage** next to **Extensions**.
Under **Advanced settings → Extension Developer**, choose **Install
Extension…** and select the `.mcpb` for your operating system and architecture.
Choose an existing workspace folder when asked. To check the connection, open
**Connectors** from the **+** menu by the chat box. See [Claude's installation
guide](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop)
for older Settings layouts.

You can also configure a standalone binary in `claude_desktop_config.json`:

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

### OpenCode V2

From the project where you want to use the server:

```sh
opencode mcp add stencilator -- \
    "/absolute/path/to/stencilator" mcp --workspace "/absolute/path/to/workspace"
opencode mcp list
```

This writes a local server under `mcp.servers` in the project configuration.
Use `/mcps` in OpenCode to see the connection. See the [OpenCode MCP
guide](https://opencode.ai/docs/mcp-servers) for other configuration options.

If OpenCode 2.4.2 says `No MCP servers configured` after saving the entry, you
can register and connect it for the running OpenCode service using its
[experimental API](https://opencode.ai/docs/api):

```sh
opencode api put /api/experimental/mcp/stencilator --data '{
  "config": {
    "type": "local",
    "command": ["/absolute/path/to/stencilator", "mcp", "--workspace", "/absolute/path/to/workspace"]
  }
}'
opencode api post /api/experimental/mcp/stencilator/connect
opencode mcp list
```

### Local Codex

```sh
codex mcp add stencilator -- \
    "/absolute/path/to/stencilator" mcp --workspace "/absolute/path/to/workspace"
codex mcp list
```

Use `/mcp` in Codex to check the live connection. In the ChatGPT desktop
app's local Codex experience, you can also use **Settings → MCP servers →
Add server**, select **STDIO**, enter the same command and arguments, then
save and select **Restart**. These clients share the [local Codex
configuration](https://learn.chatgpt.com/docs/extend/mcp). Set
`default_tools_approval_mode = "writes"` under `[mcp_servers.stencilator]`
in `~/.codex/config.toml` if you want Codex to ask before exports.

ChatGPT web uses [MCP-backed plugins](https://developers.openai.com/plugins/deploy/connect-chatgpt)
with an HTTPS endpoint or a Secure MCP Tunnel. The standalone binary and
Claude Desktop bundle are for local clients; they do not install a ChatGPT
web plugin.

## Check, then export

`check_stencil` builds the design without writing files. It returns the same
dimensions, palette and printability report as the CLI's `--json`. The report
can flag loose islands, fragile webs, narrow openings and a plate or frame that
doesn't fit the chosen bed. A particularly long report is shortened in the
tool result and marked `report_truncated`. Use your client's cancel action
to stop an export. Stencilator checks for cancellation between build stages
and before installing the file. The current stage can take time to finish,
and a file whose installation is already underway may still appear. Closing
the server's input lets it finish pending requests for up to five seconds
before cancelling the remaining work.

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

The [package workflow](https://github.com/lukaszliniewicz/stencilator/blob/main/.github/workflows/package.yml)
builds and tests on Linux x86-64, macOS Apple Silicon and Windows x86-64. It
runs on pull requests that affect the packages and can also be run manually.
It runs both extracted executables before uploading the standalone ZIP,
Claude Desktop `.mcpb` bundle and SHA-256 checksums as build artefacts.
Both archives include the bundled font licences and a third-party licence report.

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
