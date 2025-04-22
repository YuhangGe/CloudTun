import { uid } from '@/service/util';
import { listen } from '@tauri-apps/api/event';
import { vm } from 'jinge';

export interface Log {
  id: string;
  text: string;
}
export interface LogStore {
  logs: Log[];
}
export const logStore = vm<LogStore>({
  logs: [],
});

const MAX_LOGS_LENGTH = 1000;

export function appendLog(log: string) {
  // eslint-disable-next-line no-console
  console.log(log);
  logStore.logs.push({ id: uid(), text: log });
  if (logStore.logs.length > MAX_LOGS_LENGTH) {
    logStore.logs.unshift();
  }
}

void listen('log::v2ray', (ev) => appendLog(`[v2ray] ==> ${ev.payload}`));
