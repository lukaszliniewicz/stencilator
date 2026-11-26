import { describe, expect, it } from 'vitest';

import { baseName, chartFileName, stencilFileName, stencilsZipName } from './names';

describe('file names', () => {
  it('start from the picture without its extension', () => {
    expect(baseName('lighthouse.png')).toBe('lighthouse');
    expect(baseName('my.holiday.photo.jpeg')).toBe('my.holiday.photo');
    expect(baseName('README')).toBe('README');
    expect(baseName('.png')).toBe('stencilator');
    expect(baseName('')).toBe('stencilator');
  });

  it('say what each file is', () => {
    expect(chartFileName('lighthouse', 'pdf', true)).toBe('lighthouse-chart.pdf');
    expect(chartFileName('lighthouse', 'jpeg', true)).toBe('lighthouse-chart.jpg');
    expect(chartFileName('lighthouse', 'png', false)).toBe('lighthouse-tiles.png');
    expect(stencilsZipName('lighthouse')).toBe('lighthouse-stencils.zip');
    expect(stencilFileName('lighthouse', 'stencil-06-c02f34.stl')).toBe(
      'lighthouse-stencil-06-c02f34.stl'
    );
  });
});
