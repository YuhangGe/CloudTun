import { DefaultSettings, type Settings } from '@/service/settings';
import type { CVMInstance } from '@/service/tencent';
import { load } from '@tauri-apps/plugin-store';
import { vm, vmWatch } from 'jinge';

// create a new store or load the existing one
const tauriStore = await load('store.bin', {
  // we can save automatically after each store modification
  // autoSave: true,
});

export interface GlobalStore {
  settings: Settings;
  instance?: CVMInstance;
  v2rayState: 'NOT_INSTALLED' | 'INSTALLING' | 'INSTALLED';
}

async function getLs<P extends keyof GlobalStore, T = string>(key: P) {
  const v = await tauriStore.get(key);
  if (typeof v !== 'string' || !v) return undefined;
  return JSON.parse(v) as T;
}
export const globalStore = vm<GlobalStore>({
  settings: DefaultSettings,
  v2rayState: 'NOT_INSTALLED',
});

export async function loadGlobalSettings() {
  globalStore.settings = {
    ...DefaultSettings,
    ...(await getLs('settings')),
  };
}

['settings'].forEach((prop) => {
  vmWatch(globalStore, prop as keyof GlobalStore, (v) => {
    void tauriStore.set(prop, JSON.stringify(v)).then(() => {
      return tauriStore.save();
    });
  });
});
