import { defineConfig } from '@playwright/test';
import { rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// The tests run against the real engine in demo mode (simulated hardware and a simulated game),
// started by the dev server on a throw-away data folder. Specs run in file order on one server:
// 01 expects a brand-new install, later ones expect setup to be finished.
const port = 5811;
const dataDir = join(tmpdir(), 'fpsframer-e2e-data');
if (!process.env.TEST_WORKER_INDEX) rmSync(dataDir, { recursive: true, force: true });

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [['list']],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    viewport: { width: 1440, height: 900 },
    trace: 'retain-on-failure',
    acceptDownloads: true,
  },
  webServer: {
    command: `cargo run --quiet -p fpsframer-devserver -- --demo --port ${port} --data-dir "${dataDir}" --ui-dir ui/dist`,
    cwd: '..',
    env: { FPSFRAMER_DEMO_GAME: '1' },
    url: `http://127.0.0.1:${port}/`,
    reuseExistingServer: false,
    timeout: 600_000,
    stdout: 'ignore',
  },
});
