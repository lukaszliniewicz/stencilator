# Marking paint layers in SVG

Give each paint a distinct solid, opaque colour. Use the hex code as the
paint's identifier in the drawing, project and print report. For example:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="70mm" height="45mm" viewBox="0 0 70 45">
  <g id="paint-1d2f5c" fill="#1d2f5c">
    <title>Navy</title>
    <path d="M5 40 L30 12 L58 40 Z"/>
  </g>
  <g id="paint-ef7d2d" fill="#ef7d2d">
    <title>Orange</title>
    <circle cx="52" cy="12" r="8"/>
  </g>
</svg>
```

Group IDs and titles help you label the layers in your drawing program. The
visible SVG fills and strokes determine the paints, so two groups with the
same hex code share a plate. An element hidden behind another contributes no
visible area there. The SVG stack decides what is visible; the project's
**Paint order** sets the sequence for applying the coats.

- Set the SVG's width and height in `mm` for its default physical size.
- Leave unpainted areas transparent. They take the project's canvas colour,
  which by default has no stencil.
- Use Colour mode and a paint limit at least as high as the number of paints
  including the canvas where it shows. Gradients, opacity, masks and embedded
  pictures can mix colours, so Stencilator reduces them to a palette.
- Keep strokes and small details wide enough for tracing and printing. Check
  the plates for narrow openings and unsupported islands before export.
- Convert text to paths if it needs an exact typeface. Stencilator uses its
  bundled fonts and any fonts you supply in the project. External image links
  are ignored; embed images you need.

In the command line or MCP, `#ef7d2d` identifies the orange paint regardless of
the SVG title. RAL Classic matches are approximate suggestions; the source
paint code still identifies the plate.
