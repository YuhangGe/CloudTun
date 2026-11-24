import { expect, mock, test } from 'bun:test';

let timers = 0;
let latencyMs: number | null = null;
const calls: string[] = [];
const notices: string[] = [];
const settings = {
  subscriptionSelectedId: 'a',
  subscriptionAutoSwitch: false,
  subscriptionCandidates: null,
  routingMode: 'global',
};
let status = {
  running: true,
  source: 'subscription',
  nodeId: 'a',
  name: 'A',
  mode: 'global',
  systemProxy: false,
  error: null,
};
Object.assign(globalThis, {
  window: {
    setTimeout() {
      timers++;
      return timers;
    },
  },
});
mock.module('jinge', () => ({ vm: <T>(value: T) => value }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => {} }));
mock.module('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({}) }));
mock.module('@/service/util', () => ({ IS_MOBILE: true, IS_ANDROID: true, IS_IOS: false }));
mock.module('@/service/notify', () => ({
  showNotifyWindow() {
    throw new Error('Expected native notification');
  },
}));
mock.module('../src/store/settings', () => ({ globalSettings: settings }));
mock.module('../src/store/instance', () => ({ pauseCloudProxy() {} }));
mock.module('../src/store/log', () => ({ appendLog() {} }));
mock.module('@/service/desktop', () => ({
  desktopInvoke: async (command: string, args: { text?: string } = {}) => {
    calls.push(command);
    if (command === 'tauri_ping_subscription_nodes') return [{ id: 'a', latencyMs }];
    if (command === 'tauri_stop_proxy_client') status = { ...status, running: false };
    if (command === 'tauri_connect_subscription') status = { ...status, running: true };
    if (command === 'tauri_proxy_status') return status;
    if (command === 'tauri_android_notify') notices.push(args.text!);
  },
}));

test('Android stops on outage, sends a native notification and never schedules retry; manual recovery reconnects', async () => {
  const { desktop, testNodes } = await import('../src/store/desktop');
  desktop.status = status as typeof desktop.status;
  desktop.nodes = [
    {
      id: 'a',
      name: 'A',
      protocol: 'vmess',
      address: 'example.test',
      port: 443,
      unsupportedReason: null,
      latencyMs: 100,
      tested: true,
    },
  ];
  await testNodes();
  expect(desktop.subscriptionUnavailable).toBe(true);
  expect(desktop.status.running).toBe(false);
  expect(calls).toContain('tauri_stop_proxy_client');
  expect(notices).toHaveLength(1);
  expect(timers).toBe(0);
  latencyMs = 100;
  await testNodes();
  expect(desktop.subscriptionUnavailable).toBe(false);
  expect(desktop.status.running).toBe(true);
  expect(calls).toContain('tauri_connect_subscription');
  expect(timers).toBe(0);
});
