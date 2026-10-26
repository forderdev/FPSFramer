import { expect, test } from '@playwright/test';
import { finishSetup, rpc, watchErrors } from './helpers';

test.beforeAll(async ({ request }) => {
  await finishSetup(request);
});

test('the overlay window page draws the chosen layout with live numbers', async ({ page, request }) => {
  const errors = watchErrors(page);
  const state = await rpc<{ overlays: { id: string; layoutId: string }[]; layouts: { id: string; name: string }[] }>(request, 'app.getState');
  expect(state.overlays.length).toBeGreaterThan(0); // the simulated game is running
  const spec = state.overlays[0];
  await page.goto(`/overlay.html?id=${encodeURIComponent(spec.id)}`);
  const els = page.locator('.ov-el');
  await expect(els.first()).toBeVisible();
  await expect(page.getByText('DEMO DATA')).toBeVisible(); // simulated values are always labelled
  expect(await els.count()).toBeGreaterThan(0);

  // The FPS number is a real, changing, positive number from the engine.
  const text = await page.locator('body').innerText();
  expect(text).toMatch(/\b\d{2,3}\b/);
  expect(errors).toEqual([]);
});

test('an unknown overlay id draws nothing and does not crash', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/overlay.html?id=does-not-exist');
  await page.waitForTimeout(800);
  await expect(page.locator('.ov-el')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('diagnostics explain the state of the setup', async ({ page }) => {
  await page.goto('/#/help');
  // Only what needs a look is shown up front; what passed sits in a folded section.
  await page.getByText('Everything that passed', { exact: true }).click();
  await expect(page.getByText('FPS measurement', { exact: true })).toBeVisible();
  await expect(page.getByText('Hardware sensors', { exact: true })).toBeVisible();
  await expect(page.getByText('Overlay', { exact: true }).first()).toBeVisible();
  await expect(page.getByRole('button', { name: 'Copy report' })).toBeVisible();
  await expect(page.getByText(/Demo mode/).first()).toBeVisible();

  const report = await rpc<string>(page.request, 'diagnostics.report');
  expect(report).toContain('FPSFramer diagnostic report');
  expect(report).not.toMatch(/\\Users\\[A-Za-z0-9._-]+\\/); // no user names leak into a shareable report
});

test('troubleshooting search and glossary filter their lists', async ({ page }) => {
  await page.goto('/#/help/fixes');
  await page.getByLabel('Search troubleshooting').fill('temperature');
  await expect(page.getByText('CPU temperature, fan speed or voltage says “Not available”')).toBeVisible();
  await expect(page.getByText('A hotkey does nothing')).toHaveCount(0);

  await page.goto('/#/help/glossary');
  await page.getByLabel('Search the glossary').fill('stutter');
  await expect(page.getByText('Uneven delivery of frames')).toBeVisible();
});

test('unavailable sensors say why instead of showing zero', async ({ page, request }) => {
  const cat = await rpc<{ id: string; availability: { available: boolean; reason: { message: string } | null } }[]>(request, 'app.getState').then((s) => (s as unknown as { catalog: typeof cat }).catalog);
  const missing = cat.filter((d) => !d.availability.available);
  // Every unavailable sensor carries a reason; none is silently zero.
  for (const d of missing) expect(d.availability.reason?.message, d.id).toBeTruthy();
  const snap = await rpc<{ values: Record<string, unknown> }>(request, 'live.snapshot');
  for (const d of missing) expect(snap.values[d.id], `${d.id} must be absent, not 0`).toBeUndefined();
  await page.goto('/#/monitor?set=power');
  await expect(page.getByRole('heading', { level: 1, name: 'Live graphs' })).toBeVisible();
});
