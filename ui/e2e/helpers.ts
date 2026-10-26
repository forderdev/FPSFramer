import AxeBuilder from '@axe-core/playwright';
import { expect, type APIRequestContext, type Page } from '@playwright/test';

/** Call the engine directly (the same JSON-RPC the UI uses). */
export async function rpc<T = unknown>(request: APIRequestContext, method: string, params: Record<string, unknown> = {}): Promise<T> {
  const res = await request.post('/api/rpc', { data: { method, params } });
  const body = await res.json();
  if (body.error) throw new Error(`${method}: ${body.error.message}`);
  return body.ok as T;
}

/** Make sure the first-run guide is out of the way. */
export async function finishSetup(request: APIRequestContext): Promise<void> {
  await rpc(request, 'app.completeSetup', {});
}

/** Collect anything the page logs as an error, so a test can assert there was none. */
export function watchErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => {
    if (m.type() !== 'error') return;
    const t = m.text();
    // Aborted fetches while navigating away are not bugs.
    if (/Failed to load resource.*(ERR_ABORTED|net::ERR)/i.test(t)) return;
    errors.push(`console: ${t}`);
  });
  return errors;
}

/** Fail on serious accessibility problems (contrast, names, roles). */
export async function expectAccessible(page: Page, context: string): Promise<void> {
  const results = await new AxeBuilder({ page: page as never }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
  const bad = results.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
  const summary = bad.map((v) => `${v.id} (${v.impact}): ${v.help}\n   ${v.nodes.slice(0, 3).map((n) => n.target.join(' ')).join('\n   ')}`).join('\n');
  expect(bad, `${context}: accessibility problems\n${summary}`).toEqual([]);
}

export async function toast(page: Page, text: string | RegExp) {
  await expect(page.locator('.toast').filter({ hasText: text }).first()).toBeVisible();
}
