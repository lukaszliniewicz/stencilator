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
const date = new Intl.DateTimeFormat(LOCALE, { dateStyle: 'medium', timeStyle: 'short' });
const bedSize = (w: number, h: number): string => size(num(w, 4), num(h, 4), 'mm');
const delta = (mm: number): string => (mm > 0 && mm < 0.001 ? '<0.001' : num(mm, 3));

export const text = {
  wordmark: 'Stencilator',
  privacy: 'Everything happens in this browser. Nothing is uploaded.',
  engine: (version: string) => `Engine ${version}`,

  theme: { label: 'Theme', light: 'Light', dark: 'Dark', auto: 'Auto' },

  picture: {
    heading: 'Picture',
    replace: 'Replace',
    alt: 'The picture you opened',
    pixels: (w: number, h: number) => size(num(w), num(h), 'px'),
    fileLabel: 'Picture file'
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
    sizing: 'Tiles',
    sizings: { pixels: 'Per pixel', canvas: 'Fit to canvas' },
    width: 'Width (mm)',
    height: 'Height (mm)',
    pixelNote: (x: number, y: number) =>
      `The picture reads as ${size(num(x), num(y), 'pixels')}; each becomes one tile, so the tile size sets the canvas.`,
    pixelNoteEmpty: 'Each pixel of the picture becomes one tile, so the tile size sets the canvas.',
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
      `Print the stencils flat. The web between the holes is ${num(lineMm, 3)} mm wide; check in your slicer’s preview that it is filled.`
  },

  project: {
    mode: 'Stencil mode',
    modes: { tiles: 'Tiles', freeform: 'Free-form' },
    tilesNote: 'A square grid, best for pixel art.',
    freeformNote: 'Openings that follow the artwork.',
    tilesNeedPicture: 'Tiles are available for PNG, JPEG and WebP pictures.',
    open: 'Import a project…',
    fileLabel: 'Stencilator project file',
    heading: 'Stencilator project',
    save: 'Download project',
    body: 'Keep the artwork, fonts, settings and paint choices together. Reopen this file here or use it on the command line.',
    opening: 'Opening…',
    openBusy: 'Wait for the current file to finish opening or saving, then try again.'
  },

  workspace: {
    newProject: 'New project',
    recent: 'Recent projects',
    projects: 'Projects in this browser',
    projectActions: 'Project actions',
    search: 'Find a project by name',
    noProjects: 'No saved projects here yet.',
    noMatches: 'No projects match that name.',
    chooserTitle: 'What are you making?',
    chooserBody:
      'Choose how the stencil should follow your artwork. You can change its size and paints once you start.',
    presetLabel: 'Starting preset',
    presets: {
      pixel: {
        title: 'Pixel art',
        body: 'One tile per artwork pixel. Bigger tiles make a bigger stencil.'
      },
      tile: {
        title: 'Tile grid',
        body: 'Set a maximum size. The picture is sampled onto the grid that fits.'
      },
      freeform: { title: 'Free-form', body: 'Cut along the shapes in a drawing, picture or text.' }
    },
    samples: { pixel: 'Fox', tile: 'Fox tile chart', freeform: 'Paint Club badge' },
    tryFox: 'Try the fox',
    choose: 'Choose artwork…',
    imageTypes: 'PNG, JPEG or WebP. You can also drop or paste a picture.',
    drawTypes: 'SVG, PNG, JPEG or WebP. You can also drop or paste artwork.',
    textTitle: 'Text stencil',
    guide: 'First-stencil guide',
    back: 'Back to project',
    panels: { canvas: 'Canvas', artwork: 'Artwork & paints', settings: 'Settings' },
    panelLabel: 'Workspace panels',
    nameLabel: 'Project name',
    rename: 'Rename…',
    renameTitle: 'Rename project',
    duplicate: 'Duplicate',
    remove: 'Remove from this browser…',
    removeTitle: 'Remove project',
    removeConfirm: (name: string) =>
      `Remove “${name}” from this browser? Downloaded files are unchanged.`,
    removeButton: 'Remove project',
    cancel: 'Cancel',
    close: 'Close',
    saveName: 'Save name',
    copySuffix: (n: number) => (n === 1 ? ' (copy)' : ` (copy ${num(n)})`),
    edited: (mode: string, when: number) => `${mode} · ${date.format(when)}`,
    startup: 'Opening your workspace…',
    sample: 'Sample',
    keep: 'Keep project',
    saving: 'Saving…',
    saved: 'Saved in this browser',
    failed: 'Could not save in this browser',
    failedHelp: 'Your draft is still open. Retry the save or download a project file to keep it.',
    conflict: 'This project changed in another tab',
    conflictHelp: 'Your edits are still here. Keep them as a copy or load the newer save.',
    removed: 'This project was removed in another tab',
    removedHelp: 'You can keep this draft as a separate project or download it.',
    loadNewer: 'Load the newer version',
    keepCopy: 'Keep mine as a copy',
    retry: 'Retry save',
    upgrade: 'This tab needs to reload',
    upgradeHelp:
      'Download your draft if it has unsaved changes, then reload to use the current workspace.',
    oldViewHelp: 'The 3D view could not load. Download your draft if needed, then reload the app.',
    reload: 'Reload app',
    local:
      'Saved projects stay in this browser and can be cleared with its site data. Download a project or a workspace backup to keep it elsewhere.',
    leaveUnsaved: (name: string) =>
      `“${name}” could not be saved here. Leave it without saving these changes?`,
    broughtIn: (name: string) => `Open “${name}”`,
    incomingHelp: (name: string) =>
      `Start a separate project, or replace the artwork in “${name}”.`,
    separate: 'Start new project',
    replaceNow: 'Replace artwork',
    replaceText: (name: string) => `Replace the artwork in “${name}” with text?`,
    errors: {
      conflict: 'This item changed in another tab. Reopen it to see the current version.',
      upgrade: 'The workspace was opened by a newer version. Reload this tab to use it.',
      invalid: 'The backup is damaged or has missing information. No changes were made.',
      unsupported: 'This backup uses a format this version cannot read. No changes were made.',
      'too-large':
        'This workspace is too large for a single backup. Keep the individual project files.',
      full: 'The browser ran out of storage space.',
      unavailable: 'Browser storage is unavailable.'
    }
  },

  backup: {
    open: 'Back up or restore…',
    title: 'Back up or restore',
    exportTitle: 'Back up this browser',
    exportBody: 'One file with your saved projects, printer profiles and app preferences.',
    export: 'Download backup',
    exportSaved: 'Download saved projects only',
    excluded: (name: string) => `Unsaved changes in “${name}” will be left out of the backup.`,
    importTitle: 'Import a backup',
    importBody: 'Review the contents before adding them. Your existing work stays here.',
    choose: 'Choose backup…',
    input: 'Workspace backup file',
    preview: 'Preview import',
    import: 'Import backup',
    back: 'Back to backup options',
    preferences: 'Also restore app preferences',
    preferencesHint: 'Includes theme, defaults and the default printer profile.',
    previewProjects: 'Projects',
    previewProfiles: 'Printer profiles',
    count: (projects: number, profiles: number) =>
      `${plural(projects, 'project', 'projects')} · ${plural(profiles, 'printer profile', 'printer profiles')}`,
    actions: { add: 'Add', skip: 'Skip: already here', copy: 'Import as a copy' },
    updated:
      'This workspace changed while you were reviewing. Check the updated preview before importing.',
    checking: 'Checking backup…',
    blank: 'This backup has no projects or printer profiles.',
    ready: 'Backup downloaded.',
    imported: 'Workspace imported.'
  },

  printer: {
    title: 'Printer setup',
    profiles: 'Printer profiles',
    manage: 'Manage printer profiles…',
    profile: 'Printer profile',
    custom: 'Custom bed',
    customHelp: 'Enter the printable width and depth from your slicer.',
    projectSize: 'Bed in this project',
    change: 'Change bed…',
    width: 'Bed width (mm)',
    depth: 'Bed depth (mm)',
    apply: 'Apply to project',
    applyUpdate: 'Apply update',
    updated: (name: string, oldW: number, oldD: number, newW: number, newD: number) =>
      `This project uses ${bedSize(oldW, oldD)}. ${name} now uses ${bedSize(newW, newD)}.`,
    newProfile: 'New profile',
    savedProfiles: 'Saved profiles',
    name: 'Profile name',
    optional: 'Optional slicer settings',
    line: 'Extrusion line width (mm)',
    layer: 'Layer height (mm)',
    nozzle: 'Nozzle diameter (mm)',
    reference:
      'Extrusion line width is the width of a line of plastic. The stencil web is the material between openings. These settings are for reference.',
    invalid:
      'Give the profile a name, a bed between 50 and 2,000 mm on each side, and positive values for any slicer settings.',
    save: 'Save profile',
    remove: 'Remove profile…',
    removeConfirm: (name: string) =>
      `Remove “${name}”? Its projects will keep their current bed dimensions.`,
    defaultProfile: 'Default for new projects',
    makeDefault: 'Use as default',
    clearDefault: 'Clear default',
    savedBedNote:
      'Saving a profile leaves the bed in your projects as it was. Apply the profile to use its new size.',
    back: 'Back to printer setup'
  },

  bed: {
    title: 'Print bed',
    part: 'Part',
    partSelect: 'Part on the bed',
    size: bedSize,
    height: (z: number) => `${num(z, 3)} mm tall`,
    fit: {
      'as-drawn': 'Fits as drawn',
      'quarter-turn': 'Fits after a quarter-turn',
      'too-large': 'Too large for this bed'
    },
    fitNote:
      'Fit is checked one part at a time, laid flat as drawn or turned 90°. Check height and room for a brim in your slicer.',
    rotated: 'The preview turns the part 90°. The export keeps its original orientation.',
    overhang: 'Amber marks the part outside the bed.',
    clearance: (x: number, y: number) => `Room left: ${delta(x)} mm across, ${delta(y)} mm deep.`,
    overflow: (x: number, y: number) =>
      list.format(
        [x > 0 ? `${delta(x)} mm wider` : '', y > 0 ? `${delta(y)} mm deeper` : ''].filter(Boolean)
      ),
    asDrawnOver: (overflow: string) => `As drawn: ${overflow}.`,
    turnedOver: (overflow: string) => `Turned: ${overflow}.`,
    resize: 'Change size…',
    allFit: 'All parts fit as drawn.',
    checking: 'Checking the latest parts against this bed…',
    none: 'There are no parts to fit yet.',
    unavailable:
      'A print-bed preview needs a finished part. Dimensions and downloads are in Files.',
    footprint: 'Footprint only · 10 mm grid',
    orientation: 'View angle',
    angles: { angled: 'Angled', top: 'Top' },
    frame: 'Zoom to',
    fitBed: 'Fit bed',
    fitPart: 'Fit part',
    drawing: (part: string, width: number, depth: number, fit: string) =>
      `${part} on a ${bedSize(width, depth)} bed. ${fit}.`,
    placement3mf: 'The 3MF opens with the parts side by side. Arrange them in your slicer.',
    findings: 'Print-bed fit',
    tilePart: (index: number, colour: string) => `Stencil ${num(index + 1)} · ${hex(colour)}`
  },

  viewport: {
    label: 'Canvas view',
    zoomIn: 'Zoom in',
    zoomOut: 'Zoom out',
    reset: 'Reset view',
    help: 'Drag to pan, scroll or pinch to zoom. When focused, use the arrow keys, plus, minus or 0 to reset.'
  },

  help: {
    artwork:
      'Tiles make a grid of square openings. Free-form follows the shapes; the plate also includes a margin and, with a frame, a key.',
    sizing: 'Tile sizing',
    pixels:
      'One tile per artwork pixel. Tile side and line width set the physical size without changing the grid.',
    canvas:
      'The tile side and line width determine how many tiles fit. The grid may combine several pixels into one tile and lose detail.',
    supports:
      'Loose pieces, like the centre of an O, need something to hold them. Flat bridges touch the work; raised beams leave room for spray.',
    frame:
      'The off-centre key seats every plate facing the same way. Keep the frame in place as you change plates.',
    web: 'The line between tile openings is the material holding the stencil together. Check it is wide enough to print.',
    tileDetails: 'Tile openings & lines',
    plateDetails: 'Plate & registration',
    finishDetails: 'Detail & finishing',
    chartDetails: 'Chart resolution',
    resulting: (w: number, h: number) => `Resulting size: ${bedSize(w, h)}`,
    about: (label: string) => `About ${label.toLocaleLowerCase()}`
  },

  freeform: {
    artwork: 'Artwork',
    artworkAlt: 'The artwork at its original colours',
    none: 'No artwork yet',
    open: 'Open artwork',
    dropping: 'Drop artwork or a project to open it',
    example: 'Try the badge',
    type: 'Type text',
    typed: 'Text',
    initialText: 'YOUR TEXT',
    font: 'Font',
    fonts: {
      'NotoSans-Bold.ttf': 'Noto Sans Bold',
      'AllertaStencil-Regular.ttf': 'Allerta Stencil'
    },
    fontLabel: 'Font file',
    addFont: 'Add font file',
    clearFonts: 'Use bundled fonts',
    fontSize: 'Font size (mm)',
    textColour: 'Text colour',
    stencilFontTip:
      'Allerta Stencil has bridges built into its letters to keep their centres in place.',
    tryStencilFont: 'Switch to Allerta Stencil',
    views: {
      picture: 'Artwork',
      chart: 'Paints',
      both: 'Side by side',
      stencil: 'Plate',
      result: 'Painted result'
    },
    back: 'Back to the artwork',
    finding: 'Finding paints…',
    building: 'Building and checking plates…',
    previewing: 'Drawing the plate…',
    painted: 'Artwork reduced to its paints',
    area: (mm2: number) => `${num(mm2, 1)} mm²`,
    paint: (i: number, colour: string, area: number) =>
      `Paint ${num(i + 1)}, ${hex(colour)}, ${num(area, 1)} square millimetres`,
    artworkSize: (w: number, h: number) => size(num(w, 2), num(h, 2), 'mm'),
    summary: (w: number, h: number, paints: number, plates: number, cell: number) =>
      [
        size(num(w, 2), num(h, 2), 'mm'),
        plural(paints, 'paint', 'paints'),
        plural(plates, 'plate', 'plates'),
        `${num(cell, 3)} mm cells`
      ].join(' · '),
    plate: 'Plate',
    plates: 'Plates',
    plateFor: (i: number, colour: string) => `Plate ${num(i + 1)} · ${hex(colour)}`,
    plateCheck: 'Plate check',
    noWarnings: 'No printability warnings.',
    nothingSelected: 'This paint is unticked, so it has no plate.',
    noneTicked: 'Tick at least one paint to make a plate.',
    size: 'Artwork size',
    sizes: { natural: 'Own size', width: 'Width', height: 'Height', within: 'Fit in' },
    naturalSize: 'Use the SVG’s dimensions or the size of the letters you set.',
    detail: 'Detail',
    cell: 'Cell size (mm)',
    patch: 'Smallest patch (mm²)',
    detailNote:
      'Smaller cells follow edges more closely. Patches below the area you set take the paints around them. Set it to 0 to keep every patch.',
    margin: 'Margin (mm)',
    bed: 'Print bed',
    bedWidth: 'Bed width (mm)',
    bedDepth: 'Bed depth (mm)',
    checks: 'Checks',
    minWeb: 'Thinnest web (mm)',
    minOpening: 'Narrowest opening (mm)',
    highlights: 'Highlight printability issues',
    islandLayer: 'Islands',
    webLayer: 'Thin webs',
    openingLayer: 'Narrow openings',
    islands: (n: number) => plural(n, 'loose island', 'loose islands'),
    singleBridgeIslands: (n: number) =>
      `${plural(n, 'island', 'islands')} with one bridge towards the rim`,
    flatBridgesAdded: (n: number, area: number) =>
      `${plural(n, 'flat bridge', 'flat bridges')} · ${num(area, 1)} mm² masked`,
    thinWebs: (n: number) => plural(n, 'thin web', 'thin webs'),
    narrowOpenings: (n: number) => plural(n, 'narrow opening', 'narrow openings'),
    openings: (n: number) => plural(n, 'opening', 'openings'),
    includedFrame: (w: number, h: number) =>
      `Both print files include the registration frame (${size(num(w, 2), num(h, 2), 'mm')}).`,
    partSize: (w: number, h: number, z: number) => `${num(w, 2)} × ${num(h, 2)} × ${num(z, 2)} mm`,
    threeMf: '3MF',
    threeMfBody: 'Every plate in one slicer project, named and coloured by paint.',
    zip: 'STL zip',
    zipBody: 'One STL per ticked paint, in a single zip.',
    savePlate: (i: number, colour: string) => `Save plate ${num(i + 1)}, ${hex(colour)}`,
    allPlates: (n: number) => `All ${plural(n, 'plate', 'plates')}`,
    parts: 'Individual parts',
    forPrinting: 'For the slicer',
    inspect: 'Check each plate at actual size in your slicer. Each plate goes on the bed flat.',
    supportIslands:
      'Loose islands fall out after printing. Add supports, change the artwork or use a stencil font.',
    inspectThinWebs:
      'Check that thin webs are filled in the slice. They can break when you lift the plate.',
    inspectNarrowOpenings:
      'Narrow openings may close up in the slice or let too little paint through.'
  },

  supports: {
    heading: 'Island supports',
    kinds: { none: 'None', flat: 'Flat', raised: 'Raised' },
    note: 'Flat bridges leave unpainted lines. Raised connectors leave the work-face openings clear.',
    trial: 'Trial dimensions. Print and spray a sample to check strength and coverage.',
    width: 'Width (mm)',
    clearance: 'Above work (mm)',
    beamSettings: 'Beam settings',
    beam: 'Thickness (mm)',
    warn: 'Warn beyond (mm)',
    onPlate: (paint: number) => `Supports on plate ${num(paint + 1)}`,
    item: (i: number, raised: boolean) => `${num(i + 1)} · ${raised ? 'Connector' : 'Flat bridge'}`,
    span: (value: number) => `${num(value, 2)} mm span`,
    none: 'No supports placed.',
    add: 'Add support',
    move: 'Move ends',
    remove: 'Remove support',
    oldRemoval: 'Old removal',
    discardRemoval: 'Discard old removal',
    restore: 'Restore suggestions',
    placement:
      'Click two ends in Top view, or enter their positions from the bottom-left of the plate.',
    placing: (end: number) => `Place end ${num(end + 1)} on solid material`,
    coordinate: (end: number, axis: 'X' | 'Y') => `End ${num(end + 1)} ${axis} (mm)`,
    apply: 'Apply support',
    cancel: 'Cancel',
    checking: 'Checking landings…',
    bothEnds: 'Enter both ends within the plate.',
    tooMany:
      'This project already has 512 support edits. Remove a manual addition or restore suggestions.',
    issues: {
      landings: 'Each end needs room for its full landing on solid material.',
      span: 'The support must cross one opening between two separate pieces of the plate.',
      conflict: 'This support would touch another manual support.',
      suggestion:
        'This saved removal no longer matches a suggested support. Review the new suggestions.'
    },
    count: (n: number) => plural(n, 'raised connector', 'raised connectors'),
    single: (n: number) => `${plural(n, 'island', 'islands')} with one connector towards the rim`,
    long: (n: number) => plural(n, 'long free span', 'long free spans'),
    unplaced: (n: number) => plural(n, 'unplaced support edit', 'unplaced support edits'),
    unmatched: (n: number) =>
      `${plural(n, 'saved removal no longer matches', 'saved removals no longer match')} the suggestions`,
    missingPaints: (n: number) =>
      `${plural(n, 'saved support edit', 'saved support edits')} for unticked or missing paints.`
  },

  finishing: {
    heading: 'Plate finishing',
    marks: { none: 'None', number: 'Plate no.', ral: 'RAL code' },
    mark: 'Debossed mark',
    chamfer: 'Chamfer (mm)',
    overlap: 'Overlap (mm)',
    note: 'Marks go in the margin. Chamfers widen openings on the spray side. Overlap tucks earlier coats under later ones.',
    markSkipped: (mark: string) => `No room to deboss ${mark} in the solid margin.`,
    chamferLimited: 'Chamfer limited to keep the stencil intact.',
    overlapLimited: 'Paint overlap limited to keep the stencil intact.',
    palette: 'Paint matches',
    palettes: { source: 'Source colours', ral: 'RAL Classic' },
    ral: (code: number) => `RAL ${String(code)} (approx.)`,
    ralNote: 'Screen matches are approximate. Check against a physical fan deck.'
  },

  coating: {
    heading: 'Paint order',
    number: (n: number) => `Coat ${num(n)}`,
    earlier: 'Move coat earlier',
    later: 'Move coat later',
    reset: 'Use palette order',
    options: 'Coats shown',
    all: 'All coats',
    alongside: 'Show painted result alongside',
    loading: 'Drawing the painted result…',
    preview: 'Painted result from the openings at the work face',
    note: 'From the openings against the work. Spray angle and paint bleed aren’t simulated.'
  },

  registration: {
    heading: 'Registration frame',
    enable: 'Use a keyed frame',
    clearance: 'Clearance per edge (mm)',
    fitNote: 'The off-centre key keeps the plates facing the same way.',
    clearanceNote: (mm: number) => `${num(mm, 2)} mm nominal clearance per edge`,
    save: 'Save frame STL',
    lift: 'Lift at the key. Leave the frame in place for the next plate.',
    assembly: 'Position of the plate in the frame',
    poses: { seated: 'Seated', exploded: 'Exploded' },
    seat: 'Hold the frame still and seat every plate against the same corner.'
  },

  printPreview: {
    view: 'Plate view',
    views: { top: 'Top', '3d': '3D' },
    label: '3D preview of the print geometry',
    loading: 'Loading print geometry…',
    unavailable:
      'The 3D preview could not be drawn. You can still use the Top view and save print files.',
    interaction:
      'Drag to turn, right-drag to pan, or scroll to zoom. On touch, use two fingers to pan or zoom. When focused, use arrows to turn, Shift and arrows to pan, plus or minus to zoom, and 0 to reset.',
    reset: 'Reset view'
  },

  errors: {
    notArtwork: 'Open a PNG, JPEG, WebP, SVG or .stencilator project file.',
    noArtwork: 'Open artwork first.',
    noChart: 'There is no chart for these settings.',
    noPlates: 'There are no plates for these settings.',
    tilesNeedPicture: 'Tile mode needs a PNG, JPEG or WebP picture.',
    example: 'The example could not be loaded.',
    start: (reason: string) => `The engine could not start: ${reason}`,
    stopped: (reason: string) => `The engine stopped: ${reason}`,
    builtInFont: (name: string) => `The bundled font ${name} could not be loaded.`,
    unknownRequest: 'The engine could not recognise that request.'
  }
};
