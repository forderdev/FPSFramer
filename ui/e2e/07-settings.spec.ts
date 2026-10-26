import { expect, test } from '@playwright/test';
import { finishSetup, rpc, toast, watchErrors } from './helpers';

test.beforeEach(async ({ request }) => {
  await finishSetup(request);
});

test('theme and unit changes apply immediately and persist', async ({ page, request }) => {
  const errors = watchErrors(page);
  await page.goto('/#/settings/appearance');
  await page.getByRole('group', { name: 'Theme' }).getByRole('button', { name: 'Light' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('group', { name: 'Temperature unit' }).getByRole('button', { name: '°F' }).click();
  await page.goto('/#/');
  await expect(page.getByText('°F').first()).toBeVisible();

  const s = await rpc<{ theme: string; units: { temperature: string } }>(request, 'settings.update', { patch: {} });
  expect(s.theme).toBe('light');
  expect(s.units.temperature).toBe('fahrenheit');

  await rpc(request, 'settings.update', { patch: { theme: 'dark', units: { temperature: 'celsius' } } });
  expect(errors).toEqual([]);
});

test('hotkeys can be recorded, must include a modifier and must not clash', async ({ page, request }) => {
  await page.goto('/#/settings/hotkeys');
  const field = page.getByRole('button', { name: /^Show \/ hide the overlay:/ });
  await field.click();
  await expect(field).toContainText('Press the keys');
  await page.keyboard.press('K'); // no modifier → refused with a hint
  await expect(page.getByText(/Include Ctrl, Alt or Shift/)).toBeVisible();
  await page.keyboard.press('Control+Alt+K');
  await expect(page.getByRole('button', { name: /^Show \/ hide the overlay: Ctrl\+Alt\+K/ })).toBeVisible();

  // Using another action's shortcut is refused with a plain explanation.
  const other = page.getByRole('button', { name: /^Take a screenshot:/ });
  await other.click();
  await page.keyboard.press('Control+Alt+K');
  await toast(page, /assigned to more than one action/);

  await rpc(request, 'settings.update', { patch: { hotkeys: { toggleOverlay: 'Ctrl+Shift+F9' } } });
});

test('backup: create, list and restore; reset asks first', async ({ page }) => {
  await page.goto('/#/settings/backup');
  await page.getByRole('button', { name: 'Make a quick backup' }).click();
  await toast(page, 'Backup created');
  await page.getByText('Saved backups', { exact: true }).click(); // the list is folded away until asked for
  await expect(page.getByRole('button', { name: 'Go back to this' }).first()).toBeVisible();

  const [download] = await Promise.all([page.waitForEvent('download'), page.getByRole('button', { name: 'Save a backup file…' }).click()]);
  expect(download.suggestedFilename()).toMatch(/^fpsframer-backup-.*\.json$/);

  await page.getByText('Start over', { exact: true }).click();
  await page.getByRole('button', { name: 'Reset settings' }).click();
  const ask = page.getByRole('dialog', { name: 'Reset all settings?' });
  await expect(ask).toBeVisible();
  await ask.getByRole('button', { name: 'Cancel' }).click();
  await expect(ask).toBeHidden();
});

test('a damaged backup file is refused with a clear message', async ({ request }) => {
  const r = await request.post('/api/rpc', { data: { method: 'backup.import', params: { json: '{"not":"a backup"}' } } });
  const body = await r.json();
  expect(body.error.message).toMatch(/not a valid FPSFramer backup|backup/i);
});

test('advanced mode reveals extra options', async ({ page, request }) => {
  await rpc(request, 'settings.update', { patch: { uiMode: 'simple' } });
  await page.goto('/#/settings/monitoring');
  await expect(page.getByText('How “1% low” is calculated')).toHaveCount(0);
  await page.getByRole('group', { name: 'Interface detail' }).getByRole('button', { name: 'Advanced' }).click();
  await expect(page.getByText('How “1% low” is calculated')).toBeVisible();
});
