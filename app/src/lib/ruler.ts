/** Where a ruler's ticks go, in millimetres, for whatever size it is drawn at. */

const STEPS = [
  0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000, 20_000
];
const LARGEST = 20_000;

export interface TickSteps {
  minor: number;
  major: number;
}

const isMultiple = (a: number, b: number): boolean => {
  const ratio = a / b;
  return Math.abs(ratio - Math.round(ratio)) < 1e-9;
};

/**
 * Minor ticks at least `minorPx` apart and labelled ticks at least `labelPx` apart, each
 * labelled tick on a minor one and at least five minor steps from the next.
 */
export function tickSteps(pxPerMm: number, minorPx = 5, labelPx = 44): TickSteps {
  const minor = STEPS.find((step) => step * pxPerMm >= minorPx) ?? LARGEST;
  const major =
    STEPS.find(
      (step) => step >= minor * 5 && isMultiple(step, minor) && step * pxPerMm >= labelPx
    ) ?? LARGEST;
  return { minor, major };
}

/** Every multiple of `step` from 0 to `lengthMm`, rounded clear of float noise. */
export function positions(lengthMm: number, step: number): number[] {
  if (!(lengthMm > 0) || !(step > 0)) return [];
  const count = Math.floor(lengthMm / step + 1e-9);
  return Array.from({ length: count + 1 }, (_, i) => Math.round(i * step * 1000) / 1000);
}
