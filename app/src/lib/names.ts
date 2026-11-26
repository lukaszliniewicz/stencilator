/** Names for saved files, all starting from the picture's own name. */
import type { ChartFormat } from '../engine/client';

const EXTENSIONS: Record<ChartFormat, string> = { png: 'png', jpeg: 'jpg', pdf: 'pdf' };

/** The picture's name without its extension, or "stencilator" when that leaves nothing. */
export function baseName(fileName: string): string {
  const stem = fileName.replace(/\.[^.]*$/, '').trim();
  return stem === '' ? 'stencilator' : stem;
}

export function chartFileName(base: string, format: ChartFormat, withLines: boolean): string {
  return `${base}-${withLines ? 'chart' : 'tiles'}.${EXTENSIONS[format]}`;
}

export function stencilsZipName(base: string): string {
  return `${base}-stencils.zip`;
}

/** One stencil, from the engine's name for it, such as `stencil-06-c02f34.stl`. */
export function stencilFileName(base: string, engineName: string): string {
  return `${base}-${engineName}`;
}

export function platesFileName(base: string, format: '3mf' | 'zip'): string {
  return `${base}-plates.${format}`;
}

export function projectFileName(base: string): string {
  return `${base}.stencilator`;
}
