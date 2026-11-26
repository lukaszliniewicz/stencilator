import { readFileSync } from 'node:fs';
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

test('a picture becomes a chart and files', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Drop a picture.' })).toBeVisible();
  await expect(page.getByText(/^Engine \d/)).toBeVisible();

  await openPicture(page);
  await expect(page.getByText('chart-0.png', { exact: true })).toBeVisible();
  await page.getByLabel('Width (mm)', { exact: true }).fill('47');
  await page.getByLabel('Height (mm)', { exact: true }).fill('40');
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
  await expect(page.getByText('lighthouse.png', { exact: true })).toBeVisible();
  await expect(page.getByRole('img', { name: /^The chart, \d+ by \d+ tiles$/ })).toBeVisible();

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
