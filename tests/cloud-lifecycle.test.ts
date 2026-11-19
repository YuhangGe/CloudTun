import { beforeEach, expect, mock, test } from 'bun:test';

import type * as CloudModule from '../src/store/instance';

// Exercise purchase and cancellation without buying a real cloud instance.
const events: string[] = [];
let stopAllowed = true;
let purchase: () => Promise<[null, { InstanceId: string }]>;
let cloud: typeof CloudModule;
const instance = {
  InstanceId: 'test-instance',
  InstanceState: 'RUNNING',
  PublicIpAddresses: ['127.0.0.1'],
};
let readFailure: Error | undefined;
let readiness: (() => Promise<boolean>) | undefined;
const message = { close() {}, update() {} };
Object.assign(globalThis, { window: globalThis });
mock.module('jinge', () => ({ vm: <T>(value: T) => value }));
mock.module('jinge-antd', () => ({
  message: { success: () => message, error: () => message, loading: () => message },
}));
mock.module('@/store/log', () => ({ appendLog: () => {} }));
mock.module('@/service/instance', () => ({ loadInstanceDependentResources: async () => ({}) }));
mock.module('@/service/notify', () => ({ showNotifyWindow: async () => {} }));
mock.module('@/service/tencent', () => ({
  CreateInstance: async () => {
    events.push('purchase');
    return purchase();
  },
}));
mock.module('@/service/util', () => ({ IS_MOBILE: false }));
mock.module('@/views/proxy/helper', () => ({
  loadInstance: async () => {
    events.push('read');
    if (readFailure) throw readFailure;
    return [null, { InstanceSet: [instance] }];
  },
  pingServerOnce: async () => {
    events.push('ready');
    return readiness ? readiness() : true;
  },
}));
mock.module('@/store/settings', () => ({ globalSettings: { proxyRules: '' } }));
mock.module('@/store/desktop', () => ({
  disconnectProxy: async () => {
    events.push('stop');
    cloud.pauseCloudProxy();
    return stopAllowed;
  },
  connectCloudProxy: async () => {
    events.push('connect');
    return true;
  },
}));
cloud = await import('../src/store/instance');
beforeEach(() => {
  cloud.pauseCloudProxy();
  Object.assign(cloud.globalInst, {
    data: undefined,
    ip: undefined,
    loading: false,
    purchasing: false,
    state: 0,
  });
  events.length = 0;
  stopAllowed = true;
  readFailure = undefined;
  readiness = undefined;
  purchase = async () => [null, { InstanceId: instance.InstanceId }];
});

test('viewing or refreshing an existing machine never stops or connects a proxy', async () => {
  await cloud.loadGlobalInst();
  expect(events).toEqual(['read', 'ready']);
  expect(cloud.globalInst.state).toBe(3);
});

test('buying stops the current proxy first and connects automatically after readiness', async () => {
  expect(await cloud.createGlobalInst()).toBe(true);
  expect(events).toEqual(['stop', 'purchase', 'read', 'ready', 'connect']);
  expect(cloud.globalInst.state).toBe(4);
  expect(cloud.globalInst.purchasing).toBe(false);
});

test('failed stop prevents a purchase', async () => {
  stopAllowed = false;
  expect(await cloud.createGlobalInst()).toBe(false);
  expect(events).toEqual(['stop']);
  expect(cloud.globalInst.purchasing).toBe(false);
});

test('a newer proxy choice cancels pending cloud auto-connect and repeated buys are ignored', async () => {
  let finish!: (value: [null, { InstanceId: string }]) => void;
  purchase = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  const pending = cloud.createGlobalInst();
  for (let i = 0; i < 10 && !finish; i++) await Promise.resolve();
  expect(events).toEqual(['stop', 'purchase']);
  expect(await cloud.createGlobalInst()).toBe(false);
  cloud.pauseCloudProxy();
  finish([null, { InstanceId: instance.InstanceId }]);
  await pending;
  expect(events).toEqual(['stop', 'purchase', 'read', 'ready']);
  expect(cloud.globalInst.purchasing).toBe(false);
});

test('a failed instance refresh clears loading and preserves the known machine', async () => {
  cloud.globalInst.data = instance as never;
  readFailure = new Error('cloud timeout');
  await expect(cloud.loadGlobalInst()).rejects.toThrow('cloud timeout');
  expect(cloud.globalInst.loading).toBe(false);
  expect(cloud.globalInst.data?.InstanceId).toBe(instance.InstanceId);
  expect(cloud.globalInst.error).toBe('cloud timeout');
  expect(events).toEqual(['read']);
});

test('instance metadata is shown without waiting for the readiness probe', async () => {
  let finish!: (ready: boolean) => void;
  readiness = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  await cloud.loadGlobalInst();
  expect(cloud.globalInst.loading).toBe(false);
  expect(cloud.globalInst.data?.InstanceId).toBe(instance.InstanceId);
  expect(cloud.globalInst.state).toBe(2);
  finish(true);
  await Promise.resolve();
  await Promise.resolve();
  expect(cloud.globalInst.state).toBe(3);
});

test('startup resumes an existing host without purchasing or explicitly disconnecting', async () => {
  await cloud.resumeExistingCloud(instance as never);
  expect(events).toEqual(['ready', 'connect']);
  expect(cloud.globalInst.state).toBe(4);
});

test('a newer proxy choice cancels a startup connection waiting for readiness', async () => {
  let finish!: (ready: boolean) => void;
  readiness = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  const pending = cloud.resumeExistingCloud(instance as never);
  cloud.pauseCloudProxy();
  finish(true);
  await pending;
  expect(events).toEqual(['ready']);
});
