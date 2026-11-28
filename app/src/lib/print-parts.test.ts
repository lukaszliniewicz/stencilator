import { describe, expect, it } from 'vitest';

import type { BedFit } from '../engine/client';
import { printParts } from './print-parts';
import type { Workbench } from './workbench.svelte';

/** Only the fields the Tiles part list reads; the live workbench is not needed here. */
function bench(bedFit: BedFit | undefined): Workbench {
  return {
    current: true,
    valid: true,
    isFreeform: false,
    chart: { bedFit, widthMm: 20, heightMm: 20 },
    stencilsPossible: true,
    stencilCount: 1,
    colours: ['#000000'],
    unstencilled: [],
    settings: { thicknessMm: 1.2 }
  } as unknown as Workbench;
}

describe('print parts', () => {
  it('reports an invalid bed when the chart has no fit to place its parts against', () => {
    expect(printParts(bench(undefined))).toBeNull();
    expect(printParts(bench('as-drawn'))).toMatchObject([{ fit: 'as-drawn' }]);
  });
});
