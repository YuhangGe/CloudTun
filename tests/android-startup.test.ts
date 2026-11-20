import { expect, mock, test } from 'bun:test';

const events: string[] = [];
mock.module('@/service/util', () => ({ IS_IOS: false, IS_ANDROID: true }));
mock.module('@/store/settings', () => ({
  globalSettings: {
    secretId: '',
    secretKey: '',
    region: '',
    resourceName: '',
    subscriptionSelectedId: 'service-selected-node',
  },
}));
mock.module('@/store/log', () => ({ appendLog() {} }));
mock.module('@/views/proxy/helper', () => ({
  loadInstance: async () => {
    throw new Error('Unexpected cloud request');
  },
}));
mock.module('@/store/desktop', () => ({
  desktop: {
    subscriptionUrl: 'https://subscription.invalid',
    nodes: [],
    status: { running: true },
  },
  initializeDesktop: async () => {
    events.push('observe-service');
  },
  updateSubscription: async () => {
    events.push('refresh');
    return true;
  },
  testNodes: async () => {
    events.push('ping');
  },
  connectSelected: async () => {
    events.push('connect');
  },
}));
const startup = await import('../src/store/startup');

test('recreated Android UI preserves the Service session instead of refreshing and reconnecting', async () => {
  await startup.activateInitialSubscription();
  expect(events).toEqual(['observe-service']);
});
