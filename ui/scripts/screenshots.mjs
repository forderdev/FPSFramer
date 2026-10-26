// Takes the screenshots used in README.md, in demo mode (simulated hardware and game).
//
//   cargo run -p fpsframer-devserver -- --demo --port 5830 --data-dir <empty folder> --ui-dir ui/dist
//   (with FPSFRAMER_DEMO_GAME=1 in the environment)
//   node ui/scripts/screenshots.mjs http://127.0.0.1:5830 docs/images
//
// Uses the installed Chrome (no browser download).
import { chromium } from '@playwright/test';
import { mkdirSync } from 'node:fs';

const base = process.argv[2] ?? 'http://127.0.0.1:5830';
const out = process.argv[3] ?? 'docs/images';
mkdirSync(out, { recursive: true });

const rpc = async (method, params = {}) => {
  const r = await fetch(`${base}/api/rpc`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  const j = await r.json();
  if (j.error) throw new Error(`${method}: ${j.error.message}`);
  return j.ok;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const browser = await chromium.launch({ channel: 'chrome' });
const page = await browser.newPage({ viewport: { width: 1360, height: 860 } });
const shot = async (name, opts = {}) => {
  await sleep(opts.wait ?? 1500);
  await page.screenshot({ path: `${out}/${name}.png`, ...opts.shot });
  console.log('saved', name);
};

// 1. The setup guide, on the overlay style step.
await page.goto(`${base}/`);
const dialog = page.getByRole('dialog', { name: 'Welcome to FPSFramer' });
await dialog.waitFor();
const next = dialog.locator('.modal-foot .btn.primary');
for (let i = 0; i < 20; i++) {
  if (await dialog.getByRole('heading', { name: 'Pick an overlay style' }).isVisible()) break;
  await next.click();
  await sleep(150);
}
await dialog.getByRole('button', { name: 'Gaming' }).click();
await shot('setup', { wait: 800 });
await rpc('app.completeSetup', {});

// 2. A recorded session, so the report has something to show.
// The simulated game is recorded automatically; let it run for a while, then stop.
await sleep(25000);
await rpc('sessions.stopRecording');
const list = await rpc('sessions.list');

// 3. The pages.
await page.goto(`${base}/?x=1#/`);
await shot('home', { wait: 4000 });
await page.goto(`${base}/?x=2#/overlay`);
await shot('overlay', { wait: 2500 });
if (list.length) {
  await page.goto(`${base}/?x=3#/sessions/${list[0].meta.id}`);
  await shot('session', { wait: 2500 });
}
await page.goto(`${base}/?x=4#/editor/preset.gaming`);
await shot('editor', { wait: 2500 });
await page.goto(`${base}/?x=5#/help`);
await shot('help', { wait: 2000 });

await browser.close();
