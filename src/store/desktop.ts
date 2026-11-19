import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { vm } from 'jinge';

import { desktopInvoke } from '@/service/desktop';
import { IS_MOBILE, IS_ANDROID } from '@/service/util';

import { pauseCloudProxy } from './instance';
import { appendLog } from './log';
import { globalSettings } from './settings';

export type RoutingMode = 'global' | 'bypassChina';
export interface NodeEntry {
  id: string;
  name: string;
  protocol: string;
  address: string;
  port: number;
  unsupportedReason: string | null;
  latencyMs: number | null;
  tested: boolean;
}
export interface ProxyStatus {
  running: boolean;
  source: string | null;
  nodeId: string | null;
  name: string | null;
  mode: RoutingMode;
  systemProxy: boolean;
  error: string | null;
}
interface Subscription {
  url: string;
  nodes: Omit<NodeEntry, 'latencyMs' | 'tested'>[];
  updatedAt: number;
}
export interface ProxyConfig {
  listenHost: '0.0.0.0' | '127.0.0.1';
  port: number;
}
export interface ProxyAddress {
  label: string;
  url: string;
}
interface ProxyConfigView {
  config: ProxyConfig;
  addresses: ProxyAddress[];
}
export const desktop = vm({
  config: { listenHost: '0.0.0.0', port: 7893 } as ProxyConfig,
  addresses: [] as ProxyAddress[],
  savingConfig: false,
  ready: false,
  updating: false,
  testing: false,
  testingNodeId: '',
  connecting: false,
  subscriptionUrl: '',
  updatedAt: 0,
  nodes: [] as NodeEntry[],
  error: '',
  status: {
    running: false,
    source: null,
    nodeId: null,
    name: null,
    mode: 'bypassChina',
    systemProxy: false,
    error: null,
  } as ProxyStatus,
});
let initialized: Promise<void> | undefined;
let testGeneration = 0;
function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
function useSubscription(data: Subscription) {
  desktop.subscriptionUrl = data.url;
  desktop.updatedAt = data.updatedAt;
  desktop.nodes = data.nodes.map((node) => ({ ...node, latencyMs: null, tested: false }));
  if (!data.nodes.some((node) => node.id === globalSettings.subscriptionSelectedId))
    globalSettings.subscriptionSelectedId = '';
}
export async function refreshStatus() {
  desktop.status = await desktopInvoke<ProxyStatus>('tauri_proxy_status');
}
export function initializeDesktop() {
  initialized ??= (async () => {
    if (IS_MOBILE) {
      desktop.ready = true;
      return;
    }
    try {
      const [subscription, status, network] = await Promise.all([
        desktopInvoke<Subscription>('tauri_get_subscription'),
        desktopInvoke<ProxyStatus>('tauri_proxy_status'),
        desktopInvoke<ProxyConfigView>('tauri_get_proxy_config'),
      ]);
      desktop.config = network.config;
      desktop.addresses = network.addresses;
      useSubscription(subscription);
      desktop.status = status;
      desktop.error = '';
      if (status.running && status.source === 'subscription' && status.nodeId)
        globalSettings.subscriptionSelectedId = status.nodeId;
      if ('__TAURI_INTERNALS__' in window) {
        await listen<ProxyStatus>('proxy-status', (event) => {
          desktop.status = event.payload;
        });
      } else if (import.meta.env.DEV) {
        window.setInterval(() => {
          void refreshStatus().catch(() => {});
        }, 1500);
      }
    } catch (error) {
      desktop.error = errorText(error);
    }
    desktop.ready = true;
  })();
  return initialized;
}
export async function testNodes() {
  if (desktop.testing || desktop.testingNodeId) return;
  const generation = ++testGeneration;
  desktop.testing = true;
  try {
    const results = await desktopInvoke<{ id: string; latencyMs: number | null }[]>(
      'tauri_ping_subscription_nodes',
    );
    if (generation !== testGeneration) return;
    for (const result of results) {
      const node = desktop.nodes.find((node) => node.id === result.id);
      if (node) {
        node.latencyMs = result.latencyMs;
        node.tested = true;
      }
    }
  } catch (error) {
    desktop.error = errorText(error);
  } finally {
    if (generation === testGeneration) desktop.testing = false;
  }
}
export async function updateSubscription(url: string) {
  if (IS_MOBILE) return false;
  if (desktop.updating || desktop.connecting || desktop.testing || desktop.testingNodeId)
    return false;
  desktop.updating = true;
  desktop.error = '';
  try {
    const data = await desktopInvoke<Subscription>('tauri_update_subscription', { url });
    useSubscription(data);
    await refreshStatus();
    appendLog(`订阅已更新，共 ${data.nodes.length} 个节点`);
    await testNodes();
    return true;
  } catch (error) {
    desktop.error = errorText(error);
    return false;
  } finally {
    desktop.updating = false;
  }
}
export async function disconnectProxy() {
  pauseCloudProxy();
  if (IS_MOBILE) {
    if (IS_ANDROID) await invoke('tauri_android_stop_vpn');
    return true;
  }
  desktop.connecting = true;
  try {
    await desktopInvoke('tauri_stop_proxy_client');
    await desktopInvoke('tauri_interval_ping_stop');
    await refreshStatus();
    return true;
  } catch (error) {
    desktop.error = errorText(error);
    return false;
  } finally {
    desktop.connecting = false;
  }
}
export async function connectSelected() {
  if (IS_MOBILE) return;
  if (desktop.connecting || !globalSettings.subscriptionSelectedId) return;
  pauseCloudProxy();
  desktop.connecting = true;
  desktop.error = '';
  try {
    await desktopInvoke('tauri_connect_subscription', {
      nodeId: globalSettings.subscriptionSelectedId,
      mode: globalSettings.routingMode,
    });
    await desktopInvoke('tauri_interval_ping_stop');
    if (globalSettings.subscriptionSystemProxy)
      await desktopInvoke('tauri_set_system_proxy', { enabled: true });
    await refreshStatus();
    appendLog('订阅节点代理已连接');
  } catch (error) {
    desktop.error = errorText(error);
    await refreshStatus().catch(() => {});
  } finally {
    desktop.connecting = false;
  }
}
export async function selectNode(node: NodeEntry) {
  if (
    desktop.connecting ||
    node.unsupportedReason ||
    !node.tested ||
    node.latencyMs === null ||
    node.latencyMs >= 500
  )
    return;
  if (globalSettings.subscriptionSelectedId === node.id) return;
  globalSettings.subscriptionSelectedId = node.id;
  if (desktop.status.running && desktop.status.source === 'subscription') {
    await connectSelected();
    if (desktop.status.nodeId) globalSettings.subscriptionSelectedId = desktop.status.nodeId;
  }
}
export async function changeMode(mode: RoutingMode) {
  if (desktop.connecting || desktop.updating) return;
  const previous = globalSettings.routingMode;
  globalSettings.routingMode = mode;
  if (!desktop.status.running) return;
  desktop.connecting = true;
  desktop.error = '';
  try {
    await desktopInvoke('tauri_set_routing_mode', { mode });
    await refreshStatus();
  } catch (error) {
    desktop.error = errorText(error);
    globalSettings.routingMode = previous;
    await refreshStatus().catch(() => {});
  } finally {
    desktop.connecting = false;
  }
}
export async function changeSystemProxy(enabled: boolean) {
  globalSettings.subscriptionSystemProxy = enabled;
  if (!desktop.status.running) return;
  try {
    await desktopInvoke('tauri_set_system_proxy', { enabled });
    await refreshStatus();
  } catch (error) {
    desktop.error = errorText(error);
    globalSettings.subscriptionSystemProxy = desktop.status.systemProxy;
  }
}

export async function refreshProxyConfig() {
  if (IS_MOBILE) return;
  const network = await desktopInvoke<ProxyConfigView>('tauri_get_proxy_config');
  desktop.config = network.config;
  desktop.addresses = network.addresses;
}
export async function saveProxyConfig(config: ProxyConfig) {
  if (IS_MOBILE) throw new Error('移动端本地代理设置将在订阅内核接入后启用');
  desktop.savingConfig = true;
  try {
    const network = await desktopInvoke<ProxyConfigView>('tauri_set_proxy_config', { config });
    desktop.config = network.config;
    desktop.addresses = network.addresses;
  } finally {
    desktop.savingConfig = false;
  }
}

export async function connectCloudProxy(serverIp: string, cvmId: string) {
  if (desktop.connecting || desktop.updating) return false;
  if (
    desktop.status.running &&
    desktop.status.source === 'cloud' &&
    desktop.status.name === serverIp
  ) {
    await desktopInvoke('tauri_interval_ping_start', { ip: serverIp, token: cvmId });
    return true;
  }
  desktop.connecting = true;
  desktop.error = '';
  try {
    await desktopInvoke('tauri_start_proxy_client', {
      serverIp,
      cvmId,
      mode: globalSettings.routingMode,
    });
    await refreshStatus();
    await desktopInvoke('tauri_interval_ping_start', { ip: serverIp, token: cvmId });
    appendLog('云上主机代理已连接');
    return true;
  } catch (error) {
    desktop.error = errorText(error);
    await refreshStatus().catch(() => {});
    return false;
  } finally {
    desktop.connecting = false;
  }
}

export async function testNodeLatency(nodeId: string) {
  if (
    IS_MOBILE ||
    desktop.testing ||
    desktop.testingNodeId ||
    desktop.updating ||
    desktop.connecting
  )
    return;
  desktop.testingNodeId = nodeId;
  desktop.error = '';
  try {
    const results = await desktopInvoke<{ id: string; latencyMs: number | null }[]>(
      'tauri_ping_subscription_nodes',
      { nodeId },
    );
    const result = results.find((result) => result.id === nodeId);
    const node = desktop.nodes.find((node) => node.id === nodeId);
    if (node && result) {
      node.latencyMs = result.latencyMs;
      node.tested = true;
    }
  } catch (error) {
    desktop.error = errorText(error);
  } finally {
    desktop.testingNodeId = '';
  }
}
