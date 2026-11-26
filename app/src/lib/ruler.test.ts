import { describe, expect, it } from 'vitest';

import { positions, tickSteps } from './ruler';

describe('ruler ticks', () => {
  it('suit the scale they are drawn at', () => {
    // A 400 mm canvas drawn 800 px wide.
    expect(tickSteps(2)).toEqual({ minor: 5, major: 50 });
    // The same canvas on a phone, 320 px wide.
    expect(tickSteps(0.8)).toEqual({ minor: 10, major: 100 });
    // A 40 mm coaster drawn 800 px wide.
    expect(tickSteps(20)).toEqual({ minor: 0.5, major: 5 });
  });

  it('put every label on a minor tick', () => {
    for (const scale of [0.013, 0.07, 0.3, 1, 3.7, 12, 40, 160]) {
      const { minor, major } = tickSteps(scale);
      const ratio = major / minor;
      expect(Math.abs(ratio - Math.round(ratio))).toBeLessThan(1e-9);
      expect(ratio).toBeGreaterThanOrEqual(5);
    }
  });

  it('run from zero to the length, clear of float noise', () => {
    expect(positions(1, 0.2)).toEqual([0, 0.2, 0.4, 0.6, 0.8, 1]);
    expect(positions(392.3, 100)).toEqual([0, 100, 200, 300]);
    expect(positions(0, 10)).toEqual([]);
    expect(positions(Number.NaN, 10)).toEqual([]);
  });
});
