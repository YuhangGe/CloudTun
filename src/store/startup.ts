import type { CVMInstance } from '@/service/tencent';

import { IS_MOBILE } from '@/service/util';
import { loadInstance } from '@/views/proxy/helper';

import {
  desktop,
  initializeDesktop,
  updateSubscription,
  testNodes,
  connectSelected,
} from './desktop';
import { appendLog } from './log';
import { globalSettings } from './settings';

export type StartupView = 'cloud' | 'nodes';
interface StartupTarget {
  view: StartupView;
  instance?: CVMInstance;
}
let startup: Promise<StartupTarget> | undefined;
let subscriptionActivation: Promise<void> | undefined;
export function initializeStartup(): Promise<StartupTarget> {
  startup ??= (async () => {
    await initializeDesktop();
    if (
      !IS_MOBILE &&
      globalSettings.secretId &&
      globalSettings.secretKey &&
      globalSettings.region &&
      globalSettings.resourceName
    ) {
      appendLog('[log::startup] 正在检查已有云主机');
      try {
        const [error, data] = await loadInstance();
        if (error) throw error;
        const instance = data.InstanceSet.find(
          (item) => !['TERMINATING', 'LAUNCH_FAILED'].includes(item.InstanceState),
        );
        if (instance) {
          appendLog(`[log::startup] 发现云主机 ${instance.InstanceId}，自动激活云上主机`);
          return { view: 'cloud' as const, instance };
        }
        appendLog('[log::startup] 没有已有云主机，使用订阅节点');
      } catch (error) {
        appendLog(
          `[log::startup] 检查云主机失败，保留订阅页面：${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }
    return { view: 'nodes' as const };
  })();
  return startup;
}

export function activateInitialSubscription() {
  subscriptionActivation ??= (async () => {
    const target = await initializeStartup();
    if (IS_MOBILE) return;
    const url = desktop.subscriptionUrl.trim();
    if (!url) {
      if (desktop.nodes.length) await testNodes();
      return;
    }
    const selectedId = globalSettings.subscriptionSelectedId;
    appendLog('[log::startup] 首次进入订阅节点，自动更新已配置的订阅');
    if (!(await updateSubscription(url))) return;
    // An existing cloud host has startup priority. Updating the list does not switch it.
    if (target.view === 'cloud' || desktop.status.running || !selectedId) return;
    const selected = desktop.nodes.find((node) => node.id === selectedId);
    if (
      !selected ||
      selected.unsupportedReason ||
      selected.latencyMs === null ||
      selected.latencyMs >= 500
    ) {
      appendLog('[log::startup] 上次选择的节点已失效或不可用，请手动选择');
      return;
    }
    if (globalSettings.subscriptionSelectedId !== selectedId) return;
    appendLog('[log::startup] 上次选择的节点仍可用，自动启动订阅代理');
    await connectSelected();
  })();
  return subscriptionActivation;
}
