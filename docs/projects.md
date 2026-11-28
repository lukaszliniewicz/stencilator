# Stencilator project files

A `.stencilator` file is UTF-8 JSON. It carries the artwork, settings of both
modes and paint choices, so it can be reopened in the browser, command line
or MCP. Choose **Download project** in the app or use `--save` on the command
line. If you download an unchanged import, the app can keep the original file.
Any settings it omits use the engine's defaults.

`stencils.bed_mm` holds the usable bed width and depth applied to the project.
The browser's printer profiles, project list and app preferences are kept
separately. Use **Back up or restore…** for a copy of the workspace.
If it needs more than one backup file, download all the numbered files and
import them one at a time, in any order. Select **Also restore app preferences**
in a restore preview to recover the saved theme, defaults and default printer
profile; leave it unchecked to keep this browser’s preferences. Invalid saved
defaults are replaced with engine defaults. An unchanged record already restored
from a backup is skipped. Separate projects and printer profiles are kept even
if their contents match. The dialog lists any projects larger than 32 MiB and
records whose restored form would not fit into a backup file, including projects
linked to an excluded printer profile. The rest of the workspace can still be
downloaded. Open each listed project and choose **Download project** to keep a
separate copy. Keep a separate note of any listed printer profile’s settings.

A minimal project that makes plain plates from typed text:

```json
{
  "stencilator": 1,
  "source": {
    "kind": "text",
    "text": "Hello",
    "font_mm": 30,
    "colour": "#1d2f5c"
  }
}
```

`stencilator` and `source` are required. `source.kind` is `text`, `picture` or
`svg`. An image or drawing has a `name` and base64 `data` containing the source
bytes. A font in `fonts` has a `name` and, unless it is one of the bundled fonts,
base64 `data`. Installed system fonts do not affect the result.

`mode` is `tiles` or `freeform`. Omit it for a new file to use tiles for a
picture and free-form for a drawing or text. `colours`, `tiles`, `freeform`
and `stencils` carry the settings; omitted fields use the defaults. For
example, `freeform.size` can be `"natural"`, `{"width_mm": 100}`,
`{"height_mm": 100}` or `{"within_mm": [100, 80]}`.

Paint lists use hex colours such as `"#1d2f5c"`. `stencils.merges` contains
ordered `[from, to]` pairs and `stencils.skipped` lists paints with no plate.
`freeform.paint_order` puts the listed paints first. Manual support edits are
stored in `freeform.support_edits`: each names a `paint`, a `kind` (`"flat"` or
`"raised"`) and the `original` and/or new `ends` of a support. Each pair of ends
is `[[x1, y1], [x2, y2]]` in integer micrometres from the plate's bottom-left
corner, y up. The check report uses millimetres.

| Format | When it is needed                                                                               |
| ------ | ----------------------------------------------------------------------------------------------- |
| 1      | Plain plates and tiles                                                                          |
| 2      | Flat bridges                                                                                    |
| 3      | A registration frame, with or without flat bridges                                              |
| 4      | Raised connectors, manual supports, saved inactive support dimensions, paint order or finishing |

Storing these settings for the inactive mode still counts towards the format.
Stencilator saves the earliest format that represents the whole project.
`written_by` is informational. The CLI can recognise a project with a different
file extension; use `.stencilator` to open it in the browser.
