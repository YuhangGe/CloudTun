import { expect, mock, test } from 'bun:test';

const settings = {
  subscriptionAutoSwitch: false,
  subscriptionCandidates: null as string[] | null,
  subscriptionPolicyUrl: '',
  subscriptionSelectedId: 'current',
  routingMode: 'global',
};
let activeNode = 'current';
let subscriptionUrl = 'https://example.test/first';
let tick: (() => Promise<void>) | undefined;
let backupHealthy = true;
const notices: string[] = [];
const order: string[] = [];
const nodes = ['current', 'fast', 'backup'].map((id) => ({
  id,
  name: id,
  unsupportedReason: null,
}));
Object.assign(globalThis, {
  window: {
    __TAURI_INTERNALS__: {},
    setTimeout(callback: () => Promise<void>) {
      tick = callback;
    },
  },
});
mock.module('jinge', () => ({ vm: <T>(value: T) => value }));
mock.module('jinge-antd', () => ({ message: { error() {} } }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => {} }));
mock.module('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => {} }),
}));
mock.module('@/service/util', () => ({ IS_MOBILE: false, IS_IOS: false, IS_ANDROID: false }));
mock.module('@/service/notify', () => ({
  showNotifyWindow: async ({ notifyType }: { notifyType: string }) => {
    notices.push(notifyType);
  },
}));
mock.module('../src/store/settings', () => ({ globalSettings: settings }));
mock.module('../src/store/instance', () => ({ pauseCloudProxy: () => {} }));
mock.module('../src/store/log', () => ({ appendLog: () => {} }));
mock.module('@/service/desktop', () => ({
  desktopInvoke: async (command: string, args: { nodeId?: string; nodeIds?: string[] } = {}) => {
    switch (command) {
      case 'tauri_get_subscription':
      case 'tauri_update_subscription':
        return { url: subscriptionUrl, nodes, updatedAt: 1 };
      case 'tauri_proxy_status':
        return { running: true, source: 'subscription', nodeId: activeNode, name: activeNode };
      case 'tauri_get_proxy_config':
        return { config: {}, addresses: [] };
      case 'tauri_check_subscription_node':
        order.push('check ' + args.nodeId);
        return args.nodeId === 'backup' && backupHealthy;
      case 'tauri_ping_subscription_nodes':
        return (args.nodeIds ?? []).map((id) => ({ id, latencyMs: id === 'fast' ? 20 : 40 }));
      case 'tauri_connect_subscription':
        activeNode = args.nodeId!;
        order.push('connect ' + activeNode);
    }
  },
}));
const desktop = await import('../src/store/desktop');

test('failover confirms repeated failures, checks candidates and resets when the URL changes', async () => {
  await desktop.initializeDesktop();
  desktop.toggleAutoSwitch();
  expect(settings.subscriptionCandidates).toEqual(['current', 'fast', 'backup']);
  await tick!();
  await tick!();
  expect(activeNode).toBe('current');
  await tick!();
  expect(order.slice(-3)).toEqual(['check fast', 'check backup', 'connect backup']);
  expect(settings.subscriptionSelectedId).toBe('backup');
  expect(notices).toEqual(['success']);

  backupHealthy = false;
  for (let i = 0; i < 6; i++) await tick!();
  expect(activeNode).toBe('backup');
  expect(notices).toEqual(['success', 'error']);

  subscriptionUrl = 'https://example.test/other';
  await desktop.updateSubscription(subscriptionUrl);
  expect(settings.subscriptionAutoSwitch).toBe(false);
  expect(settings.subscriptionCandidates).toBeNull();
});
