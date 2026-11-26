import { fileURLToPath } from 'node:url';

import { expect, test } from '@playwright/test';

const IMAGE = fileURLToPath(new URL('../../fixtures/reference/chart-0.png', import.meta.url));

test('an image becomes a chart and files, with no policy violations', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') problems.push(message.text());
  });
  page.on('pageerror', (error) => problems.push(error.message));

  await page.goto('/');
  await expect(page.getByText(/^Engine \d/)).toBeVisible();

  await page.getByLabel('Image').setInputFiles(IMAGE);
  await page.getByLabel('Width (mm)', { exact: true }).fill('47');
  await page.getByLabel('Height (mm)', { exact: true }).fill('40');
  await expect(page.getByText('8 × 6 tiles (48), 5 colours')).toBeVisible();
  await expect(page.getByRole('img', { name: 'The chart, 8 by 6 tiles' })).toBeVisible();

  const chart = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Save chart (PNG)' }).click();
  expect((await chart).suggestedFilename()).toBe('chart-0-chart.png');

  const stencils = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Save all stencils (zip)' }).click();
  expect((await stencils).suggestedFilename()).toBe('chart-0-stencils.zip');

  await page.getByLabel('Width (mm)', { exact: true }).fill('-5');
  await expect(page.getByRole('alert')).toHaveText(
    'The canvas width must be between 1 and 10000 mm.'
  );

  expect(problems).toEqual([]);
});
