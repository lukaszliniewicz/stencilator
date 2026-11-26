/** `#rrggbb`, as colour inputs give it, to `0xRRGGBB`, as the engine takes it. */
export function hexToNumber(hex: string): number {
  return Number.parseInt(hex.replace(/^#/, ''), 16);
}

/** An RGB triple as `#rrggbb`. */
export function rgbToHex(r: number, g: number, b: number): string {
  return `#${[r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('')}`;
}
