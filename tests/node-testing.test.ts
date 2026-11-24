import { beforeEach, expect, mock, test } from 'bun:test';

const settings = {
  subscriptionSelectedId: 'current',
  subscriptionAutoSwitch: false,
  subscriptionCandidates: null as string[] | null,
};
const notices: string[] = [];
const calls: string[] = [];
const connected: string[] = [];
let backendStatus = {
  running: false,
  source: null as string | null,
  nodeId: null as string | null,
  name: null as string | null,
  mode: 'global',
  systemProxy: false,
  error: null,
};
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
mock.module('@/service/notify', () => ({
  showNotifyWindow: async ({ notifyMessage }: { notifyMessage: string }) =>
    notices.push(notifyMessage),
}));
mock.module('../src/store/settings', () => ({ globalSettings: settings }));
mock.module('../src/store/instance', () => ({ pauseCloudProxy() {} }));
mock.module('../src/store/log', () => ({ appendLog() {} }));
mock.module('@/service/desktop', () => ({
  desktopInvoke: (command: string, args: { nodeId: string }) => {
    if (command === 'tauri_connect_subscription') {
      connected.push(args.nodeId);
      backendStatus = {
        ...backendStatus,
        running: true,
        source: 'subscription',
        nodeId: args.nodeId,
      };
      return Promise.resolve();
    }
    if (command === 'tauri_proxy_status') return Promise.resolve(backendStatus);
    if (command === 'tauri_stop_proxy_client') {
      backendStatus = { ...backendStatus, running: false, source: null };
      return Promise.resolve();
    }
    if (command === 'tauri_interval_ping_stop') return Promise.resolve();
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
const timers: { fn: () => Promise<void>; delay: number }[] = [];
Object.assign(globalThis, {
  window: {
    setTimeout(fn: () => Promise<void>, delay: number) {
      timers.push({ fn, delay });
      return timers.length;
    },
  },
});
const { desktop, testNodes, cancelSubscriptionRetry, updateSubscription } =
  await import('../src/store/desktop');
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
function finish(id: string, latencyMs: number | null) {
  const request = pending.get(id)!;
  pending.delete(id);
  request.resolve([{ id, latencyMs }]);
}
beforeEach(() => {
  cancelSubscriptionRetry();
  timers.length = 0;
  pending.clear();
  calls.length = 0;
  notices.length = 0;
  connected.length = 0;
  backendStatus = { ...backendStatus, running: false, source: null, nodeId: null };
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
  expect(notices).toEqual(['当前无可用订阅节点，订阅代理已断开']);
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
  expect(notices).toEqual(['当前无可用订阅节点，订阅代理已断开']);
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

test('all unavailable disconnects once and retries after 30 seconds until recovery connects', async () => {
  desktop.status = {
    ...backendStatus,
    running: true,
    source: 'subscription',
    nodeId: 'current',
    name: 'current',
  } as typeof desktop.status;
  const first = testNodes();
  for (const id of pending.keys()) finish(id, null);
  await first;
  expect(desktop.subscriptionUnavailable).toBe(true);
  expect(desktop.status.running).toBe(false);
  expect(timers.at(-1)!.delay).toBe(30_000);
  const retry = timers.at(-1)!.fn();
  for (const id of pending.keys()) finish(id, null);
  await retry;
  expect(notices.length).toBe(1);
  expect(timers.at(-1)!.delay).toBe(30_000);
  const recovered = timers.at(-1)!.fn();
  finish('current', null);
  finish('second', 200);
  finish('third', null);
  await recovered;
  expect(desktop.subscriptionUnavailable).toBe(false);
  expect(connected).toEqual(['second']);
});
test('subscription recovery leaves a cloud proxy running and only notifies', async () => {
  desktop.subscriptionUnavailable = true;
  desktop.status = {
    ...backendStatus,
    running: true,
    source: 'cloud',
    name: '192.0.2.1',
  } as typeof desktop.status;
  const run = testNodes();
  finish('current', null);
  finish('second', 200);
  finish('third', null);
  await run;
  expect(connected).toEqual([]);
  expect(desktop.status.source).toBe('cloud');
  expect(notices).toEqual(['有订阅节点可用：second']);
});
test('updating a subscription invalidates a pending retry even if the update fails', async () => {
  const run = testNodes();
  for (const id of pending.keys()) finish(id, null);
  await run;
  const timer = timers.at(-1)!;
  const count = calls.length;
  await updateSubscription('https://example.test/updated');
  await timer.fn();
  expect(calls.length).toBe(count);
  expect(desktop.subscriptionUnavailable).toBe(false);
});

test('updating while retry requests are in flight discards their results and reconnect actions', async () => {
  desktop.subscriptionUnavailable = true;
  const run = testNodes();
  const oldRequests = new Map(pending);
  await updateSubscription('https://example.test/changed');
  for (const [id, request] of oldRequests) request.resolve([{ id, latencyMs: 100 }]);
  await run;
  expect(desktop.subscriptionUnavailable).toBe(false);
  expect(desktop.testing).toBe(false);
  expect(connected).toEqual([]);
  expect(desktop.nodes.every((node) => node.latencyMs === null)).toBe(true);
});
