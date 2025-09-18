import { DefaultSettings, type Settings } from '@/service/settings';
import { currentInWebMock } from '@/service/util';
import { type Store, load } from '@tauri-apps/plugin-store';
import { isString, vm, vmRaw, vmWatch } from 'jinge';
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
      Object.assign(globalSettings, data);
      appendLog(`load setting: ${cnt}`);
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
