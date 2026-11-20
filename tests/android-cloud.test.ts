import { expect, mock, test } from 'bun:test';

// Model Android's cloud lifecycle without making any billable API calls.
const events: string[] = [];
Object.assign(globalThis, { window: globalThis });
const settings = {
  region: 'test-region',
  resourceName: 'test-cloud',
  bandWidth: 200,
  secretId: 'test-id',
  secretKey: 'test-key',
  imageId: 'test-image',
  zone: 'test-zone',
  instanceType: 'test-type',
  diskType: 'test-disk',
  loginPwd: '',
  subscriptionCandidates: null,
};
const instance = {
  InstanceId: 'android-test-instance',
  InstanceState: 'RUNNING',
  PublicIpAddresses: ['192.0.2.1'],
};
const notification = { close() {}, update() {} };
mock.module('jinge', () => ({ vm: <T>(value: T) => value }));
mock.module('jinge-antd', () => ({
  message: {
    success: () => notification,
    error: () => notification,
    loading: () => notification,
  },
}));
mock.module('@/service/util', () => ({
  IS_MOBILE: true,
  IS_IOS: false,
  generateStrongPassword: () => 'test-generated-password',
}));
mock.module('@/store/settings', () => ({ globalSettings: settings }));
mock.module('@/store/log', () => ({ appendLog() {} }));
mock.module('@/service/notify', () => ({ showNotifyWindow: async () => {} }));
mock.module('@/service/instance', () => ({ loadInstanceDependentResources: async () => ({}) }));
mock.module('@/service/tencent', () => ({
  CreateInstance: async () => {
    events.push('purchase');
    return [null, { InstanceId: instance.InstanceId }];
  },
}));
mock.module('@/views/proxy/helper', () => ({
  loadInstance: async () => [null, { InstanceSet: [instance] }],
  pingServerOnce: async () => {
    events.push('ready');
    return true;
  },
}));
mock.module('@/store/desktop', () => ({
  disconnectProxy: async () => {
    events.push('stop');
    return true;
  },
  connectCloudProxy: async () => {
    events.push('vpn');
    return true;
  },
}));
const cloud = await import('../src/store/instance');

test('Android purchase connects the VPN after the cloud host becomes ready', async () => {
  expect(await cloud.createGlobalInst()).toBe(true);
  expect(events).toEqual(['stop', 'purchase', 'ready', 'vpn']);
  expect(settings.loginPwd).toBe('test-generated-password');
  expect(cloud.globalInst.state).toBe(4);
  expect(cloud.globalInst.purchasing).toBe(false);
});

test('Android startup reconnects an existing ready cloud host without purchasing', async () => {
  events.length = 0;
  cloud.pauseCloudProxy();
  await cloud.resumeExistingCloud(instance as never);
  expect(events).toEqual(['ready', 'vpn']);
  expect(cloud.globalInst.state).toBe(4);
});
