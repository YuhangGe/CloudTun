import { uid } from '@/service/util';
import { listen } from '@tauri-apps/api/event';
import { vm } from 'jinge';
import { message } from 'jinge-antd';

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
  console.info(log);
  logStore.logs.push({ id: uid(), text: log });
  if (logStore.logs.length > MAX_LOGS_LENGTH) {
    logStore.logs.unshift();
  }
}

void listen('log::v2ray', (ev) => appendLog(`[v2ray] ==> ${ev.payload}`));
void listen('log::ping', (ev) => appendLog(`[ping] ==> ${ev.payload}`));
void listen('log::disconnected', (ev) => {
  appendLog(`[server] ==> ${ev.payload}`);
  message.error({
    content: ev.payload as string,
    duration: 0,
  });
});
