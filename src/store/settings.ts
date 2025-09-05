import { DefaultSettings, type Settings } from '@/service/settings';
import { currentInWebMock } from '@/service/util';
import { type Store, load } from '@tauri-apps/plugin-store';
import { vm, vmWatch } from 'jinge';
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
  (await tauriSettingStore.entries()).forEach(([k, v]) => {
    (globalSettings as unknown as Record<string, unknown>)[k] = v;
    appendLog(`load setting: ${k} => ${v}`);
  });

  vmWatch(globalSettings, (v, _, p) => {
    const prop = (p as string[])[0] as keyof Settings;
    if (!prop) return;
    appendLog(`save setting: ${prop} => ${v[prop]}`);
    void tauriSettingStore?.set(prop, v[prop]);
  });
}
