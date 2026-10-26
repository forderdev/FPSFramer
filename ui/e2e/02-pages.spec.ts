import { expect, test } from '@playwright/test';
import { expectAccessible, finishSetup, rpc, watchErrors } from './helpers';

const ROUTES: { path: string; heading: string }[] = [
  { path: '/', heading: 'Home' },
  { path: '/monitor', heading: 'Live graphs' },
  { path: '/sessions', heading: 'Sessions' },
  { path: '/overlay', heading: 'Overlay' },
  { path: '/layouts', heading: 'Layouts' },
  { path: '/profiles', heading: 'Games & profiles' },
  { path: '/alerts', heading: 'Alerts' },
  { path: '/captures', heading: 'Screenshots' },
  { path: '/help', heading: 'Help & diagnostics' },
  { path: '/help/fixes', heading: 'Help & diagnostics' },
  { path: '/help/sensors', heading: 'Help & diagnostics' },
  { path: '/help/permission', heading: 'Help & diagnostics' },
  { path: '/help/compat', heading: 'Help & diagnostics' },
  { path: '/help/glossary', heading: 'Help & diagnostics' },
  { path: '/settings', heading: 'Settings' },
  { path: '/settings/appearance', heading: 'Settings' },
  { path: '/settings/monitoring', heading: 'Settings' },
  { path: '/settings/hotkeys', heading: 'Settings' },
  { path: '/settings/overlay', heading: 'Settings' },
  { path: '/settings/backup', heading: 'Settings' },
  { path: '/settings/about', heading: 'Settings' },
];

test.beforeAll(async ({ request }) => {
  await finishSetup(request);
});

for (const theme of ['dark', 'light', 'high-contrast'] as const) {
  test.describe(`${theme} theme`, () => {
    test.beforeAll(async ({ request }) => {
      await rpc(request, 'settings.update', { patch: { theme, uiMode: 'advanced' } });
    });

    for (const r of ROUTES) {
      test(`${r.path} renders without errors and is accessible`, async ({ page }) => {
        const errors = watchErrors(page);
        await page.goto(`/#${r.path}`);
        await expect(page.getByRole('heading', { level: 1, name: r.heading })).toBeVisible();
        await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
        await page.waitForTimeout(600); // let live data and charts draw
        await expectAccessible(page, `${r.path} (${theme})`);
        expect(errors).toEqual([]);
      });
    }
  });
}

test('pages do not scroll sideways at a narrow window width', async ({ page, request }) => {
  await rpc(request, 'settings.update', { patch: { theme: 'dark' } });
  await page.setViewportSize({ width: 960, height: 700 });
  for (const r of ROUTES.filter((x) => !x.path.includes('help/') && !x.path.includes('settings/'))) {
    await page.goto(`/#${r.path}`);
    await expect(page.getByRole('heading', { level: 1, name: r.heading })).toBeVisible();
    await page.waitForTimeout(300);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow, `${r.path} overflows horizontally`).toBeLessThanOrEqual(1);
  }
});

test('the interface scales with the text-size setting', async ({ page, request }) => {
  await rpc(request, 'settings.update', { patch: { uiScale: 1.4 } });
  await page.goto('/#/');
  await expect(page.getByRole('heading', { level: 1, name: 'Home' })).toBeVisible();
  await expect(page.locator('html')).toHaveCSS('font-size', '22.4px');
  await rpc(request, 'settings.update', { patch: { uiScale: 1 } });
});
