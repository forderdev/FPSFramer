import { expect, test } from '@playwright/test';
import { finishSetup, rpc, watchErrors } from './helpers';

/** Record a short session from the simulated game (the one recording automatically is ended first). */
async function recordSession(request: import('@playwright/test').APIRequestContext, seconds: number) {
  await rpc(request, 'sessions.stopRecording').catch(() => undefined);
  await rpc(request, 'sessions.startRecording');
  await expect.poll(async () => (await rpc<{ session: unknown }>(request, 'live.snapshot')).session !== null, { timeout: 15_000 }).toBe(true);
  await new Promise((r) => setTimeout(r, seconds * 1000));
  await rpc(request, 'sessions.stopRecording');
}

test.beforeAll(async ({ request }) => {
  await finishSetup(request);
  await rpc(request, 'settings.update', { patch: { recording: { minDurationS: 0 }, uiMode: 'advanced' } });
  await recordSession(request, 4);
  await recordSession(request, 4);
});

test('recorded sessions are listed, open with a plain-language report, and export', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/#/sessions');
  const rows = page.locator('tbody tr');
  await expect(rows.first()).toBeVisible();
  expect(await rows.count()).toBeGreaterThanOrEqual(2);

  await rows.first().getByRole('link').click();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await expect(page.getByText('What happened')).toBeVisible();
  await expect(page.getByText('Frame-time distribution')).toBeVisible();
  await expect(page.getByText('This PC at the time')).toBeVisible();

  // Findings are labelled with how certain they are, never presented as a diagnosis.
  await expect(page.getByText(/Measured|Likely|Possible/).first()).toBeVisible();

  const [csv] = await Promise.all([page.waitForEvent('download'), page.getByRole('button', { name: /CSV/ }).click()]);
  expect(csv.suggestedFilename()).toMatch(/\.csv$/);
  expect(errors).toEqual([]);
});

test('star, rename and delete with undo', async ({ page }) => {
  await page.goto('/#/sessions');
  await page.locator('tbody tr').first().getByRole('link').click();
  await page.getByRole('button', { name: 'Star', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Starred' })).toBeVisible();
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await page.locator('.toast').filter({ hasText: 'Deleted' }).getByRole('button', { name: 'Undo' }).click();
  await page.goto('/#/sessions');
  await page.getByRole('button', { name: 'Starred only' }).click();
  await expect(page.locator('tbody tr')).toHaveCount(1);
});

test('two sessions can be compared', async ({ page }) => {
  await page.goto('/#/sessions');
  const boxes = page.locator('tbody input[type=checkbox]');
  await boxes.nth(0).check();
  await boxes.nth(1).check();
  await page.getByRole('button', { name: /Compare selected/ }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Compare sessions' })).toBeVisible();
  await expect(page.getByText('In short')).toBeVisible();
  await expect(page.locator('table.table')).toContainText('Average FPS');
});
