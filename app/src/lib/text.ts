/**
 * Every word the interface shows, in British English. Anything with a number in it is a
 * function, so that another language can form its own numbers and plurals; a
 * translation provides an object of the same shape.
 */
const LOCALE = 'en-GB';

const formats = new Map<number, Intl.NumberFormat>();

/** A number with grouping and at most `digits` decimals, trailing zeros dropped. */
export function num(value: number, digits = 0): string {
  let format = formats.get(digits);
  if (!format) {
    format = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: digits });
    formats.set(digits, format);
  }
  return format.format(value);
}

const plural = (n: number, one: string, other: string): string =>
  `${num(n)} ${n === 1 ? one : other}`;
const size = (w: string, h: string, unit: string): string => `${w} × ${h} ${unit}`;
const hex = (colour: string): string => colour.toUpperCase();
const list = new Intl.ListFormat(LOCALE, { type: 'conjunction' });

export const text = {
  wordmark: 'Stencilator',
  privacy: 'Everything happens in this browser. Nothing is uploaded.',
  engine: (version: string) => `Engine ${version}`,

  theme: { label: 'Theme', light: 'Light', dark: 'Dark', auto: 'Auto' },

  picture: {
    heading: 'Picture',
    none: 'No picture yet',
    open: 'Open picture',
    replace: 'Replace',
    alt: 'The picture you opened',
    pixels: (w: number, h: number) => size(num(w), num(h), 'px'),
    fileLabel: 'Picture file'
  },

  how: {
    heading: 'How it works',
    steps: [
      {
        title: 'Picture.',
        body: 'PNG, JPEG or WebP. Pixel art works best, a tile per pixel; photos are reduced to a few paints.'
      },
      {
        title: 'Chart.',
        body: 'Every tile gets one paint. The chart prints at real size, to paint from.'
      },
      {
        title: 'Stencils.',
        body: 'One per paint, with a hole over each of its tiles. Lay it on the canvas and paint through.'
      }
    ]
  },

  paints: {
    heading: 'Paints',
    order: (n: number) => `${num(n)} · darkest first`,
    item: (index: number, colour: string, tiles: number) =>
      `Paint ${num(index + 1)}, ${hex(colour)}, ${plural(tiles, 'tile', 'tiles')}`,
    stencil: (index: number, colour: string) =>
      `Stencil for paint ${num(index + 1)}, ${hex(colour)}`,
    stencilNote:
      'Ticked paints get a stencil. The canvas colour needs none, so it starts unticked.',
    mergeInto: (colour: string) => `Merge ${hex(colour)} into`,
    mergeTarget: (fromIndex: number, from: string, toIndex: number, to: string) =>
      `Merge paint ${num(fromIndex + 1)}, ${hex(from)}, into paint ${num(toIndex + 1)}, ${hex(to)}`,
    merged: 'Merged',
    into: 'into',
    unmerge: (from: string, to: string) => `Undo merging ${hex(from)} into ${hex(to)}`
  },

  stencil: {
    name: (index: number) => `Stencil ${num(index + 1)}`,
    holes: (n: number) => plural(n, 'hole', 'holes'),
    none: 'Unticked, so no stencil',
    label: (index: number, colour: string, holes: number) =>
      `Stencil ${num(index + 1)} for ${hex(colour)}, with ${plural(holes, 'hole', 'holes')}`
  },

  toolbar: {
    undo: 'Undo (Ctrl+Z)',
    redo: 'Redo (Ctrl+Shift+Z)',
    view: 'View',
    views: { picture: 'Picture', chart: 'Chart', both: 'Side by side', stencil: 'Stencil' },
    files: 'Files',
    back: 'Back to the chart'
  },

  bench: {
    canvas: (w: number, h: number) => `Your canvas · ${size(num(w, 1), num(h, 1), 'mm')}`,
    perPixel: 'A tile per pixel',
    tiles: (mm: number) => `Tiles of ${num(mm, 2)} mm`,
    drop: 'Drop a picture.',
    dropBody:
      'PNG, JPEG or WebP. Pixel art gets a tile per pixel; a larger picture is fitted into your canvas. You can also paste one.',
    choose: 'Choose a picture',
    example: 'Try the example',
    dropping: 'Drop the picture to open it',
    chart: (w: number, h: number) => `Chart · ${size(num(w, 1), num(h, 1), 'mm')}`,
    onCanvas: (w: number, h: number) => `On a ${size(num(w, 1), num(h, 1), 'mm')} canvas`,
    fromPixels: (x: number, y: number) => `${size(num(x), num(y), 'pixels')}, a tile each`,
    chartLabel: (x: number, y: number) => `The chart, ${num(x)} by ${num(y)} tiles`,
    stencilCaption: (index: number, colour: string) => `Stencil ${num(index + 1)} · ${hex(colour)}`,
    status: (x: number, y: number, paints: number, tileMm: number, lineMm: number) =>
      [
        `${num(x)} × ${num(y)} tiles`,
        `${num(x * y)} in all`,
        plural(paints, 'paint', 'paints'),
        `tile ${num(tileMm, 3)} mm`,
        `line ${num(lineMm, 3)} mm`
      ].join(' · ')
  },

  settings: {
    label: 'Settings',
    canvas: 'Canvas',
    sizing: 'Tiles',
    sizings: { pixels: 'Per pixel', canvas: 'Fit to canvas' },
    width: 'Width (mm)',
    height: 'Height (mm)',
    canvasNote: 'The grid keeps the picture’s proportions and stays inside the canvas.',
    pixelNote: (x: number, y: number) =>
      `The picture reads as ${size(num(x), num(y), 'pixels')}; each becomes one tile, so the tile size sets the canvas.`,
    pixelNoteEmpty: 'Each pixel of the picture becomes one tile, so the tile size sets the canvas.',
    grid: 'Grid',
    tile: 'Tile (mm)',
    line: 'Line (mm)',
    lineColour: 'Line colour',
    rounded: (dpi: number, tileMm: number, lineMm: number) =>
      `Rounded to whole pixels at ${num(dpi)} DPI: ${num(tileMm, 3)} mm tiles, ${num(lineMm, 3)} mm lines.`,
    noLines: 'Without grid lines there are no stencils: their holes would run together.',
    thinLines: (minMm: number) => `Lines under ${num(minMm, 2)} mm make fragile stencils.`,
    paints: 'Paints',
    mode: 'Colour mode',
    modes: { colour: 'Colour', greyscale: 'Grey', 'black-white': 'B & W' },
    modeNames: { colour: 'Colour', greyscale: 'Greyscale', 'black-white': 'Black and white' },
    colours: 'Most paints',
    twoPaints: 'Black and white always uses two paints.',
    background: 'Canvas colour',
    stencils: 'Stencils',
    thickness: 'Thickness (mm)',
    chartFile: 'Chart file',
    dpi: 'Resolution (DPI)'
  },

  files: {
    heading: 'Files',
    summary: (name: string, x: number, y: number, paints: number) =>
      `${name} · ${num(x)} × ${num(y)} tiles · ${plural(paints, 'paint', 'paints')}`,
    chart: 'The chart to paint from',
    style: 'Chart style',
    withLines: 'With grid lines',
    tilesOnly: 'Tiles only',
    pdf: 'PDF',
    best: 'Best for printing',
    pdfBody: (w: number, h: number) => `Vector, at exactly ${size(num(w, 1), num(h, 1), 'mm')}.`,
    png: 'PNG',
    pngBody: (w: number, h: number, dpi: number) =>
      `${size(num(w), num(h), 'px')} at ${num(dpi)} DPI, without loss.`,
    jpeg: 'JPEG',
    jpegBody: 'The same pixels, for apps that take nothing else.',
    save: (format: string) => `Save ${format}`,
    saving: 'Saving…',
    stencils: 'Stencils, one per ticked paint',
    all: (n: number) => `All ${plural(n, 'stencil', 'stencils')}`,
    skipped: (colours: string[]) =>
      `No stencil for ${list.format(colours.map(hex))}: tick a paint in the list to add one.`,
    noneTicked: 'Every paint is unticked, so there are no stencils. Tick the paints that need one.',
    allBody: (thicknessMm: number) =>
      `STL files in one zip, each ${num(thicknessMm, 2)} mm thick, with the holes cut right through.`,
    saveAll: 'Save all stencils',
    saveStl: 'Save STL',
    saveOne: (index: number, colour: string) => `Save stencil ${num(index + 1)}, ${hex(colour)}`,
    none: 'There are no stencils without grid lines: their holes would run together. Set a line width above 0 mm.',
    beforeYouPrint: 'Before you print',
    actualSize: 'Print the chart PDF at actual size (100%), not “fit to page”.',
    flat: (lineMm: number) =>
      `Print the stencils flat. The web between the holes is ${num(lineMm, 3)} mm wide; check in your slicer’s preview that it is filled.`,
    tooLarge: (w: number, h: number) =>
      `Each stencil is ${size(num(w), num(h), 'mm')}, larger than most printer beds. Until stencils can be split into parts, choose a canvas that fits your printer.`
  },

  errors: {
    notAPicture: 'That is not a picture. Open a PNG, JPEG or WebP file.',
    example: 'The example could not be loaded.'
  }
};
