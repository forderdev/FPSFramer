import { expect, test } from '@playwright/test';
import { expectAccessible, watchErrors } from './helpers';

test('a new install walks through the setup guide and lands on Home', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/');
  const dialog = page.getByRole('dialog', { name: 'Welcome to FPSFramer' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('heading', { name: 'See how your games really run' })).toBeVisible();

  const next = dialog.locator('.modal-foot .btn.primary');
  const heading = (name: string) => dialog.getByRole('heading', { name });
  /** The optional steps (second graphics card, refresh-rate and battery goals) only exist on some PCs. */
  const skipOptional = async (name: string) => {
    if (await heading(name).isVisible()) await next.click();
  };

  // Every step asks for at most two things.
  await expectAccessible(page, 'setup guide: welcome');
  await next.click(); // Your PC
  await expect(heading('Your PC')).toBeVisible();
  await expect(dialog.getByText('Processor')).toBeVisible();
  await next.click();
  await skipOptional('Which graphics card runs your games?');
  await skipOptional('Your goals');
  await expect(heading('Permission to measure FPS')).toBeVisible();
  await next.click(); // style
  await expect(heading('Pick an overlay style')).toBeVisible();
  await dialog.getByRole('button', { name: 'Minimal' }).click();
  await expect(dialog.getByRole('button', { name: 'Minimal' })).toHaveAttribute('aria-pressed', 'true');
  await expectAccessible(page, 'setup guide: overlay style');
  await expect(dialog.locator('.ov-el').first()).toBeVisible(); // live preview of the chosen layout
  await next.click();
  await expect(heading('Overlay and detail')).toBeVisible();
  await next.click();
  await expect(heading('Shortcuts')).toBeVisible();
  await expectAccessible(page, 'setup guide: shortcuts');
  await next.click();
  await expect(heading('Startup')).toBeVisible();
  await next.click();
  await expect(heading('You’re all set')).toBeVisible();
  await dialog.getByRole('button', { name: /Finish and open FPSFramer/ }).click();

  await expect(dialog).toBeHidden();
  await expect(page.getByRole('heading', { name: 'Home', level: 1 })).toBeVisible();

  // The chosen style was applied to the overlay of the default profile.
  const state = await (await page.request.post('/api/rpc', { data: { method: 'app.getState', params: {} } })).json();
  expect(state.ok.firstRun).toBe(false);
  const def = state.ok.profiles.find((p: { kind: string }) => p.kind === 'default');
  const scene = def.scenes.find((s: { id: string }) => s.id === def.activeSceneId);
  expect(scene.overlays[0].layoutId).toBe('preset.minimal');
  expect(errors).toEqual([]);
});

test('closing the guide early still counts as done', async ({ page }) => {
  // After the first test setup is complete; re-open the guide from Settings and skip it.
  await page.goto('/#/settings/general');
  await page.getByText('More options', { exact: true }).click(); // folded away until asked for
  await page.getByRole('button', { name: 'Open setup guide' }).click();
  const dialog = page.getByRole('dialog', { name: 'Welcome to FPSFramer' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Skip setup' }).click();
  await expect(dialog).toBeHidden();
});
