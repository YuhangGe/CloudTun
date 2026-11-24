import { expect, mock, test } from 'bun:test';

const subscribed: string[] = [];
Object.assign(globalThis, { window: { __TAURI_INTERNALS__: {} } });
mock.module('jinge', () => ({ vm: <T>(value: T) => value }));
mock.module('@tauri-apps/api/event', () => ({
  listen: async (event: string) => {
    subscribed.push(event);
    return () => {};
  },
}));
mock.module('@/service/desktop', () => ({
  desktopInvoke: async () => ({ sessionId: 'test', logs: [] }),
}));

test('cloud loss keeps log subscription without subscribing a main-window error popup', async () => {
  const { initializeLogs } = await import('../src/store/log');
  await initializeLogs();
  expect(subscribed).toEqual(['log-entry']);
});
