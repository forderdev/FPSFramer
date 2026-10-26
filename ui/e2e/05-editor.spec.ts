import { expect, test } from '@playwright/test';
import { finishSetup, toast, watchErrors } from './helpers';

test.beforeEach(async ({ request }) => {
  await finishSetup(request);
});

test('design a layout: add items, undo, save, and see it on the Layouts page', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/#/layouts');
  await page.getByRole('button', { name: 'New layout' }).click();
  const dlg = page.getByRole('dialog', { name: 'New layout' });
  await dlg.getByLabel('Layout name').fill('E2E layout');
  await dlg.getByLabel('Start from').selectOption({ label: 'Minimal' });
  await dlg.getByRole('button', { name: 'Create and edit' }).click();

  await expect(page.getByRole('toolbar', { name: 'Editor toolbar' })).toBeVisible();
  const items = page.locator('.ed-center .ov-el');
  const before = await items.count();
  expect(before).toBeGreaterThan(0);

  // Add a measurement from the popular list and a text item; both appear in the live preview.
  await page.locator('.ed-left .chip').first().click();
  await expect(items).toHaveCount(before + 1);
  await page.getByRole('button', { name: 'Text', exact: true }).click();
  await expect(items).toHaveCount(before + 2);
  await expect(page.getByText('Unsaved changes').or(page.getByText('New copy, not saved yet'))).toBeVisible();

  // Undo / redo, by button and by keyboard.
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(items).toHaveCount(before + 1);
  await page.getByRole('button', { name: 'Redo' }).click();
  await expect(items).toHaveCount(before + 2);
  await page.keyboard.press('Control+z');
  await expect(items).toHaveCount(before + 1);

  // Change the whole-overlay look through the inspector (click an empty spot to select nothing).
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();

  // The saved layout is listed, and leaving with no changes does not ask anything.
  await page.getByRole('button', { name: 'Layouts', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Layouts' })).toBeVisible();
  await expect(page.getByText('E2E layout', { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test('leaving the editor with unsaved changes asks first', async ({ page, request }) => {
  const layouts = await (await request.post('/api/rpc', { data: { method: 'layouts.list', params: {} } })).json();
  const mine = layouts.ok.find((l: { name: string }) => l.name === 'E2E layout');
  await page.goto(`/#/editor/${mine.id}`);
  await expect(page.getByRole('toolbar', { name: 'Editor toolbar' })).toBeVisible();
  await page.getByRole('button', { name: 'Text', exact: true }).click();
  await page.getByRole('button', { name: 'Layouts', exact: true }).click();
  const ask = page.getByRole('dialog', { name: 'Leave without saving?' });
  await expect(ask).toBeVisible();
  await ask.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByRole('toolbar', { name: 'Editor toolbar' })).toBeVisible();
  await page.getByRole('button', { name: 'Revert' }).click();
});

test('built-in layouts are copied, never edited in place; duplicates and export work', async ({ page, request }) => {
  const r = await request.post('/api/rpc', { data: { method: 'layouts.save', params: { layout: { id: 'preset.gaming', name: 'hacked' } } } });
  const body = await r.json();
  expect(body.error?.message).toMatch(/Built-in layouts can't be changed/);

  await page.goto('/#/layouts');
  const card = page.locator('.card').filter({ hasText: 'E2E layout' }).first();
  await card.getByRole('button', { name: /Duplicate/ }).click();
  await toast(page, 'Created “E2E layout (copy)”');
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('.card').filter({ hasText: 'E2E layout' }).first().getByRole('button', { name: /^Export / }).click()]);
  expect(download.suggestedFilename()).toMatch(/\.fpsframer-layout\.json$/);
});
