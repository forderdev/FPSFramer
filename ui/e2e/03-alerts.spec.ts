import { expect, test } from '@playwright/test';
import { finishSetup, toast, watchErrors } from './helpers';

test.beforeEach(async ({ request }) => {
  await finishSetup(request);
});

test('add an alert from a ready-made template, switch it off, delete it and undo', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/#/alerts');
  await expect(page.getByRole('heading', { level: 1, name: 'Alerts' })).toBeVisible();

  await page.getByRole('button', { name: 'Add alert' }).click();
  await page.getByRole('button', { name: /Slow frames \(stutter\)/ }).click();

  const editor = page.getByRole('dialog', { name: 'New alert' });
  await expect(editor).toBeVisible();
  await editor.getByLabel('Alert name').fill('My stutter alert');
  await editor.getByLabel('Threshold').fill('45');
  await editor.getByLabel('Threshold').blur();
  await editor.getByRole('button', { name: 'Add alert' }).click();
  await toast(page, 'Alert added');

  const row = page.getByRole('listitem').filter({ hasText: 'My stutter alert' });
  await expect(row).toBeVisible();
  await expect(row).toContainText('goes above 45 ms');

  const sw = row.getByRole('switch', { name: 'My stutter alert enabled' });
  await expect(sw).toHaveAttribute('aria-checked', 'true');
  await sw.click();
  await expect(sw).toHaveAttribute('aria-checked', 'false');

  await row.getByRole('button', { name: 'Delete My stutter alert' }).click();
  await expect(row).toBeHidden();
  await page.locator('.toast').filter({ hasText: 'Deleted' }).getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByRole('listitem').filter({ hasText: 'My stutter alert' })).toBeVisible();
  expect(errors).toEqual([]);
});

test('a custom alert needs a measurement and uses the user’s units', async ({ page, request }) => {
  await request.post('/api/rpc', { data: { method: 'settings.update', params: { patch: { units: { temperature: 'fahrenheit' } } } } });
  await page.goto('/#/alerts');
  await page.getByRole('button', { name: 'Add alert' }).click();
  await page.getByRole('button', { name: 'Build a custom alert' }).click();
  const editor = page.getByRole('dialog', { name: 'New alert' });
  await expect(editor.getByRole('button', { name: 'Add alert' })).toBeDisabled(); // no measurement chosen yet

  await editor.getByRole('button', { name: 'Choose a measurement…' }).click();
  const picker = page.getByRole('dialog', { name: 'Which measurement should this alert watch?' });
  await picker.getByLabel('Search measurements').fill('gpu temperature');
  await picker.getByRole('option').first().click();

  await editor.getByLabel('Threshold').fill('185'); // °F
  await editor.getByLabel('Threshold').blur();
  await editor.getByRole('button', { name: 'Add alert' }).click();
  await toast(page, 'Alert added');
  await expect(page.getByRole('listitem').filter({ hasText: 'GPU temperature alert' })).toContainText('goes above 185 °F');

  // Stored in °C (185 °F = 85 °C), so changing the display unit changes nothing underneath.
  const rules = await (await request.post('/api/rpc', { data: { method: 'rules.list', params: {} } })).json();
  const mine = rules.ok.find((r: { name: string; sensorId: string; threshold: number }) => /alert$/.test(r.name) && r.sensorId.includes('temp') && Math.abs(r.threshold - 85) < 0.01);
  expect(mine).toBeTruthy();
  await request.post('/api/rpc', { data: { method: 'settings.update', params: { patch: { units: { temperature: 'celsius' } } } } });
});
