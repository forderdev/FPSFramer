import { expect, test } from '@playwright/test';
import { finishSetup, toast, watchErrors } from './helpers';

test.beforeEach(async ({ request }) => {
  await finishSetup(request);
});

test('create a game profile, give it a program and a target, duplicate and delete it', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/#/profiles');
  await expect(page.getByRole('heading', { level: 1, name: 'Games & profiles' })).toBeVisible();

  await page.getByRole('button', { name: 'Add a game' }).click();
  const dlg = page.getByRole('dialog', { name: 'Add a game' });
  await dlg.getByLabel('Game name').fill('Test Racer');
  await dlg.getByLabel('Program name').fill('testracer');
  await dlg.getByRole('button', { name: 'Create profile' }).click();
  await toast(page, 'Profile created for Test Racer');

  await expect(page.getByLabel('Profile name')).toHaveValue('Test Racer');
  await expect(page.getByText('testracer.exe', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: '144', exact: true }).click();
  await expect(page.getByRole('button', { name: '144', exact: true })).toHaveAttribute('aria-pressed', 'true');

  // Another program can be added, and one program can only belong to one profile.
  await page.getByLabel('Add a program name').fill('testracer2.exe');
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page.getByText('testracer2.exe', { exact: true })).toBeVisible();

  // The profile exists in the engine with those values.
  const list = await (await page.request.post('/api/rpc', { data: { method: 'profiles.list', params: {} } })).json();
  const mine = list.ok.find((p: { name: string }) => p.name === 'Test Racer');
  expect(mine.targetFps).toBe(144);
  expect(mine.executables).toEqual(['testracer.exe', 'testracer2.exe']);

  await page.getByRole('button', { name: 'Duplicate', exact: true }).last().click();
  await toast(page, 'Created “Test Racer (copy)”');

  // Delete the copy (asks first).
  await page.getByRole('button', { name: 'Delete', exact: true }).last().click();
  const confirm = page.getByRole('dialog', { name: /Delete “Test Racer \(copy\)”\?/ });
  await expect(confirm).toBeVisible();
  await confirm.getByRole('button', { name: 'Delete profile' }).click();
  await expect(page.getByRole('button', { name: 'Test Racer (copy)' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /^Test Racer/ })).toHaveCount(1);
  expect(errors).toEqual([]);
});

test('the “All games” profile cannot be deleted and its program list is not editable', async ({ page }) => {
  await page.goto('/#/profiles/default');
  await expect(page.getByLabel('Profile name')).toHaveValue('All games');
  await expect(page.getByRole('button', { name: 'Delete', exact: true })).toHaveCount(0);
  await expect(page.getByText('Programs this profile applies to')).toHaveCount(0);
});

test('a program cannot belong to two profiles', async ({ page }) => {
  await page.goto('/#/profiles');
  await page.getByRole('button', { name: 'Add a game' }).click();
  const dlg = page.getByRole('dialog', { name: 'Add a game' });
  await dlg.getByLabel('Game name').fill('Second Racer');
  await dlg.getByLabel('Program name').fill('testracer.exe'); // already owned by "Test Racer"
  await dlg.getByRole('button', { name: 'Create profile' }).click();
  await toast(page, /already belongs to the profile/);
});

test('export produces a file that can be imported again', async ({ page }) => {
  await page.goto('/#/profiles');
  await page.getByRole('button', { name: /^Test Racer$/ }).click();
  const [download] = await Promise.all([page.waitForEvent('download'), page.getByRole('button', { name: 'Export…' }).click()]);
  expect(download.suggestedFilename()).toMatch(/\.fpsframer-profile\.json$/);
  const path = await download.path();
  expect(path).toBeTruthy();

  await page.goto('/#/profiles');
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.getByRole('button', { name: 'Import…' }).click()]);
  await chooser.setFiles(path!);
  await toast(page, /Imported “Test Racer”/);
});
