import { IS_MOBILE, currentInWebMock } from '@/service/util';
import { invoke } from '@tauri-apps/api/core';
import { type Store, load } from '@tauri-apps/plugin-store';
import { appendLog } from './log';

let tauriPidStore: Store | undefined = undefined;

export async function loadSavedPid() {
  if (currentInWebMock) return;

  tauriPidStore = await load('pid.bin', {
    defaults: {},
    autoSave: true,
  });
  const pid = await tauriPidStore.get('pid');
  return pid;
}

export async function savePid(pid?: string) {
  if (!pid) await tauriPidStore?.delete('pid');
  else {
    await tauriPidStore?.set('pid', pid);
  }
}

export async function killPreviousPid() {
  if (IS_MOBILE) {
    return;
  }
  try {
    const pid = await loadSavedPid();
    if (pid) {
      appendLog(`try kill progress ${pid}`);
      await invoke('tauri_kill_progress_by_pid', {
        pid: pid,
      });
      await savePid();
    }
  } catch (ex) {
    console.error(ex);
  }
}
