# Stencilator project files

A `.stencilator` file is UTF-8 JSON. It carries the artwork, settings of both
modes and paint choices, so it can be reopened in the browser, command line
or MCP. Save a project with the app or `--save` to see the complete settings
used for your design.

A minimal project that makes plain plates from typed text:

```json
{
  "stencilator": 1,
  "source": {"kind": "text", "text": "Hello", "font_mm": 30, "colour": "#1d2f5c"}
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

| Format | When it is needed |
| --- | --- |
| 1 | Plain plates and tiles |
| 2 | Flat bridges |
| 3 | A registration frame, with or without flat bridges |
| 4 | Raised connectors, manual supports, saved inactive support dimensions, paint order or finishing |

Storing these settings for the inactive mode still counts towards the format.
Stencilator saves the earliest format that represents the whole project.
`written_by` is informational. The CLI can recognise a project with a different
file extension; use `.stencilator` to open it in the browser.
