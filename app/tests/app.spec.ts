import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { expect, test, type Page } from '@playwright/test';

const IMAGE = fileURLToPath(new URL('../../fixtures/reference/chart-0.png', import.meta.url));

/** Fails the test on any console error, content security policy violations included. */
function watchForProblems(page: Page): string[] {
  const problems: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') problems.push(message.text());
  });
  page.on('pageerror', (error) => problems.push(error.message));
  return problems;
}

async function openPicture(page: Page): Promise<void> {
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Open picture' }).click();
  await (await chooser).setFiles(IMAGE);
}

async function savedName(page: Page, click: () => Promise<void>): Promise<string> {
  const download = page.waitForEvent('download');
  await click();
  return (await download).suggestedFilename();
}

/** Files in a downloaded zip, counted by their entries in its central directory. */
async function zipEntries(page: Page, click: () => Promise<void>): Promise<number> {
  const download = page.waitForEvent('download');
  await click();
  const zip = await readFile(await (await download).path());
  return zip.toString('latin1').split('PK\x01\x02').length - 1;
}

test('a picture becomes a chart and files', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Drop a picture.' })).toBeVisible();
  await expect(page.getByText(/^Engine \d/)).toBeVisible();

  await openPicture(page);
  await expect(page.getByText('chart-0.png', { exact: true })).toBeVisible();
  // Drawn in blocks of 5 pixels, it is pixel art of 8 by 6.
  await expect(page.getByRole('button', { name: 'Per pixel' })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await expect(page.getByText('8 × 6 tiles · 48 in all · 5 paints')).toBeVisible();
  await expect(page.getByRole('img', { name: 'The chart, 8 by 6 tiles' })).toBeVisible();

  await page.getByRole('button', { name: 'Files' }).click();
  expect(await savedName(page, () => page.getByRole('button', { name: 'Save PDF' }).click())).toBe(
    'chart-0-chart.pdf'
  );
  await page.getByRole('button', { name: 'Tiles only' }).click();
  expect(await savedName(page, () => page.getByRole('button', { name: 'Save PNG' }).click())).toBe(
    'chart-0-tiles.png'
  );
  expect(
    await savedName(page, () => page.getByRole('button', { name: 'Save all stencils' }).click())
  ).toBe('chart-0-stencils.zip');
  expect(
    await savedName(page, () => page.getByRole('button', { name: /^Save stencil 1,/ }).click())
  ).toMatch(/^chart-0-stencil-01-[0-9a-f]{6}\.stl$/);
  await page.getByRole('button', { name: 'Back to the chart' }).click();

  expect(problems).toEqual([]);
});

test('a mistake is explained and can be undone', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await openPicture(page);
  await page.getByRole('button', { name: 'Fit to canvas' }).click();
  const width = page.getByLabel('Width (mm)', { exact: true });
  const undo = page.getByRole('button', { name: 'Undo (Ctrl+Z)' });

  await width.fill('47');
  await page.getByLabel('Height (mm)', { exact: true }).fill('40');
  await expect(undo).toBeEnabled();
  await width.fill('-5');
  await expect(page.getByRole('alert')).toHaveText(
    'The canvas width must be between 1 and 10000 mm.'
  );
  await expect(page.getByRole('button', { name: 'Files' })).toBeEnabled();

  await undo.click();
  await expect(width).toHaveValue('47');
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByText('8 × 6 tiles · 48 in all · 5 paints')).toBeVisible();

  await page.getByRole('button', { name: 'Redo (Ctrl+Shift+Z)' }).click();
  await expect(width).toHaveValue('-5');
  await page.keyboard.press('Control+z');
  await expect(width).toHaveValue('47');

  // Settings outlast a reload, the picture does not.
  await page.reload();
  await expect(page.getByLabel('Width (mm)', { exact: true })).toHaveValue('47');
  await expect(page.getByRole('heading', { name: 'Drop a picture.' })).toBeVisible();

  expect(problems).toEqual([]);
});

test('the example, a dropped picture and the theme', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');

  await page.getByRole('button', { name: 'Try the example' }).click();
  await expect(page.getByText('fox.png', { exact: true })).toBeVisible();
  await expect(page.getByRole('img', { name: 'The chart, 22 by 22 tiles' })).toBeVisible();

  const bytes = readFileSync(IMAGE).toString('base64');
  await page.evaluate((base64) => {
    const data = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const transfer = new DataTransfer();
    transfer.items.add(new File([data], 'dropped.png', { type: 'image/png' }));
    window.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, cancelable: true }));
  }, bytes);
  await expect(page.getByText('dropped.png', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Stencil' }).click();
  await expect(
    page.getByRole('img', { name: /^Stencil 1 for #[0-9A-F]{6}, with \d+ holes?$/ }).first()
  ).toBeVisible();

  const root = page.locator('html');
  await page.getByRole('button', { name: 'Dark' }).click();
  await expect(root).toHaveAttribute('data-theme', 'dark');
  await page.reload();
  await expect(root).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: 'Auto' }).click();
  await expect(root).toHaveAttribute('data-theme', 'auto');

  expect(problems).toEqual([]);
});

test('pixel art, with paints merged and the canvas colour left without a stencil', async ({
  page
}) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Try the example' }).click();
  await expect(page.getByText('22 × 22 tiles · 484 in all · 4 paints')).toBeVisible();

  // White is the canvas colour, so it starts without a stencil.
  // Found by colour: merging renumbers the paints after the one merged away.
  const white = page.getByRole('checkbox', { name: /^Stencil for paint \d+, #FFFFFF$/ });
  await expect(white).not.toBeChecked();
  await expect(page.getByRole('checkbox', { name: 'Stencil for paint 1, #2B2320' })).toBeChecked();

  await page.getByRole('button', { name: /^Paint 2, #9C3D17,/ }).click();
  await page.getByRole('button', { name: 'Merge paint 2, #9C3D17, into paint 3, #E8772E' }).click();
  await expect(page.getByText('22 × 22 tiles · 484 in all · 3 paints')).toBeVisible();
  await expect(page.getByRole('button', { name: /^Paint 2, #E8772E,/ })).toHaveAttribute(
    'aria-pressed',
    'true'
  );

  await page.getByRole('button', { name: 'Files' }).click();
  await expect(page.getByText('All 2 stencils')).toBeVisible();
  await expect(page.getByRole('button', { name: /^Save stencil 3,/ })).toHaveCount(0);
  expect(
    await zipEntries(page, () => page.getByRole('button', { name: 'Save all stencils' }).click())
  ).toBe(2);
  await page.getByRole('button', { name: 'Back to the chart' }).click();

  await white.check();
  await page.getByRole('button', { name: 'Files' }).click();
  await expect(page.getByText('All 3 stencils')).toBeVisible();
  await page.getByRole('button', { name: 'Back to the chart' }).click();

  await page.getByRole('button', { name: 'Undo merging #9C3D17 into #E8772E' }).click();
  await expect(page.getByText('22 × 22 tiles · 484 in all · 4 paints')).toBeVisible();

  expect(problems).toEqual([]);
});

test('an SVG becomes checked plates and reopens with its project settings', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Free-form', exact: true }).click();
  await page.getByRole('button', { name: 'Try the badge' }).click();

  const status = page.getByRole('status');
  await expect(status).toHaveText('70 × 70 mm · 3 paints · 2 plates · 0.1 mm cells');
  await expect(page.getByRole('button', { name: 'Tiles', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Plate', exact: true }).click();
  await expect(page.getByRole('img', { name: 'Plate 1 · #1D2F5C' })).toBeVisible();
  const islands = page.getByRole('checkbox', { name: 'Islands', exact: true });
  await islands.uncheck();
  await expect(islands).not.toBeChecked();
  await page.getByLabel('Bed width (mm)').fill('80');
  await expect(page.getByText('Larger than your print bed.')).toBeVisible();

  await page.getByRole('button', { name: 'Files', exact: true }).click();
  const printProject = page.getByRole('button', { name: 'Save 3MF' });
  expect(await savedName(page, () => printProject.click())).toBe('badge-plates.3mf');
  expect(
    await zipEntries(page, () => page.getByRole('button', { name: 'Save STL zip' }).click())
  ).toBe(2);
  expect(
    await savedName(page, () => page.getByRole('button', { name: 'Save plate 2, #EF7D2D' }).click())
  ).toBe('badge-plate-02-ef7d2d.stl');

  const saved = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Save project', exact: true }).click();
  const design = await saved;
  expect(design.suggestedFilename()).toBe('badge.stencilator');
  const buffer = await readFile(await design.path());
  await page.reload();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Open project' }).click();
  await (
    await chooser
  ).setFiles({ name: design.suggestedFilename(), mimeType: 'application/json', buffer });
  await expect(status).toHaveText('70 × 70 mm · 3 paints · 2 plates · 0.1 mm cells');
  await expect(page.getByLabel('Bed width (mm)')).toHaveValue('80');
  expect(problems).toEqual([]);
});

test('typed text uses the selected font and can be saved with every paint unticked', async ({
  page
}) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Free-form', exact: true }).click();
  await page.getByRole('button', { name: 'Type text', exact: true }).click();
  await page.getByRole('textbox', { name: 'Text', exact: true }).fill('AO8');
  await expect(page.getByText('4 loose islands', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Switch to Allerta Stencil' }).click();
  await expect(page.getByRole('combobox', { name: 'Font' })).toHaveValue(
    'AllertaStencil-Regular.ttf'
  );
  await expect(page.getByText('4 loose islands', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Side by side' }).click();
  await expect(
    page.getByRole('img', { name: 'The artwork at its original colours' })
  ).toBeVisible();

  await page.getByRole('checkbox', { name: 'Stencil for paint 1, #000000' }).uncheck();
  await expect(page.getByRole('status')).toHaveText(/2 paints · 0 plates/);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Save 3MF' })).toHaveCount(0);
  expect(
    await savedName(page, () =>
      page.getByRole('button', { name: 'Save project', exact: true }).click()
    )
  ).toBe('stencilator.stencilator');
  expect(problems).toEqual([]);
});

test('a picture can move between modes even with an unfinished setting', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await openPicture(page);
  const chart = page.getByRole('img', { name: 'The chart, 8 by 6 tiles' });
  await expect(chart).toBeVisible();

  const modes = page.getByRole('group', { name: 'Stencil mode' });
  await modes.getByRole('button', { name: 'Free-form' }).click();
  await expect(page.getByRole('status')).toHaveText(/200 × 150 mm · 5 paints · 4 plates/);
  await expect(page.getByRole('img', { name: 'Artwork reduced to its paints' })).toBeVisible();

  await page.getByLabel('Width (mm)', { exact: true }).fill('');
  await expect(page.getByRole('alert')).toHaveText('The width must be between 1 and 10000 mm.');
  await modes.getByRole('button', { name: 'Tiles', exact: true }).click();
  await expect(chart).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Per pixel' })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  expect(problems).toEqual([]);
});
