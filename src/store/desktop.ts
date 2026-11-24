import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { vm } from 'jinge';
import { message } from 'jinge-antd';

import { desktopInvoke } from '@/service/desktop';
import { showNotifyWindow } from '@/service/notify';
import { IS_MOBILE, IS_ANDROID, IS_IOS } from '@/service/util';

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
const defaultSubscriptionUrl = import.meta.env.VITE_SUBSCRIPTION_URL?.trim() ?? '';

export const desktop = vm({
  config: { listenHost: '0.0.0.0', port: 7893 } as ProxyConfig,
  addresses: [] as ProxyAddress[],
  savingConfig: false,
  ready: false,
  updating: false,
  testing: false,
  testingNodeId: '',
  connecting: false,
  subscriptionUrl: defaultSubscriptionUrl,
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
let failoverRevision = 0;
let monitoredNode: string | null = null;
let failures = 0;
let notified = false;
function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
function useSubscription(data: Subscription) {
  const url = data.url.trim() || defaultSubscriptionUrl;
  if (globalSettings.subscriptionPolicyUrl !== url) {
    globalSettings.subscriptionAutoSwitch = false;
    globalSettings.subscriptionCandidates = null;
    globalSettings.subscriptionPolicyUrl = '';
    ++failoverRevision;
  }
  desktop.subscriptionUrl = url;
  desktop.updatedAt = data.updatedAt;
  desktop.nodes = data.nodes.map((node) => ({ ...node, latencyMs: null, tested: false }));
  if (!data.nodes.some((node) => node.id === globalSettings.subscriptionSelectedId))
    globalSettings.subscriptionSelectedId = '';
}
export function toggleAutoSwitch() {
  globalSettings.subscriptionPolicyUrl = desktop.subscriptionUrl;
  if (!globalSettings.subscriptionAutoSwitch && globalSettings.subscriptionCandidates === null) {
    globalSettings.subscriptionCandidates = desktop.nodes.map(({ id }) => id);
  }
  globalSettings.subscriptionAutoSwitch = !globalSettings.subscriptionAutoSwitch;
  ++failoverRevision;
  monitoredNode = null;
  void syncAndroidFailover();
}

export function toggleAutoSwitchCandidate(id: string) {
  const candidates = globalSettings.subscriptionCandidates ?? [];
  globalSettings.subscriptionCandidates = candidates.includes(id)
    ? candidates.filter((candidate) => candidate !== id)
    : [...candidates, id];
  ++failoverRevision;
  monitoredNode = null;
  void syncAndroidFailover();
}

async function syncAndroidFailover() {
  if (!IS_ANDROID) return;
  try {
    await desktopInvoke('tauri_android_set_failover', {
      enabled: globalSettings.subscriptionAutoSwitch,
      nodeIds: globalSettings.subscriptionCandidates ?? [],
    });
  } catch (error) {
    desktop.error = errorText(error);
  }
}

function applyStatus(status: ProxyStatus) {
  desktop.status = status;
  if (IS_ANDROID && status.running && status.source === 'subscription' && status.nodeId)
    globalSettings.subscriptionSelectedId = status.nodeId;
}

async function monitorSubscription() {
  const { running, source, nodeId } = desktop.status;
  if (!globalSettings.subscriptionAutoSwitch || !running || source !== 'subscription' || !nodeId) {
    monitoredNode = null;
    return;
  }
  const busy = () =>
    desktop.connecting || desktop.updating || desktop.testing || desktop.testingNodeId;
  if (busy()) return;
  if (monitoredNode !== nodeId) {
    monitoredNode = nodeId;
    failures = 0;
    notified = false;
  }
  const revision = failoverRevision;
  const stillCurrent = () =>
    revision === failoverRevision &&
    globalSettings.subscriptionAutoSwitch &&
    !busy() &&
    desktop.status.running &&
    desktop.status.source === 'subscription' &&
    desktop.status.nodeId === nodeId;
  const healthy = await desktopInvoke<boolean>('tauri_check_subscription_node', { nodeId });
  if (!stillCurrent()) return;
  if (healthy) {
    failures = 0;
    notified = false;
    return;
  }
  if (++failures < 3) return;
  failures = 0;
  const nodeIds = (globalSettings.subscriptionCandidates ?? []).filter(
    (id) =>
      id !== nodeId && desktop.nodes.some((node) => node.id === id && !node.unsupportedReason),
  );
  const results = nodeIds.length
    ? await desktopInvoke<{ id: string; latencyMs: number | null }[]>(
        'tauri_ping_subscription_nodes',
        { nodeIds },
      )
    : [];
  if (!stillCurrent()) return;
  const candidates = results
    .flatMap(({ id, latencyMs }) =>
      latencyMs !== null && latencyMs < 5000 ? [{ id, latencyMs }] : [],
    )
    .sort((a, b) => a.latencyMs - b.latencyMs);
  for (const candidate of candidates) {
    const healthy = await desktopInvoke<boolean>('tauri_check_subscription_node', {
      nodeId: candidate.id,
    });
    if (!stillCurrent()) return;
    if (!healthy) continue;
    await connectSelected(candidate.id);
    if (desktop.status.running && desktop.status.nodeId === candidate.id) {
      globalSettings.subscriptionSelectedId = candidate.id;
      await showNotifyWindow({
        notifyType: 'success',
        notifyMessage: `已自动切换至 ${desktop.status.name ?? '备用节点'}`,
      });
      return;
    }
    if (!stillCurrent()) return;
  }
  if (!notified) {
    notified = true;
    await showNotifyWindow({
      notifyType: 'error',
      notifyMessage: '当前节点异常，暂无可用候选节点',
    });
  }
}

function scheduleSubscriptionMonitor() {
  if (IS_ANDROID) return;
  window.setTimeout(async () => {
    try {
      await monitorSubscription();
    } catch (error) {
      appendLog(`自动切换检测失败：${errorText(error)}`);
    } finally {
      scheduleSubscriptionMonitor();
    }
  }, 15_000);
}
export async function refreshStatus() {
  applyStatus(await desktopInvoke<ProxyStatus>('tauri_proxy_status'));
}
export function initializeDesktop() {
  initialized ??= (async () => {
    if (IS_IOS) {
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
      applyStatus(status);
      await syncAndroidFailover();
      desktop.error = '';
      if (status.running && status.source === 'subscription' && status.nodeId)
        globalSettings.subscriptionSelectedId = status.nodeId;
      if ('__TAURI_INTERNALS__' in window) {
        await listen<ProxyStatus>('proxy-status', (event) => {
          applyStatus(event.payload);
        });
        if (!IS_MOBILE) {
          const mainWindow = getCurrentWindow();
          await mainWindow.onCloseRequested((event) => {
            if (!globalSettings.subscriptionAutoSwitch) return;
            event.preventDefault();
            void mainWindow.hide().catch((error) => appendLog(errorText(error)));
          });
        }
        if (IS_ANDROID)
          window.setInterval(() => {
            void refreshStatus().catch(() => {});
          }, 1500);
      } else if (import.meta.env.DEV) {
        window.setInterval(() => {
          void refreshStatus().catch(() => {});
        }, 1500);
      }
      scheduleSubscriptionMonitor();
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
  desktop.error = '';
  const nodes = desktop.nodes.filter((node) => !node.unsupportedReason);
  for (const node of nodes) {
    node.latencyMs = null;
    node.tested = false;
  }
  let next = 0;
  async function worker() {
    while (next < nodes.length && generation === testGeneration) {
      const id = nodes[next++].id;
      let latencyMs: number | null = null;
      try {
        const results = await desktopInvoke<{ id: string; latencyMs: number | null }[]>(
          'tauri_ping_subscription_nodes',
          { nodeId: id },
        );
        latencyMs = results.find((result) => result.id === id)?.latencyMs ?? null;
      } catch (error) {
        if (generation === testGeneration) desktop.error = errorText(error);
      }
      if (generation !== testGeneration) return;
      const node = desktop.nodes.find((node) => node.id === id);
      if (node) {
        node.latencyMs = latencyMs;
        node.tested = true;
      }
    }
  }
  try {
    await Promise.all(Array.from({ length: Math.min(6, nodes.length) }, worker));
    if (generation !== testGeneration) return;
    const available = (node: NodeEntry | undefined): node is NodeEntry =>
      Boolean(
        node &&
        !node.unsupportedReason &&
        node.tested &&
        node.latencyMs !== null &&
        node.latencyMs > 0 &&
        node.latencyMs < 5000,
      );
    const current = desktop.nodes.find((node) => node.id === globalSettings.subscriptionSelectedId);
    if (available(current)) return;
    const candidates = globalSettings.subscriptionAutoSwitch
      ? (globalSettings.subscriptionCandidates ?? []).map((id) =>
          desktop.nodes.find((node) => node.id === id),
        )
      : desktop.nodes;
    const next = candidates.find(available);
    if (next) await selectNode(next);
    else void message.error('当前无可用节点');
  } finally {
    if (generation === testGeneration) desktop.testing = false;
  }
}
export async function updateSubscription(url: string) {
  if (IS_IOS) return false;
  if (desktop.updating || desktop.connecting || desktop.testing || desktop.testingNodeId)
    return false;
  desktop.updating = true;
  desktop.error = '';
  try {
    const data = await desktopInvoke<Subscription>('tauri_update_subscription', { url });
    useSubscription(data);
    await syncAndroidFailover();
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
  if (desktop.connecting) return false;
  pauseCloudProxy();
  desktop.connecting = true;
  try {
    await desktopInvoke('tauri_stop_proxy_client');
    if (!IS_MOBILE) await desktopInvoke('tauri_interval_ping_stop');
    await refreshStatus();
    return true;
  } catch (error) {
    desktop.error = errorText(error);
    return false;
  } finally {
    desktop.connecting = false;
  }
}
export async function connectSelected(nodeId = globalSettings.subscriptionSelectedId) {
  if (IS_IOS) return;
  if (desktop.connecting || !nodeId) return;
  pauseCloudProxy();
  desktop.connecting = true;
  desktop.error = '';
  try {
    await desktopInvoke('tauri_connect_subscription', {
      nodeId,
      mode: globalSettings.routingMode,
    });
    if (!IS_MOBILE) await desktopInvoke('tauri_interval_ping_stop');
    if (!IS_MOBILE && globalSettings.subscriptionSystemProxy)
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
    node.latencyMs >= 5000
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
  if (IS_IOS) return;
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
    if (!IS_MOBILE)
      await desktopInvoke('tauri_interval_ping_start', {
        ip: serverIp,
        token: cvmId,
        cloud: {
          secretId: globalSettings.secretId,
          secretKey: globalSettings.secretKey,
          region: globalSettings.region,
        },
      });
    return true;
  }
  desktop.connecting = true;
  desktop.error = '';
  try {
    await desktopInvoke('tauri_start_proxy_client', {
      serverIp,
      cvmId,
      mode: globalSettings.routingMode,
      cloud: {
        secretId: globalSettings.secretId,
        secretKey: globalSettings.secretKey,
        region: globalSettings.region,
      },
    });
    await refreshStatus();
    if (!IS_MOBILE)
      await desktopInvoke('tauri_interval_ping_start', {
        ip: serverIp,
        token: cvmId,
        cloud: {
          secretId: globalSettings.secretId,
          secretKey: globalSettings.secretKey,
          region: globalSettings.region,
        },
      });
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
  if (IS_IOS || desktop.testing || desktop.testingNodeId || desktop.updating || desktop.connecting)
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
