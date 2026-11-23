import { beforeEach, expect, mock, test } from 'bun:test';

const settings = {
  subscriptionSelectedId: 'current',
  subscriptionAutoSwitch: false,
  subscriptionCandidates: null as string[] | null,
};
const notices: string[] = [];
const calls: string[] = [];
const pending = new Map<
  string,
  {
    resolve: (result: { id: string; latencyMs: number | null }[]) => void;
    reject: (error: Error) => void;
  }
>();
mock.module('jinge', () => ({ vm: <T>(value: T) => value }));
mock.module('jinge-antd', () => ({ message: { error: (text: string) => notices.push(text) } }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => {} }));
mock.module('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({}) }));
mock.module('@/service/util', () => ({ IS_MOBILE: false, IS_ANDROID: false, IS_IOS: false }));
mock.module('@/service/notify', () => ({ showNotifyWindow: async () => {} }));
mock.module('../src/store/settings', () => ({ globalSettings: settings }));
mock.module('../src/store/instance', () => ({ pauseCloudProxy() {} }));
mock.module('../src/store/log', () => ({ appendLog() {} }));
mock.module('@/service/desktop', () => ({
  desktopInvoke: (command: string, args: { nodeId: string }) => {
    if (command !== 'tauri_ping_subscription_nodes') throw new Error(`Unexpected ${command}`);
    calls.push(args.nodeId);
    return new Promise((resolve, reject) =>
      pending.set(args.nodeId, {
        resolve: resolve as (result: { id: string; latencyMs: number | null }[]) => void,
        reject,
      }),
    );
  },
}));
const { desktop, testNodes } = await import('../src/store/desktop');
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
function finish(id: string, latencyMs: number | null) {
  const request = pending.get(id)!;
  pending.delete(id);
  request.resolve([{ id, latencyMs }]);
}
beforeEach(() => {
  pending.clear();
  calls.length = 0;
  notices.length = 0;
  settings.subscriptionSelectedId = 'current';
  settings.subscriptionAutoSwitch = false;
  settings.subscriptionCandidates = null;
  desktop.testing = false;
  desktop.testingNodeId = '';
  desktop.connecting = false;
  desktop.error = '';
  desktop.status.running = false;
  desktop.nodes = ['current', 'second', 'third'].map((id) => ({
    id,
    name: id,
    protocol: 'vmess',
    address: 'example.test',
    port: 443,
    unsupportedReason: null,
    latencyMs: 100,
    tested: true,
  }));
});
test('clears old results and publishes each completion without waiting for the slowest node', async () => {
  const run = testNodes();
  expect(desktop.nodes.every((node) => !node.tested && node.latencyMs === null)).toBe(true);
  finish('second', 1200);
  await flush();
  expect(desktop.nodes[1].latencyMs).toBe(1200);
  expect(desktop.nodes[1].tested).toBe(true);
  expect(desktop.nodes[0].tested).toBe(false);
  expect(desktop.testing).toBe(true);
  finish('current', 50);
  finish('third', null);
  await run;
  expect(desktop.testing).toBe(false);
  expect(settings.subscriptionSelectedId).toBe('current');
  expect(notices).toEqual([]);
});
test('selects the first available node after the current one fails', async () => {
  const run = testNodes();
  finish('third', 10);
  finish('current', null);
  finish('second', 1200);
  await run;
  expect(settings.subscriptionSelectedId).toBe('second');
});
test('automatic switching uses candidate order and excludes other available nodes', async () => {
  settings.subscriptionAutoSwitch = true;
  settings.subscriptionCandidates = ['third'];
  const run = testNodes();
  finish('current', null);
  finish('second', 10);
  finish('third', 1200);
  await run;
  expect(settings.subscriptionSelectedId).toBe('third');
});
test('no available candidate shows message.error even if an excluded node works', async () => {
  settings.subscriptionAutoSwitch = true;
  settings.subscriptionCandidates = ['third'];
  const run = testNodes();
  finish('current', null);
  finish('second', 10);
  finish('third', null);
  await run;
  expect(notices).toEqual(['当前无可用节点']);
});
test('a request failure finishes its loading state and does not block remaining nodes', async () => {
  const run = testNodes();
  pending.get('current')!.reject(new Error('connection lost'));
  pending.delete('current');
  finish('second', null);
  finish('third', null);
  await run;
  expect(desktop.nodes.every((node) => node.tested)).toBe(true);
  expect(desktop.testing).toBe(false);
  expect(notices).toEqual(['当前无可用节点']);
});
test('keeps concurrency at six while queued nodes also show loading', async () => {
  desktop.nodes.push(
    ...['four', 'five', 'six', 'seven'].map((id) => ({ ...desktop.nodes[0], id })),
  );
  const run = testNodes();
  expect(calls.length).toBe(6);
  expect(desktop.nodes.every((node) => !node.tested)).toBe(true);
  finish('second', 50);
  await flush();
  expect(calls.length).toBe(7);
  for (const id of pending.keys()) finish(id, 100);
  await run;
  expect(desktop.nodes.every((node) => node.tested)).toBe(true);
});
