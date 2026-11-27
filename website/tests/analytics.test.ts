import { afterEach, expect, test } from 'bun:test';
import { pagePath, startAnalytics } from '../src/lib/analytics';

test('page paths drop the trailing slash and keep the root', () => {
  expect(pagePath('/')).toBe('/');
  expect(pagePath('/docs/ios/')).toBe('/docs/ios');
  expect(pagePath('/zh-CN/docs')).toBe('/zh-CN/docs');
});

// A page in a browser, just enough of one for the client: a location, a
// document that can be hidden, localStorage and sendBeacon.
const saved = new Map<string, PropertyDescriptor | undefined>();
function fakeBrowser(hostname: string, pathname: string) {
  const sent: { url: string; body: string }[] = [];
  const listeners = new Map<string, () => void>();
  const store = new Map<string, string>();
  const document = {
    visibilityState: 'visible',
    addEventListener: (type: string, fn: () => void) => listeners.set(type, fn),
    removeEventListener: () => {},
  };
  const globals: Record<string, unknown> = {
    document,
    location: { hostname, pathname, search: '?q=secret', hash: '#x' },
    localStorage: { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) },
    navigator: { sendBeacon: (url: string, body: string) => (sent.push({ url, body }), true) },
    fetch: async (url: string, init: { body: string }) => (sent.push({ url, body: init.body }), new Response(null, { status: 204 })),
  };
  for (const [key, value] of Object.entries(globals)) {
    saved.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  const hide = () => {
    document.visibilityState = 'hidden';
    listeners.get('visibilitychange')?.();
  };
  return { sent, store, hide };
}

afterEach(() => {
  for (const [key, descriptor] of saved) {
    if (descriptor) Object.defineProperty(globalThis, key, descriptor);
    else delete (globalThis as Record<string, unknown>)[key];
  }
  saved.clear();
});

test('a page view on nsh.dev goes out in one batch when the page is hidden', async () => {
  const page = fakeBrowser('nsh.dev', '/docs/ios/');
  startAnalytics();
  expect(page.sent).toHaveLength(0);
  page.hide();
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(page.sent).toHaveLength(1);
  expect(page.sent[0].url).toBe('https://nsmetry.huacnlee-account.workers.dev/api/events/batch');
  const batch = JSON.parse(page.sent[0].body);
  expect(batch.site).toBe('nsh.dev');
  expect(batch.events).toHaveLength(1);
  expect(batch.events[0].name).toBe('screen_view');
  expect(batch.events[0].url).toBe('/docs/ios');
  expect(batch.events[0].user_id).toBe(page.store.get('nsmetry.uid'));
  expect(page.sent[0].body).not.toContain('secret');
});

test('other hosts (local builds, previews) send nothing', async () => {
  const page = fakeBrowser('localhost', '/docs');
  startAnalytics();
  page.hide();
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(page.sent).toHaveLength(0);
  expect(page.store.size).toBe(0);
});
