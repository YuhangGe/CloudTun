import { DefaultSettings, type Settings } from '@/service/settings';
import type { CVMInstance } from '@/service/tencent';
import { currentInWebMock } from '@/service/util';
import { type Store, load } from '@tauri-apps/plugin-store';
import { vm, vmWatch } from 'jinge';

let tauriSettingStore: Store | undefined = undefined;

export interface GlobalStore {
  settings: Settings;
  instance?: CVMInstance;
  v2rayState: 'NOT_INSTALLED' | 'INSTALLING' | 'INSTALLED';
}

export const globalStore = vm<GlobalStore>({
  settings: DefaultSettings,
  v2rayState: 'NOT_INSTALLED',
});

export async function loadGlobalSettings() {
  if (currentInWebMock) return;

  tauriSettingStore = await load('settings.bin', {
    autoSave: true,
  });
  (await tauriSettingStore.entries()).forEach(([k, v]) => {
    //@ts-ignore
    globalStore.settings[k] = v;
    console.info('load setting:', k, v);
  });

  vmWatch(globalStore.settings, (v, _, p) => {
    const prop = (p as string[])[0] as keyof Settings;
    if (!prop) return;
    console.info('save setting:', prop, v[prop]);
    void tauriSettingStore?.set(prop, v[prop]);
  });
}
