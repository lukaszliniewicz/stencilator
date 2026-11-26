import { describe, expect, it } from 'vitest';

import { baseName, chartFileName, stencilFileName, stencilsZipName } from './names';

describe('file names', () => {
  it('start from the picture without its extension', () => {
    expect(baseName('fox.png')).toBe('fox');
    expect(baseName('my.holiday.photo.jpeg')).toBe('my.holiday.photo');
    expect(baseName('README')).toBe('README');
    expect(baseName('.png')).toBe('stencilator');
    expect(baseName('')).toBe('stencilator');
  });

  it('say what each file is', () => {
    expect(chartFileName('fox', 'pdf', true)).toBe('fox-chart.pdf');
    expect(chartFileName('fox', 'jpeg', true)).toBe('fox-chart.jpg');
    expect(chartFileName('fox', 'png', false)).toBe('fox-tiles.png');
    expect(stencilsZipName('fox')).toBe('fox-stencils.zip');
    expect(stencilFileName('fox', 'stencil-06-c02f34.stl')).toBe('fox-stencil-06-c02f34.stl');
  });
});
