import type { BedFit, PlateReport } from '../engine/client';
import { text } from './text';
import type { Workbench } from './workbench.svelte';

export interface PrintPart {
  id: number | 'frame';
  name: string;
  width: number;
  depth: number;
  height: number;
  fit: BedFit;
  plate?: PlateReport;
}

export function printParts(bench: Workbench): PrintPart[] {
  if (!bench.current || !bench.valid) return [];
  if (bench.isFreeform) {
    const build = bench.freeform;
    if (!build) return [];
    const parts = build.plates.map((plate): PrintPart => ({
      id: plate.paint,
      name: text.freeform.plateFor(plate.paint, plate.colour),
      width: plate.width_mm,
      depth: plate.height_mm,
      height: plate.height_with_supports_mm ?? build.thicknessMm,
      fit: plate.bed_fit,
      plate
    }));
    const frame = build.frame;
    if (frame) {
      parts.push({
        id: 'frame',
        name: text.registration.heading,
        width: frame.width_mm,
        depth: frame.height_mm,
        height: build.thicknessMm,
        fit: frame.bed_fit
      });
    }
    return parts;
  }

  const chart = bench.chart;
  const fit = chart?.bedFit;
  if (!chart || !bench.stencilsPossible || !fit) return [];
  return bench.colours.flatMap((colour, index) =>
    bench.unstencilled.includes(colour)
      ? []
      : [
          {
            id: index,
            name: text.bed.tilePart(index, colour),
            width: chart.widthMm,
            depth: chart.heightMm,
            height: bench.settings.thicknessMm,
            fit
          }
        ]
  );
}

/** Top projection of the very mesh that the slicer receives, including the frame key and supports. */
export function footprint(bytes: Uint8Array): { path: string; x: number; y: number } {
  const mesh = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const count = mesh.getUint32(80, true);
  if (!count || 84 + count * 50 !== bytes.byteLength) throw new Error(text.bed.unavailable);
  const triangles: string[] = [];
  let left = Infinity;
  let top = -Infinity;
  for (let offset = 84; offset < bytes.byteLength; offset += 50) {
    const ax = mesh.getFloat32(offset + 12, true);
    const ay = mesh.getFloat32(offset + 16, true);
    const bx = mesh.getFloat32(offset + 24, true);
    const by = mesh.getFloat32(offset + 28, true);
    const cx = mesh.getFloat32(offset + 36, true);
    const cy = mesh.getFloat32(offset + 40, true);
    left = Math.min(left, ax, bx, cx);
    top = Math.max(top, ay, by, cy);
    // Mesh faces visible from above all have the same winding, so overlapping projections fill.
    if ((bx - ax) * (cy - ay) > (by - ay) * (cx - ax)) {
      triangles.push(
        `M${ax.toFixed(4)} ${(-ay).toFixed(4)}L${bx.toFixed(4)} ${(-by).toFixed(4)} ${cx.toFixed(4)} ${(-cy).toFixed(4)}Z`
      );
    }
  }
  if (!triangles.length) throw new Error(text.bed.unavailable);
  return { path: triangles.join(''), x: -left, y: top };
}
