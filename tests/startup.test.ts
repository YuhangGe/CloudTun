import { beforeEach, expect, mock, test } from 'bun:test';

const events: string[] = [];
const instance = { InstanceId: 'cloud-test', InstanceState: 'RUNNING' };
let instances = [instance];
let readError: Error | undefined;
let updateResult = true;
let replaceNodes: (() => void) | undefined;
const settings = {
  secretId: 'id',
  secretKey: 'key',
  region: 'region',
  resourceName: 'cloudtun',
  subscriptionSelectedId: 'saved',
};
const desktop = {
  subscriptionUrl: 'https://subscription.invalid',
  nodes: [] as { id: string; unsupportedReason: string | null; latencyMs: number | null }[],
  status: { running: false },
};
mock.module('@/service/util', () => ({ IS_MOBILE: false }));
mock.module('@/views/proxy/helper', () => ({
  loadInstance: async () => {
    events.push('cloud-read');
    if (readError) throw readError;
    return [null, { InstanceSet: instances }];
  },
}));
mock.module('@/store/desktop', () => ({
  desktop,
  initializeDesktop: async () => {
    events.push('initialize');
  },
  updateSubscription: async () => {
    events.push('update');
    replaceNodes?.();
    return updateResult;
  },
  testNodes: async () => {
    events.push('test');
  },
  connectSelected: async () => {
    events.push('connect');
  },
}));
mock.module('@/store/log', () => ({ appendLog: () => {} }));
mock.module('@/store/settings', () => ({ globalSettings: settings }));
let run = 0;
async function fresh() {
  return await import(`../src/store/startup.ts?test=${++run}`);
}
beforeEach(() => {
  events.length = 0;
  instances = [];
  readError = undefined;
  updateResult = true;
  replaceNodes = undefined;
  settings.secretId = 'id';
  settings.subscriptionSelectedId = 'saved';
  desktop.subscriptionUrl = 'https://subscription.invalid';
  desktop.nodes = [{ id: 'saved', unsupportedReason: null, latencyMs: 100 }];
  desktop.status.running = false;
});
test('existing cloud host has startup priority and later subscription refresh does not take over', async () => {
  instances = [instance];
  const startup = await fresh();
  expect((await startup.initializeStartup()).view).toBe('cloud');
  await startup.activateInitialSubscription();
  await startup.activateInitialSubscription();
  expect(events).toEqual(['initialize', 'cloud-read', 'update']);
});
test('no host: refresh once and reconnect the remembered available node', async () => {
  const startup = await fresh();
  expect((await startup.initializeStartup()).view).toBe('nodes');
  await Promise.all([startup.activateInitialSubscription(), startup.activateInitialSubscription()]);
  expect(events).toEqual(['initialize', 'cloud-read', 'update', 'connect']);
});
test.each(['missing', 'unsupported', 'slow', 'untested'])(
  'does not connect when the remembered node is %s',
  async (reason) => {
    replaceNodes = () => {
      if (reason === 'missing') desktop.nodes = [];
      if (reason === 'unsupported') desktop.nodes[0].unsupportedReason = 'unsupported';
      if (reason === 'slow') desktop.nodes[0].latencyMs = 500;
      if (reason === 'untested') desktop.nodes[0].latencyMs = null;
    };
    await (await fresh()).activateInitialSubscription();
    expect(events).not.toContain('connect');
  },
);
test('does not reconnect after failed update or replace a running proxy', async () => {
  updateResult = false;
  await (await fresh()).activateInitialSubscription();
  expect(events).not.toContain('connect');
  updateResult = true;
  desktop.status.running = true;
  await (await fresh()).activateInitialSubscription();
  expect(events).not.toContain('connect');
});
test('missing subscription only tests cached nodes and never updates or connects', async () => {
  desktop.subscriptionUrl = '';
  await (await fresh()).activateInitialSubscription();
  expect(events).toEqual(['initialize', 'cloud-read', 'test']);
});
test('cloud query failure falls back to subscription without purchasing anything', async () => {
  readError = new Error('timeout');
  const startup = await fresh();
  expect((await startup.initializeStartup()).view).toBe('nodes');
  await startup.activateInitialSubscription();
  expect(events).toEqual(['initialize', 'cloud-read', 'update', 'connect']);
});
