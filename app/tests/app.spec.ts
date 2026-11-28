import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { expect, test, type Page } from '@playwright/test';

const IMAGE = fileURLToPath(new URL('../../fixtures/reference/chart-0.png', import.meta.url));
const WIDE_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" width="80mm" height="60mm" viewBox="0 0 80 60">' +
  '<rect width="80" height="60" fill="#1d2f5c"/></svg>';
const OPEN_BUSY = 'Wait for the current file to finish opening or saving, then try again.';

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
  await page.getByRole('button', { name: 'Pixel art', exact: true }).click();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Choose artwork…' }).click();
  await (await chooser).setFiles(IMAGE);
}

async function openText(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Free-form', exact: true }).click();
  await page.getByRole('button', { name: 'Type text', exact: true }).click();
}

async function openProjectActions(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Project actions' }).click();
}

async function downloadProject(page: Page) {
  const file = page.waitForEvent('download');
  await openProjectActions(page);
  await page
    .getByRole('dialog', { name: 'Project actions' })
    .getByRole('button', { name: 'Download project', exact: true })
    .click();
  return file;
}

async function changeBed(page: Page, width: string, depth: string): Promise<void> {
  await page.getByRole('button', { name: 'Change bed…', exact: true }).first().click();
  const setup = page.getByRole('dialog', { name: 'Printer setup' });
  await setup
    .getByRole('combobox', { name: 'Printer profile' })
    .selectOption({ label: 'Custom bed' });
  await setup.getByLabel('Bed width (mm)').fill(width);
  await setup.getByLabel('Bed depth (mm)').fill(depth);
  await setup.getByRole('button', { name: 'Apply to project' }).click();
}

const workspaceReady = (page: Page) =>
  expect(page.getByRole('button', { name: 'Recent projects' })).toBeEnabled();
const saved = (page: Page) =>
  expect(page.getByText('Saved in this browser', { exact: true })).toBeVisible();
const settings = (page: Page) => page.getByRole('complementary', { name: 'Settings' });
const bench = (page: Page) => page.getByRole('main');

async function projects(page: Page, current = 'Recent projects') {
  await page.getByRole('button', { name: current, exact: true }).click();
  return page.getByRole('dialog', { name: 'Projects in this browser' });
}

async function renameProject(page: Page, title: string): Promise<void> {
  await openProjectActions(page);
  await page.getByRole('button', { name: 'Rename…' }).click();
  await page.getByRole('textbox', { name: 'Project name' }).fill(title);
  await page.getByRole('button', { name: 'Save name' }).click();
  await saved(page);
}

async function backups(page: Page, current?: string) {
  await (
    await projects(page, current)
  )
    .getByRole('button', { name: 'Back up or restore…' })
    .click();
  return page.getByRole('dialog', { name: 'Back up or restore' });
}

async function importBackup(page: Page, buffer: Buffer): Promise<void> {
  await page.getByLabel('Workspace backup file').setInputFiles({
    name: 'workspace.json',
    mimeType: 'application/json',
    buffer
  });
}

async function savedName(page: Page, click: () => Promise<void>): Promise<string> {
  const download = page.waitForEvent('download');
  await click();
  return (await download).suggestedFilename();
}

/** Holds one engine request so the test can make assertions before it finishes. */
async function controlWorkerRequest(page: Page, kind: string) {
  await page.addInitScript((kind) => {
    let release: (() => void) | undefined;
    document.addEventListener('release-worker-request', () => {
      document.documentElement.removeAttribute('data-hold-worker-request');
      release?.();
      release = undefined;
    });
    window.Worker = class extends Worker {
      override postMessage(
        message: { request?: { kind?: string } },
        options: StructuredSerializeOptions | Transferable[] = []
      ): void {
        const forward = (): void => {
          if (Array.isArray(options)) super.postMessage(message, options);
          else super.postMessage(message, options);
        };
        const root = document.documentElement;
        if (message.request?.kind === kind && root.dataset.holdWorkerRequest === 'next') {
          release = forward;
          root.dataset.holdWorkerRequest = 'held';
        } else forward();
      }
    };
  }, kind);
  return {
    hold: () =>
      page.evaluate(() => {
        document.documentElement.dataset.holdWorkerRequest = 'next';
      }),
    held: () => expect(page.locator('html')).toHaveAttribute('data-hold-worker-request', 'held'),
    release: () => page.evaluate(() => document.dispatchEvent(new Event('release-worker-request')))
  };
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
  await expect(page.getByRole('heading', { name: 'What are you making?' })).toBeVisible();
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

  await page.getByRole('button', { name: 'Files', exact: true }).click();
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
  await expect(page.getByRole('button', { name: 'Files', exact: true })).toBeEnabled();

  await undo.click();
  await expect(width).toHaveValue('47');
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByText('8 × 6 tiles · 48 in all · 5 paints')).toBeVisible();

  await page.getByRole('button', { name: 'Redo (Ctrl+Shift+Z)' }).click();
  await expect(width).toHaveValue('-5');
  await page.keyboard.press('Control+z');
  await expect(width).toHaveValue('47');

  // The picture, its edits and view survive a reload.
  await page.getByRole('button', { name: 'Stencil', exact: true }).click();
  await saved(page);
  await page.reload();
  await expect(page.getByLabel('Width (mm)', { exact: true })).toHaveValue('47');
  await expect(page.getByText('chart-0.png', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Stencil', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );

  expect(problems).toEqual([]);
});

test('a selected paint keeps its place when a photo is reduced to new colours', async ({
  page
}) => {
  await page.goto('/');
  const photo = fileURLToPath(
    new URL('../../fixtures/reference/plates-photo.png', import.meta.url)
  );
  await workspaceReady(page);
  await page.getByLabel('Picture file', { exact: true }).setInputFiles(photo);
  await expect(page.getByRole('img', { name: 'The chart, 34 by 23 tiles' })).toBeVisible();
  const paint = page.getByRole('button', { name: /^Paint 4,/ });
  await paint.click();
  const original = await paint.getAttribute('aria-label');

  await page.getByLabel('Tile (mm)', { exact: true }).fill('6');
  await expect(page.getByRole('img', { name: 'The chart, 29 by 19 tiles' })).toBeVisible();
  expect(await paint.getAttribute('aria-label')).not.toBe(original);
  await expect(paint).toHaveAttribute('aria-pressed', 'true');
});

test('the example, a dropped picture and the theme', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');

  await page.getByRole('button', { name: 'Pixel art', exact: true }).click();
  await page.getByRole('button', { name: 'Try the fox' }).click();
  await expect(page.getByText('fox.png', { exact: true })).toBeVisible();
  await expect(page.getByRole('img', { name: 'The chart, 22 by 22 tiles' })).toBeVisible();

  const bytes = readFileSync(IMAGE).toString('base64');
  await page.evaluate((base64) => {
    const data = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const transfer = new DataTransfer();
    transfer.items.add(new File([data], 'dropped.png', { type: 'image/png' }));
    window.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, cancelable: true }));
  }, bytes);
  await expect(page.getByRole('dialog', { name: 'Open “dropped”' })).toBeVisible();
  await page.getByRole('button', { name: 'Start new project', exact: true }).click();
  await expect(page.getByText('dropped.png', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Stencil' }).click();
  await expect(
    page.getByRole('img', { name: /^Stencil 1 for #[0-9A-F]{6}, with \d+ holes?$/ }).first()
  ).toBeVisible();

  const root = page.locator('html');
  await page.getByRole('combobox', { name: 'Theme' }).selectOption('dark');
  await expect(root).toHaveAttribute('data-theme', 'dark');
  await saved(page);
  await page.evaluate(() => {
    localStorage.setItem('stencilator.theme', JSON.stringify('auto'));
  });
  await page.reload();
  await expect(page.getByRole('button', { name: 'dropped', exact: true })).toBeEnabled();
  await expect(root).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('combobox', { name: 'Theme' }).selectOption('auto');
  await expect(root).toHaveAttribute('data-theme', 'auto');
  await page.evaluate(() => {
    localStorage.setItem('stencilator.theme', JSON.stringify('dark'));
  });
  await page.reload();
  await expect(page.getByRole('button', { name: 'dropped', exact: true })).toBeEnabled();
  await expect(root).toHaveAttribute('data-theme', 'auto');

  expect(problems).toEqual([]);
});

test('a second drop during a project open tells the user to retry', async ({ page }) => {
  await page.goto('/');
  await workspaceReady(page);
  await page.evaluate((base64) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    for (const name of ['first.png', 'second.png']) {
      const transfer = new DataTransfer();
      transfer.items.add(new File([bytes], name, { type: 'image/png' }));
      window.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, cancelable: true }));
    }
  }, readFileSync(IMAGE).toString('base64'));
  await expect(page.getByRole('alert')).toContainText(OPEN_BUSY);
  await expect(page.getByRole('button', { name: 'first', exact: true })).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('the open shortcut stays in the app during a project transition', async ({ page }) => {
  const request = await controlWorkerRequest(page, 'open');
  await page.goto('/');
  await workspaceReady(page);
  await request.hold();
  await openPicture(page);
  await request.held();

  const prevented = await page.evaluate(() =>
    ['ctrlKey', 'metaKey'].map(
      (modifier) =>
        !window.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'o', [modifier]: true, cancelable: true })
        )
    )
  );
  expect.soft(prevented).toEqual([true, true]);
  await expect(page.getByRole('alert')).toHaveText(OPEN_BUSY);
  await request.release();
  await expect(page.getByRole('button', { name: 'chart-0', exact: true })).toBeEnabled();
});

test('a theme write failure keeps the local choice through a workspace refresh', async ({
  page
}) => {
  await page.goto('/');
  await workspaceReady(page);
  await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/unbound-method -- Restored after the failed write.
    const transaction = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function (stores, mode, options) {
      if (mode === 'readwrite') {
        IDBDatabase.prototype.transaction = transaction;
        throw new DOMException('The workspace is full.', 'QuotaExceededError');
      }
      return transaction.call(this, stores, mode, options);
    };
  });
  const root = page.locator('html');
  await page.getByRole('combobox', { name: 'Theme' }).selectOption('dark');
  await expect(page.getByRole('alert')).toHaveText('The browser ran out of storage space.');
  await expect(root).toHaveAttribute('data-theme', 'dark');

  const picker = await projects(page);
  await picker.getByRole('button', { name: 'Manage printer profiles…' }).click();
  const profiles = page.getByRole('dialog', { name: 'Printer profiles' });
  await profiles.getByRole('textbox', { name: 'Profile name' }).fill('Sheet A');
  await profiles.getByRole('button', { name: 'Save profile' }).click();
  // The option comes from the refreshed IndexedDB snapshot.
  await expect(profiles.getByRole('option', { name: 'Sheet A' })).toBeAttached();
  await expect(root).toHaveAttribute('data-theme', 'dark');
  expect(await page.evaluate(() => localStorage.getItem('stencilator.theme'))).toBe('"dark"');

  await page.reload();
  await workspaceReady(page);
  await expect(root).toHaveAttribute('data-theme', 'auto');
});

test('pixel art, with paints merged and the canvas colour left without a stencil', async ({
  page
}) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Pixel art', exact: true }).click();
  await page.getByRole('button', { name: 'Try the fox' }).click();
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

  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await expect(page.getByText('All 2 stencils')).toBeVisible();
  await expect(page.getByRole('button', { name: /^Save stencil 3,/ })).toHaveCount(0);
  expect(
    await zipEntries(page, () => page.getByRole('button', { name: 'Save all stencils' }).click())
  ).toBe(2);
  await page.getByRole('button', { name: 'Back to the chart' }).click();

  await white.check();
  await page.getByRole('button', { name: 'Files', exact: true }).click();
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

  const status = bench(page).getByRole('status');
  await expect(status).toHaveText('70 × 70 mm · 3 paints · 2 plates · 0.1 mm cells');
  await expect(page.getByRole('button', { name: 'Tiles', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Plate', exact: true }).click();
  await expect(page.getByRole('img', { name: 'Plate 1 · #1D2F5C' })).toBeVisible();
  const islands = page.getByRole('checkbox', { name: 'Islands', exact: true });
  await islands.uncheck();
  await expect(islands).not.toBeChecked();
  await changeBed(page, '80', '256');
  await expect(page.getByText('Too large for this bed', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Files', exact: true }).click();
  const printProject = page.getByRole('button', { name: 'Save 3MF' });
  expect(await savedName(page, () => printProject.click())).toBe('badge-plates.3mf');
  expect(
    await zipEntries(page, () => page.getByRole('button', { name: 'Save STL zip' }).click())
  ).toBe(2);
  expect(
    await savedName(page, () => page.getByRole('button', { name: 'Save plate 2, #EF7D2D' }).click())
  ).toBe('badge-plate-02-ef7d2d.stl');

  const file = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download project', exact: true }).click();
  const design = await file;
  expect(design.suggestedFilename()).toBe('Paint Club badge.stencilator');
  const buffer = await readFile(await design.path());
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Import a project…' }).click();
  await (
    await chooser
  ).setFiles({ name: design.suggestedFilename(), mimeType: 'application/json', buffer });
  await expect(status).toHaveText('70 × 70 mm · 3 paints · 2 plates · 0.1 mm cells');
  await expect(settings(page).getByText('80 × 256 mm', { exact: true })).toBeVisible();
  expect(problems).toEqual([]);
});

test('typed text uses the selected font and can be saved with every paint unticked', async ({
  page
}) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await openText(page);
  await page.getByRole('textbox', { name: 'Text', exact: true }).fill('AO8');
  await expect(page.getByText('4 loose islands', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: /^Paint 2, #FFFFFF,/ }).click();
  await page.getByRole('button', { name: 'Switch to Allerta Stencil' }).click();
  await expect(page.getByRole('combobox', { name: 'Font' })).toHaveValue(
    'AllertaStencil-Regular.ttf'
  );
  await page.getByRole('button', { name: /^Paint 1, #000000,/ }).click();
  await expect(page.getByText('4 loose islands', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Side by side' }).click();
  await expect(
    page.getByRole('img', { name: 'The artwork at its original colours' })
  ).toBeVisible();

  await page.getByRole('checkbox', { name: 'Stencil for paint 1, #000000' }).uncheck();
  await expect(bench(page).getByRole('status')).toHaveText(/2 paints · 0 plates/);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Save 3MF' })).toHaveCount(0);
  expect(
    await savedName(page, () =>
      page.getByRole('button', { name: 'Download project', exact: true }).click()
    )
  ).toBe('Text stencil.stencilator');
  expect(problems).toEqual([]);
});

test('flat bridges hold a text island and stay with the saved project', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await openText(page);
  await page.getByRole('textbox', { name: 'Text', exact: true }).fill('O');
  const loose = page.getByText('1 loose island', { exact: true });
  await expect(loose).toBeVisible();

  const modes = page.getByRole('group', { name: 'Island supports' });
  const bridges = modes.getByRole('button', { name: 'Flat', exact: true });
  const added = page.getByText(/^2 flat bridges ·/);
  const width = page.getByRole('region', { name: 'Island supports' }).getByLabel('Width (mm)');
  await bridges.click();
  await expect(added).toBeVisible();
  await expect(loose).toHaveCount(0);
  await width.fill('1.4');

  const design = await downloadProject(page);
  const buffer = await readFile(await design.path());
  expect(buffer.toString('utf8')).toContain('"bridge_width_mm": 1.4');
  await page.getByLabel('Stencilator project file', { exact: true }).setInputFiles({
    name: design.suggestedFilename(),
    mimeType: 'application/json',
    buffer
  });
  await expect(bridges).toHaveAttribute('aria-pressed', 'true');
  await expect(width).toHaveValue('1.4');
  await expect(added).toBeVisible();
  await modes.getByRole('button', { name: 'None', exact: true }).click();
  await expect(loose).toBeVisible();
  expect(problems).toEqual([]);
});

test('an imported keyed project keeps its frame in the print files and when saved', async ({
  page
}) => {
  const problems = watchForProblems(page);
  const badge = new URL('../../fixtures/svg/badge.svg', import.meta.url);
  const project = {
    stencilator: 3,
    source: { kind: 'svg', name: 'badge.svg', data: readFileSync(badge).toString('base64') },
    freeform: { registration_clearance_mm: 0.35 },
    stencils: { bed_mm: [95, 96] }
  };
  await page.goto('/');
  await workspaceReady(page);
  await page.getByLabel('Stencilator project file', { exact: true }).setInputFiles({
    name: 'keyed.stencilator',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(project))
  });
  const files = page.getByRole('button', { name: 'Files', exact: true });
  await files.click();
  await expect(
    page.getByRole('region', { name: 'Print-bed fit' }).getByText(/Registration frame.*Too large/)
  ).toBeVisible();
  const included = page.getByText(/Both print files include the registration frame/);
  await expect(included).toBeVisible();
  const zip = page.getByRole('button', { name: 'Save STL zip' });
  expect(await zipEntries(page, () => zip.click())).toBe(3);

  const file = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download project', exact: true }).click();
  const buffer = await readFile(await (await file).path());
  expect(JSON.parse(buffer.toString('utf8'))).toMatchObject({
    freeform: { registration_clearance_mm: 0.35 },
    stencils: { bed_mm: [95, 96] }
  });

  await page.getByLabel('Picture file', { exact: true }).setInputFiles(fileURLToPath(badge));
  await page.getByRole('button', { name: 'Start new project', exact: true }).click();
  await files.click();
  await expect(included).toHaveCount(0);
  expect(await zipEntries(page, () => zip.click())).toBe(2);
  expect(problems).toEqual([]);
});

test('raised support edits validate both ends, persist and undo', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await openText(page);
  await page.getByRole('textbox', { name: 'Text', exact: true }).fill('O');
  const supports = page.getByRole('region', { name: 'Island supports' });
  await supports.getByRole('button', { name: 'Raised', exact: true }).click();
  const two = page.getByText('2 raised connectors', { exact: true });
  await expect(two).toBeVisible();
  const first = supports.getByRole('button', { name: /^1 · Connector/ });
  await first.click();
  await supports.getByRole('button', { name: 'Move ends' }).click();
  await expect(page.getByRole('button', { name: 'Top', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  const endX = supports.getByLabel('End 1 X (mm)');
  const moved = String(Math.round(Number(await endX.inputValue()) * 1000 + 20) / 1000);
  await endX.fill('-1');
  const apply = supports.getByRole('button', { name: 'Apply support' });
  await apply.click();
  await expect(supports.getByRole('alert')).toHaveText(
    'Each end needs room for its full landing on solid material.'
  );
  await endX.fill(moved);
  await apply.click();
  await expect(apply).toHaveCount(0);
  await expect(first).toBeEnabled();

  const design = await downloadProject(page);
  const buffer = await readFile(await design.path());
  expect(buffer.toString('utf8')).toContain('"stencilator": 4');
  expect(buffer.toString('utf8')).toContain('"support_edits": [');
  await page.getByLabel('Stencilator project file', { exact: true }).setInputFiles({
    name: design.suggestedFilename(),
    mimeType: 'application/json',
    buffer
  });
  await expect(first).toBeEnabled();
  await first.click();
  await supports.getByRole('button', { name: 'Move ends' }).click();
  await expect(endX).toHaveValue(moved);
  await page.keyboard.press('Escape');
  await expect(apply).toHaveCount(0);
  await supports.getByRole('button', { name: 'Remove support' }).click();
  await expect(page.getByText('1 raised connector', { exact: true })).toBeVisible();
  await expect(page.getByText('1 island with one connector towards the rim')).toBeVisible();
  await page.getByRole('button', { name: 'Undo (Ctrl+Z)' }).click();
  await expect(two).toBeVisible();
  await supports.getByRole('button', { name: 'Restore suggestions' }).click();
  await expect(supports.getByRole('button', { name: 'Restore suggestions' })).toHaveCount(0);
  await expect(two).toBeVisible();

  await first.click();
  await supports.getByRole('button', { name: 'Remove support' }).click();
  await expect(page.getByText('1 raised connector', { exact: true })).toBeVisible();
  await settings(page).getByText('Plate & registration', { exact: true }).click();
  await page.getByLabel('Margin (mm)', { exact: true }).fill('9');
  await expect(page.getByText('1 saved removal no longer matches the suggestions')).toBeVisible();
  await supports.getByRole('button', { name: /Old removal/ }).click();
  await supports.getByRole('button', { name: 'Discard old removal' }).click();
  await expect(supports.getByRole('button', { name: /Old removal/ })).toHaveCount(0);
  await expect(two).toBeVisible();
  expect(problems).toEqual([]);
});

test('the keyed 3D preview and print files follow the chosen paint order', async ({ page }) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await workspaceReady(page);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="60mm" height="40mm" viewBox="0 0 60 40">
    <path fill="#1d2f5c" d="M0 0H30V40H0Z"/><path fill="#c6532d" d="M30 0H60V40H30Z"/></svg>`;
  await page.getByLabel('Picture file', { exact: true }).setInputFiles({
    name: 'two-paints.svg',
    mimeType: 'image/svg+xml',
    buffer: Buffer.from(svg)
  });
  const registration = settings(page).getByText('Plate & registration', { exact: true });
  const finishing = settings(page).getByText('Detail & finishing', { exact: true });
  await registration.click();
  await page.getByRole('checkbox', { name: 'Use a keyed frame' }).check();
  await page.getByRole('button', { name: 'RAL Classic', exact: true }).click();
  await finishing.click();
  await page.getByRole('button', { name: 'RAL code', exact: true }).click();
  await page.getByLabel('Chamfer (mm)').fill('0.4');
  await page.getByLabel('Overlap (mm)').fill('0.2');
  await page.getByRole('button', { name: /^Paint 2, #C6532D,/ }).click();
  await page.getByRole('button', { name: 'Move coat earlier' }).click();

  await page.getByRole('button', { name: 'Painted result', exact: true }).click();
  const result = page.getByRole('img', {
    name: 'Painted result from the openings at the work face'
  });
  const coats = result.locator('path[data-paint]');
  await expect(coats).toHaveCount(2);
  await expect(coats.first()).toHaveAttribute('data-paint', '1');
  await expect(coats.last()).toHaveAttribute('data-paint', '0');
  await page.getByRole('combobox', { name: 'Coats shown' }).selectOption({ label: 'Coat 1' });
  await expect(coats).toHaveCount(1);
  await expect(coats.first()).toHaveAttribute('data-paint', '1');
  await expect(coats.first()).not.toHaveAttribute('fill', '#c6532d');

  await page.getByRole('button', { name: 'Plate', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Show painted result alongside' }).check();
  await page.getByRole('button', { name: '3D', exact: true }).click();
  const exploded = page.getByRole('button', { name: 'Exploded', exact: true });
  await expect(exploded).toBeEnabled();
  const model = page.getByRole('application', { name: '3D preview of the print geometry' });
  const canvas = model.locator('canvas');
  const seated = await canvas.screenshot();
  // Measure the silhouette on the flat canvas background to check zoom direction.
  const modelArea = (png: Buffer) =>
    page.evaluate(async (base64) => {
      const image = new Image();
      image.src = `data:image/png;base64,${base64}`;
      await image.decode();
      const bitmap = new OffscreenCanvas(image.naturalWidth, image.naturalHeight);
      const context = bitmap.getContext('2d');
      if (!context) throw new Error('Could not read the model screenshot');
      context.drawImage(image, 0, 0);
      const { data } = context.getImageData(0, 0, bitmap.width, bitmap.height);
      const background = (4 * bitmap.width + Math.floor(bitmap.width / 2)) * 4;
      const [red, green, blue] = data.subarray(background, background + 3);
      let pixels = 0;
      for (let i = 0; i < data.length; i += 4) {
        if (data[i] !== red || data[i + 1] !== green || data[i + 2] !== blue) pixels++;
      }
      return pixels;
    }, png.toString('base64'));
  const originalArea = await modelArea(seated);
  await model.press('+');
  expect(await modelArea(await canvas.screenshot())).toBeGreaterThan(originalArea);
  await model.press('0');
  await model.press('-');
  expect(await modelArea(await canvas.screenshot())).toBeLessThan(originalArea);
  await model.press('0');
  await exploded.click();
  expect(await canvas.screenshot()).not.toEqual(seated);
  await expect(result).toBeVisible();

  await page.getByRole('button', { name: 'Files', exact: true }).click();
  const parts = page.getByRole('region', { name: 'Individual parts' });
  await expect(parts.getByRole('list').getByRole('listitem').first()).toContainText('Plate 2');
  expect(
    await savedName(page, () => parts.getByRole('button', { name: 'Save frame STL' }).click())
  ).toBe('two-paints-registration-frame.stl');
  expect(
    await savedName(page, () =>
      parts.getByRole('button', { name: 'Save plate 2, #C6532D' }).click()
    )
  ).toBe('two-paints-plate-02-c6532d.stl');

  const file = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download project', exact: true }).click();
  const design = await file;
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByLabel('Stencilator project file', { exact: true }).setInputFiles({
    name: design.suggestedFilename(),
    mimeType: 'application/json',
    buffer: await readFile(await design.path())
  });
  await registration.click();
  await finishing.click();
  await expect(page.getByRole('checkbox', { name: 'Use a keyed frame' })).toBeChecked();
  await expect(page.getByLabel('Chamfer (mm)')).toHaveValue('0.4');
  await expect(page.getByLabel('Overlap (mm)')).toHaveValue('0.2');
  await expect(page.getByRole('button', { name: 'RAL code', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await page.getByRole('button', { name: 'Painted result', exact: true }).click();
  await expect(coats.first()).toHaveAttribute('data-paint', '1');
  expect(problems).toEqual([]);
});

test('the 3D preview marks the old frame as stale while the new mesh loads', async ({ page }) => {
  await page.addInitScript(() => {
    let release: (() => void) | undefined;
    document.addEventListener('release-print-geometry', () => {
      document.documentElement.removeAttribute('data-hold-plate-file');
      release?.();
      release = undefined;
    });
    window.Worker = class extends Worker {
      override postMessage(
        message: { request?: { kind?: string } },
        options: StructuredSerializeOptions | Transferable[] = []
      ): void {
        const forward = (): void => {
          if (Array.isArray(options)) super.postMessage(message, options);
          else super.postMessage(message, options);
        };
        const root = document.documentElement;
        if (message.request?.kind === 'plateFile' && root.hasAttribute('data-hold-plate-file')) {
          release = forward;
          root.setAttribute('data-hold-plate-file', 'held');
        } else forward();
      }
    };
  });
  await page.goto('/');
  await openText(page);
  await page.getByRole('textbox', { name: 'Text', exact: true }).fill('O');
  await settings(page).getByText('Plate & registration', { exact: true }).click();
  const frame = page.getByRole('checkbox', { name: 'Use a keyed frame' });
  await frame.check();
  await page.getByRole('button', { name: 'Plate', exact: true }).click();
  await page.getByRole('button', { name: '3D', exact: true }).click();
  const exploded = page.getByRole('button', { name: 'Exploded', exact: true });
  await expect(exploded).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.setAttribute('data-hold-plate-file', 'yes');
  });
  await frame.uncheck();
  await expect(page.locator('html')).toHaveAttribute('data-hold-plate-file', 'held');
  const model = page.getByRole('application', { name: '3D preview of the print geometry' });
  await expect(exploded).toHaveCount(0);
  await expect(model).toHaveAttribute('aria-busy', 'true');
  await expect(model.locator('canvas')).toHaveCSS('opacity', '0.4');
  await page.evaluate(() => document.dispatchEvent(new Event('release-print-geometry')));
  await expect(model).toHaveAttribute('aria-busy', 'false');
});

test('the painted result shows all remaining coats when a step disappears', async ({ page }) => {
  await page.goto('/');
  await workspaceReady(page);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="60mm" height="40mm" viewBox="0 0 60 40">
    <path fill="#1d2f5c" d="M0 0H20V40H0Z"/>
    <path fill="#c6532d" d="M20 0H40V40H20Z"/>
    <path fill="#edd4a8" d="M40 0H60V40H40Z"/>
  </svg>`;
  await page.getByLabel('Picture file', { exact: true }).setInputFiles({
    name: 'three-paints.svg',
    mimeType: 'image/svg+xml',
    buffer: Buffer.from(svg)
  });
  await page.getByRole('button', { name: 'Painted result', exact: true }).click();
  const steps = page.getByRole('combobox', { name: 'Coats shown' });
  await steps.selectOption({ label: 'Coat 2' });
  await page.getByRole('checkbox', { name: /^Stencil for paint \d+, #EDD4A8$/ }).uncheck();
  await expect(steps.locator('option')).toHaveCount(2);
  await expect(steps.locator('option:checked')).toHaveText('All coats');
  const result = page.getByRole('img', {
    name: 'Painted result from the openings at the work face'
  });
  await expect(result.locator('path[data-paint]')).toHaveCount(2);
});

test('replacing a picture starts fresh undo history for the fitted artwork', async ({ page }) => {
  await page.goto('/');
  await openPicture(page);
  await page.getByRole('button', { name: 'Free-form', exact: true }).click();
  await settings(page).getByText('Plate & registration', { exact: true }).click();
  const frame = page.getByRole('checkbox', { name: 'Use a keyed frame' });
  await frame.check();
  const undo = page.getByRole('button', { name: 'Undo (Ctrl+Z)' });
  await expect(undo).toBeEnabled();
  await page.getByRole('button', { name: /^Paint 2,/ }).click();
  await page.getByRole('button', { name: 'Move coat earlier' }).click();

  await page
    .getByLabel('Picture file', { exact: true })
    .setInputFiles(fileURLToPath(new URL('../../fixtures/pixel-art/fox.png', import.meta.url)));
  await page
    .getByRole('dialog', { name: 'Open “fox”' })
    .getByRole('button', { name: 'Replace artwork', exact: true })
    .click();
  await expect(page.getByText('fox.png', { exact: true })).toBeVisible();
  await expect(frame).not.toBeChecked();
  await expect(undo).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Redo (Ctrl+Shift+Z)' })).toBeDisabled();
});

test('replacing a raster picture keeps settings-only undo', async ({ page }) => {
  const write = await controlWorkerRequest(page, 'projectFile');
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  await write.hold();
  const tile = page.getByLabel('Tile (mm)', { exact: true });
  await tile.fill('6');
  const undo = page.getByRole('button', { name: 'Undo (Ctrl+Z)' });
  await expect(undo).toBeEnabled();

  await page
    .getByLabel('Picture file', { exact: true })
    .setInputFiles(fileURLToPath(new URL('../../fixtures/pixel-art/fox.png', import.meta.url)));
  const dialog = page.getByRole('dialog', { name: 'Open “fox”' });
  await expect(dialog).toBeVisible();
  await write.held();
  // Saving the draft already in this tab should not invalidate confirmation.
  await write.release();
  await saved(page);
  await dialog.getByRole('button', { name: 'Replace artwork', exact: true }).click();
  await expect(page.getByText('fox.png', { exact: true })).toBeVisible();
  await expect(undo).toBeEnabled();
  await undo.click();
  await expect(tile).toHaveValue('5');
  await expect(page.getByText('fox.png', { exact: true })).toBeVisible();
});

test('a picture can switch modes and be saved with an unfinished setting in the other mode', async ({
  page
}) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await openPicture(page);
  const chart = page.getByRole('img', { name: 'The chart, 8 by 6 tiles' });
  await expect(chart).toBeVisible();

  const modes = page.getByRole('group', { name: 'Stencil mode' });
  await modes.getByRole('button', { name: 'Free-form' }).click();
  await expect(bench(page).getByRole('status')).toHaveText(/200 × 150 mm · 5 paints · 4 plates/);
  await expect(page.getByRole('img', { name: 'Artwork reduced to its paints' })).toBeVisible();

  const width = page.getByLabel('Width (mm)', { exact: true });
  await width.fill('160');
  await expect(bench(page).getByRole('status')).toContainText('160 × 120 mm');
  await width.fill('');
  await expect(page.getByRole('alert')).toHaveText('The width must be between 1 and 10000 mm.');
  await modes.getByRole('button', { name: 'Tiles', exact: true }).click();
  await expect(chart).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Per pixel' })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await expect(page.getByRole('region', { name: 'Could not save in this browser' })).toContainText(
    'Check the Free-form settings before saving.'
  );

  const design = await downloadProject(page);
  const buffer = await readFile(await design.path());
  const confirm = page.waitForEvent('dialog').then(async (prompt) => {
    await prompt.accept();
    return prompt.message();
  });
  await page.getByLabel('Stencilator project file', { exact: true }).setInputFiles({
    name: 'Recovered chart.stencilator',
    mimeType: 'application/json',
    buffer
  });
  expect(await confirm).toContain('Leave it without saving');
  await expect(page.getByRole('button', { name: 'Recovered chart', exact: true })).toBeEnabled();
  await expect(chart).toBeVisible();
  await modes.getByRole('button', { name: 'Free-form' }).click();
  await expect(width).toHaveValue('160');
  await expect(page.getByRole('alert')).toHaveCount(0);
  expect(problems).toEqual([]);
});

test('pasting into a text field keeps the artwork and a separate project leaves the text intact', async ({
  page
}) => {
  const problems = watchForProblems(page);
  await page.goto('/');
  await openText(page);
  const typed = page.getByRole('textbox', { name: 'Text', exact: true });
  await typed.fill('PAINT');

  await typed.evaluate((element, base64) => {
    const bytes = Uint8Array.from(atob(base64), (character) => character.charCodeAt(0));
    const clipboardData = new DataTransfer();
    clipboardData.items.add(new File([bytes], 'pasted.png', { type: 'image/png' }));
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData, bubbles: true }));
  }, readFileSync(IMAGE).toString('base64'));
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(typed).toHaveValue('PAINT');

  const picker = page.getByLabel('Picture file', { exact: true });
  await picker.setInputFiles(IMAGE);
  const dialog = page.getByRole('dialog', { name: 'Open “chart-0”' });
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(typed).toHaveValue('PAINT');

  await picker.setInputFiles(IMAGE);
  await dialog.getByRole('button', { name: 'Start new project', exact: true }).click();
  await expect(page.getByText('chart-0.png', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'chart-0', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: /^Text stencil/ })
    .click();
  await expect(typed).toHaveValue('PAINT');
  expect(problems).toEqual([]);
});

test('a sample is temporary until kept, and projects with the same artwork have separate histories', async ({
  page
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Tile grid', exact: true }).click();
  await page.getByRole('button', { name: 'Try the fox' }).click();
  await expect(page.getByRole('button', { name: 'Keep project' })).toBeVisible();
  const recent = await projects(page, 'Fox tile chart');
  await expect(recent.getByText('No saved projects here yet.')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Keep project' }).click();
  await saved(page);

  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await openPicture(page);
  await renameProject(page, 'Garden sign');
  const tile = page.getByLabel('Tile (mm)', { exact: true });
  await tile.fill('6');
  await page.getByRole('button', { name: 'Stencil', exact: true }).click();
  await saved(page);
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await openPicture(page);
  await renameProject(page, 'Workshop sign');
  await expect(tile).toHaveValue('5');
  await expect(page.getByRole('button', { name: 'Undo (Ctrl+Z)' })).toBeDisabled();

  // Switch straight after an edit, while the other project is rebuilding.
  await tile.fill('8');
  const picker = await projects(page, 'Workshop sign');
  await picker.getByRole('searchbox').fill('garden');
  await expect(picker.getByRole('button', { name: /^Workshop sign/ })).toHaveCount(0);
  await picker.getByRole('button', { name: /^Garden sign/ }).click();
  await expect(tile).toHaveValue('6');
  await expect(page.getByRole('button', { name: 'Stencil', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await page.getByRole('button', { name: 'Undo (Ctrl+Z)' }).click();
  await expect(tile).toHaveValue('5');
  await saved(page);
  await (
    await projects(page, 'Garden sign')
  )
    .getByRole('button', { name: /^Workshop sign/ })
    .click();
  await expect(tile).toHaveValue('8');
  await (
    await projects(page, 'Workshop sign')
  )
    .getByRole('button', { name: /^Garden sign/ })
    .click();
  await expect(page.getByRole('button', { name: 'Garden sign', exact: true })).toBeEnabled();
  await page.reload();
  await expect(page.getByRole('button', { name: 'Garden sign', exact: true })).toBeVisible();
});

test('a saved project that fails to open can be retried from the Projects dialog', async ({
  page
}) => {
  await page.goto('/');
  await openText(page);
  await page.getByRole('textbox', { name: 'Text', exact: true }).fill('O');
  await renameProject(page, 'Letter');
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await openPicture(page);
  await saved(page);

  const font = '**/NotoSans-Bold-*.ttf';
  await page.route(font, (route) => route.fulfill({ status: 503 }));
  await page.reload();
  const picker = await projects(page, 'chart-0');
  const letter = picker.getByRole('button', { name: /^Letter/ });
  await letter.click();
  await expect(picker.getByRole('alert')).toHaveText(
    'The bundled font NotoSans-Bold.ttf could not be loaded.'
  );
  await expect(letter).toBeEnabled();

  await page.unroute(font);
  await letter.click();
  await expect(picker).not.toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Text', exact: true })).toHaveValue('O');
});

for (const [reason, contents] of Object.entries({
  'malformed JSON': 'not json',
  'unsupported version': '{"stencilator":999}',
  'invalid picture': '{"stencilator":1,"source":{"kind":"picture","data":"YmFkIGltYWdl"}}'
})) {
  test(`an unreadable project file (${reason}) leaves the open project in place`, async ({
    page
  }) => {
    await page.goto('/');
    await openPicture(page);
    await saved(page);
    const alert = page.getByRole('alert');
    await expect(alert).toHaveCount(0);
    await page.getByLabel('Stencilator project file').setInputFiles({
      name: 'Unreadable.stencilator',
      mimeType: 'application/json',
      buffer: Buffer.from(contents)
    });
    await expect(alert).toHaveText(
      'This project file could not be opened. It may be damaged or need a newer version of Stencilator.'
    );
    await expect(page.getByRole('button', { name: 'chart-0', exact: true })).toBeEnabled();
    await page.getByRole('button', { name: 'Files', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Save stencil 1,/ })).toBeEnabled();
  });
}

test('the dedicated project picker rejects artwork without opening a new project', async ({
  page
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  await page.getByLabel('Stencilator project file', { exact: true }).setInputFiles(IMAGE);
  await expect(page.getByRole('alert')).toContainText('Choose a .stencilator project file.');
  await expect(page.getByRole('button', { name: 'chart-0', exact: true })).toBeVisible();
  const picker = await projects(page, 'chart-0');
  await expect(picker.getByRole('listitem')).toHaveCount(1);
});

test('modal backdrop dismissal requires the same uncancelled pointer', async ({ page }) => {
  await page.goto('/');
  const dialog = await projects(page);
  await dialog.evaluate((node) => {
    const { left, top } = node.getBoundingClientRect();
    const send = (type: string, pointerId: number, outside: boolean) =>
      node.dispatchEvent(
        new PointerEvent(type, {
          pointerId,
          pointerType: 'touch',
          bubbles: true,
          clientX: outside ? left - 1 : left + 10,
          clientY: outside ? top - 1 : top + 10
        })
      );
    send('pointerdown', 1, false);
    send('pointerdown', 2, true);
    send('pointerup', 1, true);
    send('pointercancel', 2, true);
    send('pointerup', 2, true);
  });
  await expect(dialog).toBeVisible();
  await dialog.evaluate((node) => {
    const { left, top } = node.getBoundingClientRect();
    for (const type of ['pointerdown', 'pointerup']) {
      node.dispatchEvent(
        new PointerEvent(type, {
          pointerId: 3,
          pointerType: 'touch',
          bubbles: true,
          clientX: left - 1,
          clientY: top - 1
        })
      );
    }
  });
  await expect(dialog).toHaveCount(0);
});

test('the Projects dialog explains why opening is blocked during an STL export', async ({
  page
}) => {
  const stencil = await controlWorkerRequest(page, 'stencil');
  await page.goto('/');
  await openPicture(page);
  await renameProject(page, 'First chart');
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await openPicture(page);
  await saved(page);

  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await stencil.hold();
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: /^Save stencil 1,/ }).click();
  await stencil.held();

  const picker = await projects(page, 'chart-0');
  const first = picker.getByRole('button', { name: /^First chart/ });
  await first.click();
  await expect(picker.getByRole('alert')).toHaveText(OPEN_BUSY);
  await page.keyboard.press('Escape');
  await projects(page, 'chart-0');
  await picker.getByRole('button', { name: 'Import a project…' }).click();
  await expect(picker.getByRole('alert')).toHaveText(OPEN_BUSY);
  await page.keyboard.press('Escape');
  await projects(page, 'chart-0');
  // A file-picker result can also arrive after the export has started.
  await page.getByLabel('Stencilator project file').setInputFiles({
    name: 'Incoming.stencilator',
    mimeType: 'application/json',
    buffer: Buffer.from('not json')
  });
  await expect(picker.getByRole('alert')).toHaveText(OPEN_BUSY);

  await stencil.release();
  expect((await download).suggestedFilename()).toMatch(/\.stl$/);
  await first.click();
  await expect(page.getByRole('button', { name: 'First chart', exact: true })).toBeEnabled();
});

for (const withDraft of [false, true]) {
  test(
    withDraft
      ? 'a local edit made during an STL export survives a newer save from another tab'
      : 'a clean tab opens the newer save after its STL export',
    async ({ page, context }) => {
      await page.goto('/');
      await openPicture(page);
      await saved(page);
      const other = await context.newPage();
      await other.addInitScript(() => {
        let release: (() => void) | undefined;
        document.addEventListener('release-stencil', () => {
          release?.();
        });
        window.Worker = class extends Worker {
          override postMessage(
            message: { request?: { kind?: string } },
            options: StructuredSerializeOptions | Transferable[] = []
          ): void {
            if (message.request?.kind === 'stencil') {
              release = () => {
                if (Array.isArray(options)) super.postMessage(message, options);
                else super.postMessage(message, options);
              };
              document.documentElement.dataset.heldStencil = 'yes';
            } else if (Array.isArray(options)) super.postMessage(message, options);
            else super.postMessage(message, options);
          }
        };
      });
      await other.goto('/');
      await other.getByRole('button', { name: 'Files', exact: true }).click();
      const download = other.waitForEvent('download');
      await other.getByRole('button', { name: /^Save stencil 1,/ }).click();
      await expect(other.locator('html')).toHaveAttribute('data-held-stencil', 'yes');

      const size = 'Tile (mm)';
      await page.getByLabel(size, { exact: true }).fill('6');
      await saved(page);
      await renameProject(page, 'Newer chart');
      const picker = await projects(other, 'chart-0');
      // The list has read the newer save, but the export still has the old project open.
      await expect(picker.getByRole('button', { name: /^Newer chart/ })).toBeVisible();
      await other.keyboard.press('Escape');
      await other.getByRole('button', { name: 'Back to the chart' }).click();
      const tile = other.getByLabel(size, { exact: true });
      await expect(tile).toHaveValue('5');
      if (withDraft) await tile.fill('7');
      await other.evaluate(() => document.dispatchEvent(new Event('release-stencil')));
      expect((await download).suggestedFilename()).toMatch(/^chart-0-stencil-01-[0-9a-f]{6}\.stl$/);

      if (withDraft) {
        await expect(other.getByRole('button', { name: 'Load the newer version' })).toBeVisible();
        await expect(tile).toHaveValue('7');
        await expect(page.getByLabel(size, { exact: true })).toHaveValue('6');
      } else {
        await expect(tile).toHaveValue('6');
        await expect(other.getByRole('button', { name: 'Newer chart', exact: true })).toBeVisible();
        await saved(other);
      }
    }
  );
}

test('a second tab cannot overwrite a newer save and can keep its draft as a copy', async ({
  page,
  context
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const other = await context.newPage();
  await other.addInitScript(() => {
    let held: (() => void) | undefined;
    let intercept = true;
    document.addEventListener('release-save', () => {
      held?.();
      held = undefined;
    });
    window.Worker = class extends Worker {
      override postMessage(
        message: { request?: { kind?: string } },
        options: StructuredSerializeOptions | Transferable[] = []
      ): void {
        if (message.request?.kind === 'projectFile' && intercept) {
          intercept = false;
          held = () => {
            if (Array.isArray(options)) super.postMessage(message, options);
            else super.postMessage(message, options);
          };
          document.documentElement.dataset.heldSave = 'yes';
        } else if (Array.isArray(options)) super.postMessage(message, options);
        else super.postMessage(message, options);
      }
    };
  });
  await other.goto('/');
  const size = 'Tile (mm)';
  await other.getByLabel(size, { exact: true }).fill('7');
  await expect(other.locator('html')).toHaveAttribute('data-held-save', 'yes');
  await page.getByLabel(size, { exact: true }).fill('6');
  await saved(page);
  await other.evaluate(() => document.dispatchEvent(new Event('release-save')));
  await expect(other.getByRole('button', { name: 'Load the newer version' })).toBeVisible();
  await expect(other.getByLabel(size, { exact: true })).toHaveValue('7');
  await other.getByRole('button', { name: 'Keep mine as a copy' }).click();
  await expect(other.getByRole('button', { name: 'chart-0 (copy)', exact: true })).toBeVisible();
  await expect(other.getByLabel(size, { exact: true })).toHaveValue('7');
  await expect(page.getByLabel(size, { exact: true })).toHaveValue('6');
});

test('a view-only save from another tab keeps Files and this tab’s undo history', async ({
  page,
  context
}) => {
  await page.addInitScript(() => {
    window.BroadcastChannel = class extends BroadcastChannel {
      constructor(name: string) {
        super(name);
        if (name !== 'stencilator-workspace') return;
        this.addEventListener('message', () => {
          const root = document.documentElement;
          root.dataset.workspaceBroadcasts = String(
            Number(root.dataset.workspaceBroadcasts ?? 0) + 1
          );
        });
      }
    };
  });
  const revision = () =>
    page.evaluate(
      () =>
        new Promise<number>((resolve, reject) => {
          const request = indexedDB.open('stencilator-workspace');
          request.onerror = () => {
            reject(request.error ?? new Error('The workspace database could not open.'));
          };
          request.onsuccess = () => {
            const db = request.result;
            const tx = db.transaction('projects', 'readonly');
            const read = tx.objectStore('projects').getAll();
            read.onsuccess = () => {
              const record = (read.result as { title: string; revision: number }[]).find(
                ({ title }) => title === 'chart-0'
              );
              if (!record) {
                db.close();
                reject(new Error('The saved project could not be read.'));
                return;
              }
              tx.oncomplete = () => {
                db.close();
                resolve(record.revision);
              };
            };
          };
        })
    );

  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const tile = page.getByLabel('Tile (mm)', { exact: true });
  await tile.fill('6');
  await saved(page);
  await page.getByRole('button', { name: 'Side by side', exact: true }).click();
  await saved(page);
  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Files', exact: true })).toBeVisible();
  const before = await revision();

  const other = await context.newPage();
  await other.goto('/');
  await expect(other.getByLabel('Tile (mm)', { exact: true })).toHaveValue('6');
  // A paint choice changes only the view, so the artwork bytes stay the same.
  const paint = other.getByRole('button', { name: /^Paint 2, #/ });
  await paint.click();
  await expect(paint).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(revision).toBeGreaterThan(before);
  await saved(other);

  // This tab saw the newer revision and adopted it without reopening the project.
  await expect(page.locator('html')).toHaveAttribute('data-workspace-broadcasts', '1');
  await expect(page.getByRole('heading', { name: 'Files', exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Back to the chart' }).click();
  const sideBySide = page.getByRole('button', { name: 'Side by side', exact: true });
  await expect(sideBySide).toHaveAttribute('aria-pressed', 'true');
  // Later view-only saves commit against the adopted revision instead of conflicting.
  const adopted = await revision();
  await page.getByRole('button', { name: 'Chart', exact: true }).click();
  await expect.poll(revision).toBeGreaterThan(adopted);
  await saved(page);
  const adoptedAfterWrite = await revision();
  await sideBySide.click();
  await expect.poll(revision).toBeGreaterThan(adoptedAfterWrite);
  await saved(page);
  await expect(sideBySide).toHaveAttribute('aria-pressed', 'true');

  const undo = page.getByRole('button', { name: 'Undo (Ctrl+Z)' });
  await expect(undo).toBeEnabled();
  await undo.click();
  await expect(tile).toHaveValue('5');
  await saved(page);
});

test('a stale draft cannot bring a removed project back under its old name', async ({
  page,
  context
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const other = await context.newPage();
  // A tab can miss the notification; the revision check must still stop its write.
  await other.addInitScript(() => {
    window.BroadcastChannel = class extends BroadcastChannel {
      override set onmessage(_listener: BroadcastChannel['onmessage']) {
        // Ignore messages from other tabs.
      }
    };
  });
  await other.goto('/');
  const tile = other.getByLabel('Tile (mm)', { exact: true });
  await expect(tile).toHaveValue('5');

  await openProjectActions(page);
  await page.getByRole('button', { name: 'Remove from this browser…' }).click();
  await page.getByRole('button', { name: 'Remove project', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'What are you making?' })).toBeVisible();

  await tile.fill('7');
  await expect(
    other.getByRole('region', { name: 'This project was removed in another tab', exact: true })
  ).toBeVisible();
  await other.getByRole('button', { name: 'Keep mine as a copy' }).click();
  await saved(other);
  const picker = await projects(other, 'chart-0 (copy)');
  await expect(picker.getByRole('listitem')).toHaveCount(1);
  await expect(picker.getByRole('button', { name: /^chart-0 \(copy\)/ })).toBeVisible();
  await other.keyboard.press('Escape');
  await expect(tile).toHaveValue('7');
});

test('a rename form asks again if the name changed while it was open', async ({
  page,
  context
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const other = await context.newPage();
  await other.goto('/');

  await openProjectActions(page);
  await page.getByRole('button', { name: 'Rename…' }).click();
  const rename = page.getByRole('dialog', { name: 'Rename project' });
  await rename.getByRole('textbox', { name: 'Project name' }).fill('Garden draft');
  await renameProject(other, 'Garden final');
  await expect(page.locator('header').getByText('Garden final', { exact: true })).toBeVisible();
  await rename.getByRole('button', { name: 'Save name' }).click();
  await expect(rename.getByRole('alert')).toContainText('“Garden final”');
  await expect(rename.getByRole('textbox', { name: 'Project name' })).toHaveValue('Garden draft');
  await expect(other.getByRole('button', { name: 'Garden final', exact: true })).toBeVisible();
  await rename.getByRole('button', { name: 'Save name' }).click();
  await saved(page);
  await expect(other.getByRole('button', { name: 'Garden draft', exact: true })).toBeVisible();

  await openProjectActions(page);
  await page.getByRole('button', { name: 'Rename…' }).click();
  await rename.getByRole('textbox', { name: 'Project name' }).fill('Garden sign');
  await other.getByLabel('Tile (mm)', { exact: true }).fill('6');
  await saved(other);
  await expect(page.getByLabel('Tile (mm)', { exact: true })).toHaveValue('6');
  await rename.getByRole('button', { name: 'Save name' }).click();
  await saved(page);
  await expect(other.getByRole('button', { name: 'Garden sign', exact: true })).toBeVisible();
  await expect(other.getByLabel('Tile (mm)', { exact: true })).toHaveValue('6');
});

test('removal needs a new confirmation after another tab saves', async ({ page, context }) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const other = await context.newPage();
  await other.goto('/');

  await openProjectActions(page);
  await page.getByRole('button', { name: 'Remove from this browser…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Remove project' });
  const remove = dialog.getByRole('button', { name: 'Remove project', exact: true });
  const size = 'Tile (mm)';
  await other.getByLabel(size, { exact: true }).fill('6');
  await saved(other);
  await expect(page.getByLabel(size, { exact: true })).toHaveValue('6');
  await expect(remove).toBeDisabled();
  await expect(dialog.getByRole('alert')).toContainText('Reopen this dialog to confirm');
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await other.reload();
  await expect(other.getByLabel(size, { exact: true })).toHaveValue('6');

  await openProjectActions(page);
  await page.getByRole('button', { name: 'Remove from this browser…' }).click();
  await remove.click();
  await expect(page.getByRole('heading', { name: 'What are you making?' })).toBeVisible();
});

for (const choice of ['incoming artwork', 'text'] as const) {
  test(`replacing with ${choice} needs a new confirmation after another tab saves`, async ({
    page,
    context
  }) => {
    await page.goto('/');
    await page.getByRole('button', { name: 'Free-form', exact: true }).click();
    await page.getByRole('button', { name: 'Try the badge' }).click();
    await page.getByRole('button', { name: 'Keep project' }).click();
    await saved(page);
    const other = await context.newPage();
    await other.goto('/');

    if (choice === 'text') {
      await page.getByRole('button', { name: 'Type text', exact: true }).click();
    } else {
      await page.getByLabel('Picture file', { exact: true }).setInputFiles(IMAGE);
    }
    const dialog = page.getByRole('dialog', {
      name: choice === 'text' ? 'Type text' : 'Open “chart-0”'
    });
    const replace = dialog.getByRole('button', { name: 'Replace artwork', exact: true });
    await expect(replace).toBeEnabled();

    const file = other.waitForEvent('filechooser');
    await other.getByRole('button', { name: 'Replace', exact: true }).click();
    await (
      await file
    ).setFiles({
      name: 'updated.svg',
      mimeType: 'image/svg+xml',
      buffer: Buffer.from(WIDE_SVG)
    });
    await saved(other);
    await expect(page.getByText('updated.svg', { exact: true })).toBeVisible();
    await expect(replace).toBeDisabled();
    await expect(dialog.getByRole('alert')).toContainText('The project has changed');

    if (choice === 'text') {
      await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
      await page.getByRole('button', { name: 'Type text', exact: true }).click();
      await replace.click();
      await expect(page.getByLabel('Text', { exact: true })).toHaveValue('YOUR TEXT');
    } else {
      await dialog.getByRole('button', { name: 'Start new project', exact: true }).click();
      await expect(page.getByRole('button', { name: 'chart-0', exact: true })).toBeVisible();
      await expect(other.getByText('updated.svg', { exact: true })).toBeVisible();
    }
  });
}

test('a failed separate artwork import is visible alongside stale replacement', async ({
  page,
  context
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const other = await context.newPage();
  await other.goto('/');
  await page.getByLabel('Picture file', { exact: true }).setInputFiles({
    name: 'broken.png',
    mimeType: 'image/png',
    buffer: Buffer.from('not a PNG')
  });
  const dialog = page.getByRole('dialog', { name: 'Open “broken”' });
  const replace = dialog.getByRole('button', { name: 'Replace artwork' });
  await expect(replace).toBeEnabled();
  await other.getByLabel('Tile (mm)', { exact: true }).fill('6');
  await saved(other);
  await expect(replace).toBeDisabled();
  await expect(dialog.getByRole('alert')).toHaveCount(1);
  await dialog.getByRole('button', { name: 'Start new project' }).click();
  await expect(dialog.getByRole('alert')).toHaveCount(2);
  await expect(dialog.getByRole('alert').first()).toContainText('The project has changed');
  await expect(dialog.getByRole('alert').last()).not.toContainText('The project has changed');
  await expect(replace).toBeDisabled();
  await expect(dialog.getByRole('button', { name: 'Start new project' })).toBeEnabled();
});

test('settings and theme remembered by v0.1.0 carry over to a new workspace', async ({ page }) => {
  await page.addInitScript(() => {
    if (localStorage.getItem('stencilator.settings') === null) {
      localStorage.setItem('stencilator.settings', JSON.stringify({ tileMm: 6, sizing: 'pixels' }));
      localStorage.setItem('stencilator.theme', JSON.stringify('dark'));
    }
  });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Pixel art', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await page.getByRole('button', { name: 'Try the fox' }).click();
  await expect(page.getByLabel('Tile (mm)', { exact: true })).toHaveValue('6');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('starting a separate project with an incoming picture uses the workspace mode', async ({
  page
}) => {
  await page.addInitScript(() => {
    localStorage.setItem('stencilator.settings', JSON.stringify({ projectMode: 'freeform' }));
  });
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const modes = page.getByRole('group', { name: 'Stencil mode' });
  await expect(modes.getByRole('button', { name: 'Tiles', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );

  await page.getByLabel('Picture file', { exact: true }).setInputFiles(IMAGE);
  await page.getByRole('button', { name: 'Start new project', exact: true }).click();
  await expect(modes.getByRole('button', { name: 'Free-form', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
});

test('a browser without storage keeps the draft editable and asks before leaving it', async ({
  page
}) => {
  await page.addInitScript(() => {
    indexedDB.open = () => {
      throw new DOMException('Denied', 'SecurityError');
    };
  });
  await page.goto('/');
  await openPicture(page);
  const recovery = page.getByRole('region', { name: 'Could not save in this browser' });
  await expect(recovery.getByText('Browser storage is unavailable.')).toBeVisible();
  await page.getByLabel('Tile (mm)', { exact: true }).fill('6');
  const file = page.waitForEvent('download');
  await recovery.getByRole('button', { name: 'Download project' }).click();
  expect((await readFile(await (await file).path())).toString()).toContain('"tile_mm": 6');

  await page.getByRole('button', { name: 'New project', exact: true }).click();
  const confirm = page.waitForEvent('dialog').then(async (prompt) => {
    await prompt.dismiss();
    return prompt.message();
  });
  await page.getByRole('button', { name: 'Try the fox' }).click();
  expect(await confirm).toContain('Leave it without saving');
  await page.getByRole('button', { name: 'Back to project', exact: true }).click();
  await expect(page.getByLabel('Tile (mm)', { exact: true })).toHaveValue('6');
});

test('Retry save keeps keyboard focus through failure and returns it to the project on success', async ({
  page
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/unbound-method -- Preserve the native transaction implementation.
    const transaction = IDBDatabase.prototype.transaction;
    document.documentElement.dataset.failSave = 'yes';
    IDBDatabase.prototype.transaction = function (stores, mode, options) {
      if (mode === 'readwrite' && document.documentElement.dataset.failSave === 'yes') {
        document.documentElement.dataset.failedSaveCount = String(
          Number(document.documentElement.dataset.failedSaveCount ?? 0) + 1
        );
        throw new DOMException('The workspace is full.', 'QuotaExceededError');
      }
      return transaction.call(this, stores, mode, options);
    };
  });
  await page.getByLabel('Tile (mm)', { exact: true }).fill('6');
  const recovery = page.getByRole('region', { name: 'Could not save in this browser' });
  const retry = recovery.getByRole('button', { name: 'Retry save' });
  await expect(retry).toBeEnabled();
  const before = Number(await page.locator('html').getAttribute('data-failed-save-count'));
  await retry.focus();
  await retry.press('Enter');
  await expect(page.locator('html')).toHaveAttribute('data-failed-save-count', String(before + 1));
  await expect(retry).toBeEnabled();
  await expect(retry).toBeFocused();
  await page.evaluate(() => delete document.documentElement.dataset.failSave);
  await retry.press('Enter');
  await saved(page);
  await expect(recovery).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'chart-0', exact: true })).toBeFocused();
});

test('a new view saves automatically after a view-only save fails', async ({ page }) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/unbound-method -- Restored after the failed write.
    const transaction = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function (stores, mode, options) {
      if (mode === 'readwrite') {
        IDBDatabase.prototype.transaction = transaction;
        throw new DOMException('The workspace is full.', 'QuotaExceededError');
      }
      return transaction.call(this, stores, mode, options);
    };
  });

  await page.getByRole('button', { name: 'Picture', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Could not save in this browser' })).toContainText(
    'The browser ran out of storage space.'
  );
  const next = page.getByRole('button', { name: 'Side by side', exact: true });
  await next.click();
  await saved(page);
  await page.reload();
  await expect(next).toHaveAttribute('aria-pressed', 'true');
});

test('a failed view-only save clears its notice when the view returns to the saved one', async ({
  page
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/unbound-method -- Restored after the failed write.
    const transaction = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function (stores, mode, options) {
      if (mode === 'readwrite') {
        IDBDatabase.prototype.transaction = transaction;
        throw new DOMException('The workspace is full.', 'QuotaExceededError');
      }
      return transaction.call(this, stores, mode, options);
    };
  });

  await page.getByRole('button', { name: 'Side by side', exact: true }).click();
  const recovery = page.getByRole('region', { name: 'Could not save in this browser' });
  await expect(recovery).toContainText('The browser ran out of storage space.');

  const savedView = page.getByRole('button', { name: 'Chart', exact: true });
  await savedView.click();
  await expect(savedView).toHaveAttribute('aria-pressed', 'true');
  await expect(recovery).toHaveCount(0);
  await saved(page);
});

for (const removed of [false, true]) {
  test(
    removed
      ? 'reconnecting for Retry save cannot recreate a removed project'
      : 'Retry save reconnects after the workspace database closes unexpectedly',
    async ({ page, context }) => {
      const file = await controlWorkerRequest(page, 'projectFile');
      await page.addInitScript(() => {
        const open = indexedDB.open.bind(indexedDB);
        let connection: IDBDatabase | undefined;
        indexedDB.open = (name, version) => {
          const request = open(name, version);
          request.addEventListener('success', () => {
            connection = request.result;
          });
          return request;
        };
        document.addEventListener('close-workspace', () => {
          // Close this connection without deleting the database, and deliver
          // the event that an unexpected close would send.
          connection?.close();
          connection?.dispatchEvent(new Event('close'));
        });
      });
      await page.goto('/');
      await openPicture(page);
      await saved(page);
      await file.hold();
      const tile = page.getByLabel('Tile (mm)', { exact: true });
      await tile.fill('6');
      await file.held();
      await page.evaluate(() => document.dispatchEvent(new Event('close-workspace')));
      await file.release();
      const recovery = page.getByRole('region', { name: 'Could not save in this browser' });
      await expect(recovery).toBeVisible();

      if (removed) {
        const other = await context.newPage();
        await other.goto('/');
        await openProjectActions(other);
        await other.getByRole('button', { name: 'Remove from this browser…' }).click();
        await other.getByRole('button', { name: 'Remove project', exact: true }).click();
        await expect(other.getByRole('heading', { name: 'What are you making?' })).toBeVisible();
      }

      await recovery.getByRole('button', { name: 'Retry save' }).click();
      if (removed) {
        await expect(page.getByText('This project was removed in another tab')).toBeVisible();
        await expect(tile).toHaveValue('6');
        const picker = await projects(page, 'chart-0');
        await expect(picker.getByText('No saved projects here yet.')).toBeVisible();
      } else {
        await saved(page);
        await page.reload();
        await expect(tile).toHaveValue('6');
      }
    }
  );
}

test('the keyed badge fits turned, with the unchanged frame file when the bed gets smaller', async ({
  page
}) => {
  const build = await controlWorkerRequest(page, 'freeform');
  await page.goto('/');
  await page.getByRole('button', { name: 'Free-form', exact: true }).click();
  await page.getByRole('button', { name: 'Try the badge' }).click();
  await settings(page).getByText('Plate & registration', { exact: true }).click();
  await page.getByRole('checkbox', { name: 'Use a keyed frame' }).check();
  await page.getByLabel('Clearance per edge (mm)').fill('0.25');
  await changeBed(page, '104', '101');
  const view = page.getByRole('button', { name: 'Print bed', exact: true });
  await view.click();
  await page.getByRole('combobox', { name: 'Part on the bed' }).selectOption('frame');
  const fit = bench(page).getByRole('region', { name: 'Print-bed fit' });
  await expect(fit.getByText('Fits after a quarter-turn', { exact: true })).toBeVisible();
  const drawing = bench(page).getByRole('img', {
    name: /Registration frame on a 104 × 101 mm bed/
  });
  await expect(drawing).toBeVisible();
  await bench(page).getByRole('button', { name: 'Top', exact: true }).click();
  const outline = await drawing.locator('use.on-bed').evaluate((part) => {
    const parent = part.parentNode;
    if (!(parent instanceof SVGGElement)) throw new Error('The part has no position on the bed.');
    const box = parent.getBBox();
    return { x: box.x, y: box.y, width: box.width, height: box.height };
  });
  expect(outline.width).toBeCloseTo(103.5, 3);
  expect(outline.height).toBeCloseTo(100.5, 3);
  expect(outline.x).toBeCloseTo(0.25, 3);
  expect(outline.y).toBeCloseTo(0.25, 3);
  const fitted = page.getByRole('button', { name: 'Fit part', exact: true });
  await fitted.click();
  await expect(fitted).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('application', { name: 'Print bed' }).press('+');
  await expect(fitted).toHaveAttribute('aria-pressed', 'false');
  await page.getByRole('button', { name: 'Reset view' }).click();
  await expect(fitted).toHaveAttribute('aria-pressed', 'true');

  const saveFrame = page.getByRole('button', { name: 'Save frame STL' });
  const downloadFrame = async () => {
    const file = page.waitForEvent('download');
    await saveFrame.click();
    return readFile(await (await file).path());
  };
  await page.getByRole('button', { name: 'Files', exact: true }).click();
  const original = await downloadFrame();
  await expect(
    bench(page).getByText('The 3MF opens with the parts side by side. Arrange them in your slicer.')
  ).toBeVisible();
  const printFiles = bench(page).getByRole('region', { name: 'Individual parts' });
  const oldVerdict = printFiles.getByText('Fits after a quarter-turn');
  await expect(oldVerdict).toBeVisible();
  await build.hold();
  await changeBed(page, '103.49', '101');
  await build.held();
  await expect(fit.getByText('Checking the latest parts against this bed…')).toBeVisible();
  await expect(oldVerdict).toHaveCount(0);
  await expect(saveFrame).toBeDisabled();
  await build.release();
  await expect(printFiles.getByText('Too large for this bed')).toBeVisible();
  expect(await downloadFrame()).toEqual(original);
  await page.getByRole('button', { name: 'Back to the artwork' }).click();
  await expect(fit.getByText('Too large for this bed', { exact: true })).toBeVisible();
  await expect(fit.getByText(/Turned: 0\.01 mm wider/)).toBeVisible();
});

test('Tiles checks the saved bed in the preview and in Files', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Pixel art', exact: true }).click();
  await page.getByRole('button', { name: 'Try the fox' }).click();
  await page.getByLabel('Tile (mm)', { exact: true }).fill('20');
  await changeBed(page, '500', '500');
  await page.getByRole('button', { name: 'Print bed', exact: true }).click();
  const fit = bench(page).getByRole('region', { name: 'Print-bed fit' });
  await expect(fit.getByText('Fits as drawn', { exact: true })).toBeVisible();
  await expect(bench(page).getByRole('img', { name: /on a 500 × 500 mm bed/ })).toBeVisible();
  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await expect(fit.getByText('All parts fit as drawn.', { exact: true })).toBeVisible();
  await changeBed(page, '50', '50');
  await expect(fit.getByText(/Too large for this bed/).first()).toBeVisible();
  await fit.getByRole('button', { name: 'Change size…' }).click();
  await expect(settings(page).getByRole('heading', { name: 'Tile sizing' })).toBeFocused();
});

test('Tiles explains a legacy bed outside the fit range and still offers downloads', async ({
  page
}) => {
  await page.addInitScript(() => {
    localStorage.setItem('stencilator.settings', JSON.stringify({ bedWidthMm: 40 }));
  });
  await page.goto('/');
  await openPicture(page);
  const fit = bench(page).getByRole('region', { name: 'Print-bed fit' });
  const outsideRange = 'Set both bed dimensions between 50 and 2,000 mm to check fit.';
  await page.getByRole('button', { name: 'Print bed', exact: true }).click();
  await expect(fit.getByText(outsideRange)).toBeVisible();
  await expect(fit.getByRole('button', { name: 'Change bed…' })).toBeEnabled();
  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await expect(fit.getByText(outsideRange)).toBeVisible();
  await expect(page.getByRole('button', { name: /^Save stencil 1,/ })).toBeEnabled();
  await expect(page.getByRole('region', { name: 'Could not save in this browser' })).toContainText(
    'Check the settings for this project before saving.'
  );
  const download = page.waitForEvent('download', { timeout: 5_000 });
  await bench(page).getByRole('button', { name: 'Download project', exact: true }).click();
  const file = await readFile(await (await download).path(), 'utf8');
  expect(JSON.parse(file)).toMatchObject({ stencils: { bed_mm: [256, 256] } });
  await expect(fit.getByText(outsideRange)).toBeVisible();
  await changeBed(page, '256', '256');
  await expect(fit.getByText('All parts fit as drawn.')).toBeVisible();
});

test('the separate fit verdict does not sit beside a no-warnings message', async ({ page }) => {
  await page.goto('/');
  await workspaceReady(page);
  await page.getByLabel('Picture file', { exact: true }).setInputFiles({
    name: 'wide.svg',
    mimeType: 'image/svg+xml',
    buffer: Buffer.from(WIDE_SVG)
  });
  const noWarnings = 'No printability warnings.';
  await expect(page.getByText(noWarnings, { exact: true })).toBeVisible();
  await changeBed(page, '50', '50');
  await page.getByRole('button', { name: 'Print bed', exact: true }).click();
  const fit = bench(page).getByRole('region', { name: 'Print-bed fit' });
  await expect(fit.getByText('Too large for this bed', { exact: true })).toBeVisible();
  await expect(bench(page).getByText(noWarnings, { exact: true })).toHaveCount(0);

  await page.getByRole('button', { name: 'Files', exact: true }).click();
  await expect(fit.getByText(/Too large for this bed/)).toBeVisible();
  await expect(bench(page).getByText(noWarnings, { exact: true })).toHaveCount(0);
});

test('zero optional printer measurements name the invalid field and allow blank or small values', async ({
  page
}) => {
  await page.goto('/');
  await (await projects(page)).getByRole('button', { name: 'Manage printer profiles…' }).click();
  const profiles = page.getByRole('dialog', { name: 'Printer profiles' });
  await profiles.getByRole('textbox', { name: 'Profile name' }).fill('Fine printer');
  await profiles.getByText('Optional slicer settings', { exact: true }).click();
  const labels = ['Extrusion line width (mm)', 'Layer height (mm)', 'Nozzle diameter (mm)'];
  for (const label of labels) {
    await profiles.getByLabel(label).fill('0');
    await profiles.getByRole('button', { name: 'Save profile' }).click();
    await expect(profiles.getByRole('alert')).toContainText(label);
    await expect(profiles.getByRole('option', { name: 'Fine printer' })).toHaveCount(0);
    await profiles.getByLabel(label).fill('');
  }
  await profiles.getByLabel('Nozzle diameter (mm)').fill('0.005');
  await profiles.getByRole('button', { name: 'Save profile' }).click();
  await expect(profiles.getByRole('option', { name: 'Fine printer' })).toBeAttached();
  await expect(profiles.getByRole('alert')).toHaveCount(0);
});

test('printer setup cannot apply a bed while remote artwork is reopening', async ({
  page,
  context
}) => {
  const opening = await controlWorkerRequest(page, 'open');
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const other = await context.newPage();
  await other.goto('/');
  await page.getByRole('button', { name: 'Change bed…', exact: true }).first().click();
  const printer = page.getByRole('dialog', { name: 'Printer setup' });
  await printer.getByLabel('Bed width (mm)').fill('210');
  await printer.getByLabel('Bed depth (mm)').fill('220');
  await opening.hold();
  await other.getByLabel('Tile (mm)', { exact: true }).fill('6');
  await saved(other);
  await opening.held();
  const apply = printer.getByRole('button', { name: 'Apply to project' });
  await expect(apply).toBeDisabled();
  await printer
    .locator('form')
    .evaluate((form) =>
      form.dispatchEvent(new SubmitEvent('submit', { bubbles: true, cancelable: true }))
    );
  await expect(printer).toBeVisible();
  await opening.release();
  await expect(apply).toBeEnabled();
  await apply.click();
  await saved(page);
  await expect(printer).toHaveCount(0);
  await page.getByRole('button', { name: 'Print bed', exact: true }).click();
  await expect(bench(page).getByRole('img', { name: /on a 210 × 220 mm bed/ })).toBeVisible();
});

test('compact header follows the visual order with keyboard Tab', async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 900 });
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const first = page.getByRole('button', { name: 'chart-0', exact: true });
  const controls = [
    first,
    page.getByRole('button', { name: 'Project actions' }),
    page.getByRole('button', { name: 'New project', exact: true }),
    page.getByRole('button', { name: 'Files', exact: true }),
    page.getByRole('combobox', { name: 'Theme' })
  ];
  await first.focus();
  let lastY = 0;
  for (const control of controls) {
    await expect(control).toBeFocused();
    const box = await control.boundingBox();
    if (!box) throw new Error('Header control has no box.');
    expect(box.y + box.height / 2).toBeGreaterThanOrEqual(lastY - 5);
    lastY = box.y + box.height / 2;
    await page.keyboard.press('Tab');
  }
});

test('a printer profile can be renamed without changing small restored measurements', async ({
  page
}) => {
  await page.goto('/');
  await backups(page);
  await importBackup(
    page,
    Buffer.from(
      JSON.stringify({
        stencilator_workspace: 1,
        projects: [],
        profiles: [
          {
            id: 'printer',
            revision: 1,
            name: 'Printer',
            widthMm: 200,
            depthMm: 200,
            lineWidthMm: 0.0005,
            layerHeightMm: 0.005,
            nozzleMm: 0.009
          }
        ],
        preferences: { defaults: {}, theme: 'auto', defaultProfileId: null }
      })
    )
  );
  const preview = page.getByRole('dialog', { name: 'Preview import' });
  await preview.getByRole('button', { name: 'Import backup' }).click();
  await expect(page.getByText('Workspace imported.')).toBeVisible();
  await page.keyboard.press('Escape');

  await (await projects(page)).getByRole('button', { name: 'Manage printer profiles…' }).click();
  const profiles = page.getByRole('dialog', { name: 'Printer profiles' });
  await profiles.getByRole('textbox', { name: 'Profile name' }).fill('Fine settings');
  await profiles.getByRole('button', { name: 'Save profile' }).click();
  await expect(profiles.getByRole('option', { name: 'Fine settings' })).toBeAttached();
  await profiles.getByText('Optional slicer settings', { exact: true }).click();
  await expect(profiles.getByLabel('Extrusion line width (mm)')).toHaveValue('0.0005');
  await expect(profiles.getByLabel('Layer height (mm)')).toHaveValue('0.005');
  await expect(profiles.getByLabel('Nozzle diameter (mm)')).toHaveValue('0.009');
});

test('printer profiles keep an applied bed until explicitly updated, and backups round-trip', async ({
  page,
  browser
}) => {
  await page.goto('/');
  await openPicture(page);
  await renameProject(page, 'Garden sign');
  await settings(page).getByRole('button', { name: 'Manage printer profiles…' }).click();
  const profiles = page.getByRole('dialog', { name: 'Printer profiles' });
  await profiles.getByRole('textbox', { name: 'Profile name' }).fill('MK4 · textured sheet');
  await profiles.getByLabel('Bed width (mm)').fill('220');
  await profiles.getByLabel('Bed depth (mm)').fill('210');
  const slicerSizes = [
    ['Extrusion line width (mm)', '0.42'],
    ['Layer height (mm)', '0.2'],
    ['Nozzle diameter (mm)', '0.4']
  ] as const;
  await profiles.getByText('Optional slicer settings', { exact: true }).click();
  for (const [label, value] of slicerSizes) await profiles.getByLabel(label).fill(value);
  await profiles.getByRole('button', { name: 'Save profile' }).click();
  const printer = page.getByRole('dialog', { name: 'Printer setup' });
  await printer.getByRole('button', { name: 'Apply to project' }).click();
  await saved(page);
  await settings(page).getByRole('button', { name: 'Manage printer profiles…' }).click();
  await profiles.getByLabel('Bed width (mm)').fill('230');
  await profiles.getByText('Optional slicer settings', { exact: true }).click();
  for (const [label, value] of slicerSizes) {
    const field = profiles.getByLabel(label);
    await expect(field).toHaveValue(value);
    await field.fill('');
  }
  await profiles.getByRole('button', { name: 'Save profile' }).click();
  await expect(printer.getByText(/uses 220 × 210 mm.*now uses 230 × 210 mm/)).toBeVisible();
  await printer.getByRole('button', { name: 'Apply update' }).click();
  await expect(settings(page).getByText('230 × 210 mm', { exact: true })).toBeVisible();

  const design = await downloadProject(page);
  const expected = await readFile(await design.path());
  await page.getByRole('combobox', { name: 'Theme' }).selectOption('dark');
  await saved(page);
  const backup = await backups(page, 'Garden sign');
  const file = page.waitForEvent('download');
  await backup.getByRole('button', { name: 'Download backup', exact: true }).click();
  const backupBytes = await readFile(await (await file).path());

  const fresh = await browser.newPage();
  try {
    await fresh.goto(page.url());
    await backups(fresh);
    await importBackup(fresh, Buffer.from('{"stencilator_workspace":1,"projects":[{}]}'));
    await expect(fresh.getByRole('dialog').getByRole('alert')).toContainText(
      'No changes were made.'
    );
    await importBackup(fresh, Buffer.from('{"stencilator_workspace":2}'));
    await expect(fresh.getByRole('dialog').getByRole('alert')).toContainText(
      'a format this version cannot read'
    );
    await importBackup(fresh, backupBytes);
    const preview = fresh.getByRole('dialog', { name: 'Preview import' });
    await expect(preview.getByRole('heading', { name: 'Preview import' })).toBeFocused();
    await expect(preview.getByText('Garden sign', { exact: true })).toBeVisible();
    await expect(preview.getByText('230 × 210 mm', { exact: true })).toBeVisible();
    const preferences = preview.getByRole('checkbox', { name: 'Also restore app preferences' });
    await expect(preferences).not.toBeChecked();
    // Simulate an asynchronous request error before the transaction has its final
    // quota error. None of the already queued writes should commit.
    await fresh.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/unbound-method -- Restored below and otherwise called with the store as `this`.
      const put = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function (value: unknown, key?: IDBValidKey) {
        const request = put.call(this, value, key);
        const tx = this.transaction;
        if (
          this.name === 'workspace' &&
          tx.objectStoreNames.contains('projects') &&
          tx.objectStoreNames.contains('profiles') &&
          typeof value === 'object' &&
          value !== null &&
          'key' in value &&
          value.key === 'revision'
        ) {
          IDBObjectStore.prototype.put = put;
          request.addEventListener('success', () => {
            tx.dispatchEvent(new Event('error'));
            Object.defineProperty(tx, 'error', {
              value: new DOMException('The workspace is full.', 'QuotaExceededError')
            });
            tx.abort();
          });
        }
        return request;
      };
    });
    const importButton = preview.getByRole('button', { name: 'Import backup' });
    await preferences.check();
    await importButton.click();
    await expect(preview.getByRole('alert')).toContainText('ran out of storage space');
    await expect(fresh.locator('html')).toHaveAttribute('data-theme', 'auto');
    await preview.getByRole('button', { name: 'Back to backup options' }).click();
    await importBackup(fresh, backupBytes);
    await expect(preview.getByText('Add', { exact: true })).toHaveCount(2);
    await expect(preferences).not.toBeChecked();
    await importButton.click();
    await expect(fresh.locator('html')).toHaveAttribute('data-theme', 'auto');
    await expect(fresh.getByText('Workspace imported.')).toBeVisible();
    await importBackup(fresh, backupBytes);
    await expect(preview.getByText('Skip: already here', { exact: true })).toHaveCount(2);
    await preferences.check();
    await preview.getByRole('button', { name: 'Import backup' }).click();
    await expect(fresh.locator('html')).toHaveAttribute('data-theme', 'dark');
    await fresh.keyboard.press('Escape');
    await (await projects(fresh)).getByRole('button', { name: /^Garden sign/ }).click();
    await expect(settings(fresh).getByText('MK4 · textured sheet')).toBeVisible();
    expect(await readFile(await (await downloadProject(fresh)).path())).toEqual(expected);

    await settings(fresh).getByRole('button', { name: 'Manage printer profiles…' }).click();
    const restoredProfiles = fresh.getByRole('dialog', { name: 'Printer profiles' });
    await restoredProfiles.getByText('Optional slicer settings', { exact: true }).click();
    for (const [label] of slicerSizes)
      await expect(restoredProfiles.getByLabel(label)).toHaveValue('');
    fresh.once('dialog', (prompt) => void prompt.accept());
    await restoredProfiles.getByRole('button', { name: 'Remove profile…', exact: true }).click();
    const savedProfiles = restoredProfiles.getByRole('combobox', { name: 'Saved profiles' });
    await expect(savedProfiles.locator('option')).toHaveCount(1);
    await restoredProfiles.getByRole('button', { name: 'Close', exact: true }).click();
    await fresh.reload();
    await expect(settings(fresh).getByText('Custom bed', { exact: true })).toBeVisible();
    await expect(settings(fresh).getByText('230 × 210 mm', { exact: true })).toBeVisible();
  } finally {
    await fresh.close();
  }
});

test('skipping an already restored backup leaves another tab’s restore preview valid', async ({
  page,
  context
}) => {
  await page.addInitScript(() => {
    window.BroadcastChannel = class extends BroadcastChannel {
      override postMessage(message: unknown): void {
        if (this.name === 'stencilator-workspace')
          document.documentElement.dataset.workspaceBroadcast = 'yes';
        super.postMessage(message);
      }
    };
  });
  const printerBackup = (id: string) =>
    Buffer.from(
      JSON.stringify({
        stencilator_workspace: 1,
        projects: [],
        profiles: [{ id, name: id, revision: 1, widthMm: 220, depthMm: 220 }],
        preferences: { defaults: {}, theme: 'auto', defaultProfileId: null }
      })
    );
  await page.goto('/');
  const options = await backups(page);
  const first = printerBackup('Textured sheet');
  await importBackup(page, first);
  const preview = page.getByRole('dialog', { name: 'Preview import' });
  const restore = preview.getByRole('button', { name: 'Import backup' });
  await restore.click();
  await expect(options.getByText('Workspace imported.')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-workspace-broadcast', 'yes');

  const other = await context.newPage();
  await other.goto('/');
  const otherOptions = await backups(other);
  await importBackup(other, printerBackup('Smooth sheet'));
  const otherPreview = other.getByRole('dialog', { name: 'Preview import' });
  await expect(otherPreview.getByText('Add', { exact: true })).toBeVisible();

  await importBackup(page, first);
  await expect(preview.getByText('Skip: already here', { exact: true })).toBeVisible();
  await expect(
    preview.getByRole('checkbox', { name: 'Also restore app preferences' })
  ).not.toBeChecked();
  await page.evaluate(() => {
    document.documentElement.removeAttribute('data-workspace-broadcast');
  });
  await restore.click();
  await expect(options.getByText('Workspace imported.')).toBeVisible();
  expect.soft(await page.locator('html').getAttribute('data-workspace-broadcast')).toBeNull();

  await otherPreview.getByRole('button', { name: 'Import backup' }).click();
  await expect(otherOptions.getByText('Workspace imported.')).toBeVisible();
  await expect(otherOptions.getByText('0 projects · 2 printer profiles')).toBeVisible();
});

test('text and SVG imports marked Tiles build in Free-form and keep their original bytes', async ({
  page,
  browser
}) => {
  await page.addInitScript(() => {
    const create = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (blob) => {
      if (blob instanceof Blob) document.documentElement.dataset.blobType = blob.type;
      return create(blob);
    };
  });
  await page.goto('/');
  await workspaceReady(page);
  const originals = [
    {
      title: 'Letter',
      source: { kind: 'text', text: 'O', font_mm: 30, colour: '#1d2f5c' }
    },
    {
      title: 'Drawing',
      source: { kind: 'svg', name: 'wide.svg', data: Buffer.from(WIDE_SVG).toString('base64') }
    }
  ].map(({ title, source }) => ({
    title,
    bytes: Buffer.from(JSON.stringify({ stencilator: 1, mode: 'tiles', source }))
  }));
  for (const { title, bytes } of originals) {
    await page.getByLabel('Stencilator project file').setInputFiles({
      name: `${title}.stencilator`,
      mimeType: '',
      buffer: bytes
    });
    await expect(page.getByRole('button', { name: title, exact: true })).toBeEnabled();
    await saved(page);
    await page.getByRole('button', { name: 'Files', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Save 3MF' })).toBeEnabled();
    const design = await downloadProject(page);
    expect(await readFile(await design.path())).toEqual(bytes);
    await expect.soft(page.locator('html')).toHaveAttribute('data-blob-type', 'application/json');
  }

  const download = page.waitForEvent('download');
  await (await backups(page, 'Drawing')).getByRole('button', { name: 'Download backup' }).click();
  const backupBytes = await readFile(await (await download).path());
  const fresh = await browser.newPage();
  try {
    await fresh.goto(page.url());
    const dialog = await backups(fresh);
    const font = '**/NotoSans-Bold-*.ttf';
    await fresh.route(font, (route) => route.fulfill({ status: 503 }));
    await importBackup(fresh, backupBytes);
    await expect(dialog.getByRole('alert')).toContainText('Could not check the projects');
    await expect(dialog.getByRole('button', { name: 'Choose backup…' })).toBeEnabled();
    await fresh.unroute(font);
    await importBackup(fresh, backupBytes);
    const preview = fresh.getByRole('dialog', { name: 'Preview import' });
    await expect(preview.getByText('Letter', { exact: true })).toBeVisible();
    await expect(preview.getByText('Drawing', { exact: true })).toBeVisible();
    await expect(preview.getByText(/^Free-form ·/)).toHaveCount(2);
    await preview.getByRole('button', { name: 'Import backup' }).click();
    await expect(fresh.getByText('Workspace imported.')).toBeVisible();
    await fresh.keyboard.press('Escape');
    let current: string | undefined;
    for (const { title, bytes } of originals) {
      const picker = await projects(fresh, current);
      await picker.getByRole('button', { name: new RegExp(`^${title}`) }).click();
      if (title === 'Letter') {
        await expect(fresh.getByRole('textbox', { name: 'Text', exact: true })).toHaveValue('O');
      }
      await fresh.getByRole('button', { name: 'Files', exact: true }).click();
      await expect(fresh.getByRole('button', { name: 'Save 3MF' })).toBeEnabled();
      const design = await downloadProject(fresh);
      expect(await readFile(await design.path())).toEqual(bytes);
      current = title;
    }
  } finally {
    await fresh.close();
  }
});

test('an imported raster project with natural sizing builds at a set width', async ({ page }) => {
  await page.goto('/');
  await workspaceReady(page);
  const original = Buffer.from(
    JSON.stringify({
      stencilator: 1,
      mode: 'freeform',
      source: {
        kind: 'picture',
        name: 'picture.png',
        data: readFileSync(IMAGE).toString('base64')
      },
      freeform: { size: 'natural' }
    })
  );
  await page.getByLabel('Stencilator project file').setInputFiles({
    name: 'Raster.stencilator',
    mimeType: 'application/json',
    buffer: original
  });
  await expect(settings(page).getByRole('button', { name: 'Width', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true'
  );
  await expect(settings(page).getByText(/^Resulting size:/)).toBeVisible();
  await saved(page);
  expect(await readFile(await (await downloadProject(page)).path())).toEqual(original);
  await page.reload();
  await expect(settings(page).getByText(/^Resulting size:/)).toBeVisible();
});

test('an oversized backup import identifies the file that could not be read', async ({ page }) => {
  await page.goto('/');
  const dialog = await backups(page);
  await dialog.getByLabel('Workspace backup file').evaluate((element) => {
    const input = element as HTMLInputElement;
    const file = new File([''], 'Studio archive.json', { type: 'application/json' });
    Object.defineProperty(file, 'size', { value: 161 * 1024 * 1024 });
    // The native FileList exposes a new File wrapper, so override the getter for this test.
    Object.defineProperty(input, 'files', { value: [file] });
    input.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await expect(dialog.getByRole('alert')).toContainText('“Studio archive.json” is too large');
  await expect(dialog.getByRole('button', { name: 'Choose backup…' })).toBeEnabled();
});

test('a backup can leave out an oversized project and keep the rest', async ({ page }) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  // Seed a valid project with enough JSON whitespace to exceed the backup limit.
  await page.evaluate(
    () =>
      new Promise<void>((resolve, reject) => {
        const request = indexedDB.open('stencilator-workspace');
        request.onerror = () => {
          reject(request.error ?? new Error('The workspace database could not open.'));
        };
        request.onsuccess = () => {
          const db = request.result;
          const tx = db.transaction('projects', 'readwrite');
          const store = tx.objectStore('projects');
          const read = store.getAll();
          read.onsuccess = () => {
            const [original] = read.result as { file: Blob }[];
            if (!original) {
              tx.abort();
              return;
            }
            store.put({
              ...original,
              id: 'oversized',
              title: 'Large studio artwork',
              file: new Blob([original.file, ' '.repeat(32 * 1024 * 1024)])
            });
          };
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onabort = () => {
            db.close();
            reject(tx.error ?? new Error('Could not seed the oversized project.'));
          };
        };
      })
  );
  const dialog = await backups(page, 'chart-0');
  await dialog.getByRole('button', { name: 'Download backup', exact: true }).click();
  await expect(dialog.getByText('Large studio artwork', { exact: true })).toBeVisible();
  await expect(dialog.getByText(/32 MiB/)).toContainText('Download project');
  const download = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Download the rest of the workspace' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('stencilator-workspace.json');
  expect(JSON.parse(await readFile(await file.path(), 'utf8'))).toMatchObject({
    projects: [{ title: 'chart-0' }]
  });
  await expect(dialog.getByText('Large studio artwork', { exact: true })).toBeVisible();
});

test('an aborted backup snapshot reports the failure and allows a retry', async ({ page }) => {
  await page.goto('/');
  const dialog = await backups(page);
  await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/unbound-method -- Restored after the one injected abort.
    const get = IDBObjectStore.prototype.get;
    IDBObjectStore.prototype.get = function (key: IDBValidKey | IDBKeyRange) {
      const request = get.call(this, key);
      const tx = this.transaction;
      if (this.name === 'workspace' && tx.mode === 'readonly' && key === 'preferences') {
        IDBObjectStore.prototype.get = get;
        request.addEventListener('success', () => {
          tx.abort();
        });
      }
      return request;
    };
  });
  const button = dialog.getByRole('button', { name: 'Download backup', exact: true });
  await button.click();
  await expect(dialog.getByRole('alert')).toHaveText('Browser storage is unavailable.');
  expect(await savedName(page, () => button.click())).toBe('stencilator-workspace.json');
});

test('two restores can be backed up as separate files when the workspace outgrows one', async ({
  page
}) => {
  await page.goto('/');
  const dialog = await backups(page);
  const profiles = (offset: number) =>
    Array.from({ length: 300 }, (_, index) => ({
      id: `printer-${String(index + offset)}`,
      name: `Printer ${String(index + offset)}`,
      revision: 1,
      widthMm: 200,
      depthMm: 200
    }));
  for (const offset of [0, 300]) {
    await importBackup(
      page,
      Buffer.from(
        JSON.stringify({
          stencilator_workspace: 1,
          projects: [],
          profiles: profiles(offset),
          preferences: { defaults: {}, theme: 'auto', defaultProfileId: null }
        })
      )
    );
    const preview = page.getByRole('dialog', { name: 'Preview import' });
    await expect(preview.getByText('Add', { exact: true })).toHaveCount(300);
    await preview.getByRole('button', { name: 'Import backup' }).click();
    await expect(dialog).toBeVisible();
  }
  await expect(dialog.getByText('0 projects · 600 printer profiles')).toBeVisible();
  await dialog.getByRole('button', { name: 'Download backup', exact: true }).click();
  const files: string[] = [];
  for (const part of [1, 2]) {
    const file = page.waitForEvent('download');
    await dialog.getByRole('button', { name: `Download file ${String(part)} of 2` }).click();
    const downloaded = await file;
    expect(downloaded.suggestedFilename()).toBe(`stencilator-workspace-${String(part)}-of-2.json`);
    files.push(await downloaded.path());
  }
  await expect(dialog.getByText('Backup downloaded.')).toBeVisible();
  for (const [index, path] of files.entries()) {
    await importBackup(page, await readFile(path));
    const preview = page.getByRole('dialog', { name: 'Preview import' });
    await expect(preview.getByText('Skip: already here', { exact: true })).toHaveCount(
      index === 0 ? 500 : 100
    );
    await preview.getByRole('button', { name: 'Back to backup options' }).click();
  }
});

test('a workspace connection opened after a blocked request is closed', async ({ page }) => {
  await page.addInitScript(() => {
    const open = indexedDB.open.bind(indexedDB);
    indexedDB.open = (name, version) => {
      const request = open(name, version);
      if (name === 'stencilator-workspace') {
        indexedDB.open = open;
        request.addEventListener('success', () => {
          const db = request.result;
          const close = db.close.bind(db);
          const root = document.documentElement;
          root.dataset.lateOpen = 'open';
          db.close = () => {
            close();
            root.dataset.lateOpen = 'closed';
          };
        });
        // Simulate the blocked event, then let IndexedDB open a real connection.
        queueMicrotask(() => {
          request.dispatchEvent(new Event('blocked'));
        });
      }
      return request;
    };
  });
  await page.goto('/');
  await expect(page.getByRole('region', { name: 'This tab needs to reload' })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-late-open', 'closed');
});

test('an older tab stops writing when another connection upgrades the database', async ({
  page,
  context
}) => {
  await page.goto('/');
  await openPicture(page);
  await saved(page);
  const other = await context.newPage();
  await other.goto('/');
  await other.evaluate(
    () =>
      new Promise<void>((resolve, reject) => {
        const upgrade = indexedDB.open('stencilator-workspace', 2);
        upgrade.onsuccess = () => {
          upgrade.result.close();
          resolve();
        };
        upgrade.onerror = () => {
          reject(upgrade.error ?? new Error('The database upgrade failed.'));
        };
      })
  );
  const recovery = page.getByRole('region', { name: 'This tab needs to reload' });
  await expect(recovery).toBeVisible();
  await page.getByLabel('Tile (mm)', { exact: true }).fill('7');
  const file = page.waitForEvent('download');
  await recovery.getByRole('button', { name: 'Download project' }).click();
  expect((await readFile(await (await file).path())).toString()).toContain('"tile_mm": 7');
  await expect(recovery.getByRole('button', { name: 'Reload app' })).toBeEnabled();
  const reopened = await context.newPage();
  await reopened.goto('/');
  await expect(reopened.getByRole('region', { name: 'This tab needs to reload' })).toBeVisible();
});

test('an unavailable 3D view offers to download the project and reload', async ({ page }) => {
  await page.goto('/');
  await openText(page);
  await page.getByRole('button', { name: 'Plate', exact: true }).click();
  await page.route('**/PrintPreview-*.js', (route) => route.abort());
  await page.getByRole('button', { name: '3D', exact: true }).click();
  await expect(page.getByText(/The 3D view could not load/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Reload app' })).toBeEnabled();
  expect(
    await savedName(page, () => page.getByRole('button', { name: 'Download project' }).click())
  ).toBe('Text stencil.stencilator');
});

test.describe('narrow workspace', () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

  test('view choices and downloads are reachable, and support placement uses the panned coordinates', async ({
    page
  }) => {
    await page.goto('/');
    await openText(page);
    const panels = page.getByRole('navigation', { name: 'Workspace panels' });
    await panels.getByRole('button', { name: 'Artwork & paints' }).tap();
    await page.getByRole('textbox', { name: 'Text', exact: true }).fill('O');
    await panels.getByRole('button', { name: 'Settings' }).tap();
    await settings(page).getByLabel('About island supports').tap();
    await expect(settings(page).getByText(/Loose pieces, like the centre of an O/)).toBeVisible();
    const supports = settings(page).getByRole('region', { name: 'Island supports' });
    await supports.getByRole('button', { name: 'Raised', exact: true }).tap();
    await supports.getByRole('button', { name: 'Add support', exact: true }).tap();
    await expect(bench(page)).toBeVisible();
    const canvas = page.getByRole('application', { name: 'Canvas view' });
    const placement = canvas.locator('svg rect.target');
    await expect(placement).toBeVisible();
    const before = await placement.boundingBox();
    if (!before) throw new Error('The plate placement target has no box.');
    const x = before.x + before.width / 2;
    const y = before.y + before.height / 2;
    const touch = await page.context().newCDPSession(page);
    await touch.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
    await touch.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ x: x + 12, y: y + 6 }]
    });
    await touch.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ x: x + 24, y: y + 12 }]
    });
    await touch.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await touch.detach();
    const panned = await placement.boundingBox();
    if (!panned) throw new Error('The plate placement target disappeared after panning.');
    expect(panned.x - before.x).toBeCloseTo(24, 0);
    expect(panned.y - before.y).toBeCloseTo(12, 0);
    await canvas.press('+');
    await expect(bench(page).getByRole('status')).toContainText('Place end 1');
    const tap = await placement.evaluate((rect: SVGRectElement) => {
      const xy = new DOMPoint(rect.width.baseVal.value / 2, rect.height.baseVal.value / 2);
      const screen = xy.matrixTransform(rect.getScreenCTM() ?? undefined);
      return { x: screen.x, y: screen.y, mm: xy.x };
    });
    await page.touchscreen.tap(tap.x, tap.y);
    await expect(bench(page).getByRole('status')).toContainText('Place end 2');
    await panels.getByRole('button', { name: 'Settings' }).tap();
    expect(Number(await supports.getByLabel('End 1 X (mm)').inputValue())).toBeCloseTo(tap.mm, 0);
    await panels.getByRole('button', { name: 'Canvas' }).tap();
    await page.touchscreen.tap(tap.x, tap.y);
    await expect(bench(page).getByRole('status')).toContainText('Apply support');
    await bench(page).getByRole('button', { name: 'Apply support' }).tap();
    await expect(bench(page).getByRole('alert')).toBeVisible();
    await bench(page).getByRole('button', { name: 'Cancel', exact: true }).tap();

    await panels.getByRole('button', { name: 'Settings' }).tap();
    await supports.getByRole('button', { name: 'Add support', exact: true }).press('Enter');
    await expect(supports.getByLabel('End 1 X (mm)')).toBeFocused();
    await supports.getByRole('button', { name: 'Cancel', exact: true }).click();
    await panels.getByRole('button', { name: 'Canvas' }).tap();
    await expect(
      page.getByRole('button', { name: 'Painted result', exact: true })
    ).toBeInViewport();
    await page.getByRole('button', { name: 'Files', exact: true }).tap();
    await expect(page.getByRole('button', { name: 'Save 3MF' })).toBeVisible();
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.getByRole('button', { name: 'Back to the artwork' }).tap();
    await expect(canvas).toBeInViewport();
  });

  test('a finger moving before the second lands does not skew a two-finger pan', async ({
    page
  }) => {
    await page.goto('/');
    await openPicture(page);
    const canvas = page.getByRole('application', { name: 'Canvas view' });
    const bounds = await canvas.boundingBox();
    if (!bounds) throw new Error('The viewport has no box.');
    const left = Math.round(bounds.x + bounds.width / 2 - 50);
    const y = Math.round(bounds.y + bounds.height / 2);
    const transform = () =>
      canvas.locator(':scope > div').evaluate((element) => {
        const matrix = new DOMMatrix(getComputedStyle(element).transform);
        return { scale: matrix.a, x: matrix.e };
      });
    const touch = await page.context().newCDPSession(page);
    const a = (offset: number) => ({ x: left + offset, y, id: 1 });
    const b = (offset: number) => ({ x: left + 100 + offset, y, id: 2 });
    await touch.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [a(0)] });
    await touch.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [a(4)] });
    expect(await transform()).toEqual({ scale: 1, x: 0 });
    await touch.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [a(4), b(0)] });
    await touch.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [a(14), b(10)]
    });
    await touch.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await touch.detach();
    const moved = await transform();
    expect(moved.scale).toBeCloseTo(1, 5);
    expect(moved.x).toBeCloseTo(10, 5);

    await canvas.press('0');
    await page.mouse.move(left, y);
    await page.mouse.down();
    await page.mouse.move(left + 4, y);
    expect(await transform()).toEqual({ scale: 1, x: 0 });
    await page.mouse.move(left + 10, y);
    await page.mouse.up();
    expect(await transform()).toEqual({ scale: 1, x: 10 });
  });

  test('two-finger panning preserves zoom at the limits', async ({ page }) => {
    await page.goto('/');
    await openPicture(page);
    const canvas = page.getByRole('application', { name: 'Canvas view' });
    const scene = canvas.locator(':scope > div');
    const transform = () =>
      scene.evaluate((element) => {
        const matrix = new DOMMatrix(getComputedStyle(element).transform);
        return { scale: matrix.a, x: matrix.e };
      });
    const limits = [
      { scale: 10, key: '+', expand: true },
      { scale: 0.3, key: '-', expand: false }
    ];
    for (const { scale, key, expand } of limits) {
      await canvas.press('0');
      for (let step = 0; step < 13; step++) await canvas.press(key);
      const start = await transform();
      expect(start.scale).toBe(scale);
      const bounds = await canvas.boundingBox();
      if (!bounds) throw new Error('The viewport has no box.');
      const left = Math.round(bounds.x + bounds.width / 2 - 50);
      const y = Math.round(bounds.y + bounds.height / 2);
      const points = (a: number, b: number) => [
        { x: left + a, y, id: 1 },
        { x: left + 100 + b, y, id: 2 }
      ];
      const touch = await page.context().newCDPSession(page);
      await touch.send('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: points(0, 0)
      });
      await touch.send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: expand ? points(0, 10) : points(10, 0)
      });
      expect((await transform()).scale).toBe(scale);
      await touch.send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: points(10, 10)
      });
      await touch.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await touch.detach();
      const moved = await transform();
      expect(moved.scale).toBe(scale);
      expect(moved.x - start.x).toBeCloseTo(10, 0);
    }
  });
});
