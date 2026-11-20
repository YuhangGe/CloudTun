import { type Store, load } from '@tauri-apps/plugin-store';
import { isString, vm, vmRaw, vmWatch } from 'jinge';

import { DefaultSettings, type Settings } from '@/service/settings';
import { currentInWebMock } from '@/service/util';

import { appendLog } from './log';

let tauriSettingStore: Store | undefined = undefined;

export const globalSettings = vm<Settings>({
  ...DefaultSettings,
});

export async function loadGlobalSettings() {
  if (currentInWebMock) return;

  tauriSettingStore = await load('settings.bin', {
    defaults: {},
    autoSave: true,
  });

  const cnt = await tauriSettingStore.get('settings');
  if (isString(cnt)) {
    try {
      const data = JSON.parse(cnt);
      delete data.autoProxy;
      delete data.proxyRules;
      if (data.routingMode === undefined && data.subscriptionRoutingMode !== undefined)
        data.routingMode = data.subscriptionRoutingMode;
      delete data.subscriptionRoutingMode;
      if (data.region === 'ap-singapore') {
        data.region = DefaultSettings.region;
        data.zone = DefaultSettings.zone;
        data.instanceType = DefaultSettings.instanceType;
        data.imageId = DefaultSettings.imageId;
      }
      if (
        (!data.secretId || !data.secretKey) &&
        DefaultSettings.secretId &&
        DefaultSettings.secretKey
      ) {
        data.secretId = DefaultSettings.secretId;
        data.secretKey = DefaultSettings.secretKey;
      }
      Object.assign(globalSettings, data);
      appendLog('已加载本地设置');
    } catch (ex) {
      console.error(ex);
    }
  }
  let tm = 0;
  vmWatch(globalSettings, () => {
    if (tm) clearTimeout(tm);
    tm = window.setTimeout(() => {
      tm = 0;
      void tauriSettingStore?.set('settings', JSON.stringify(vmRaw(globalSettings), null, 2));
    });
  });
}
