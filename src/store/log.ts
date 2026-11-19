import { listen } from '@tauri-apps/api/event';
import { vm } from 'jinge';
import { message } from 'jinge-antd';

import { desktopInvoke } from '@/service/desktop';

export interface Log {
  id: number;
  timestamp: number;
  text: string;
}
interface LogSnapshot {
  sessionId: string;
  logs: Log[];
}
interface LogEvent {
  sessionId: string;
  entry: Log;
}
export const logStore = vm({ logs: [] as Log[], ready: false, error: '' });
const MAX_LOGS = 1000;
let sessionId = '';
let initialized: Promise<void> | undefined;
function merge(logs: Log[], session: string) {
  if (sessionId !== session) {
    sessionId = session;
    logStore.logs = [];
  }
  const entries = new Map(logStore.logs.map((log) => [log.id, log]));
  for (const log of logs) entries.set(log.id, log);
  const next = [...entries.values()].sort((a, b) => a.id - b.id).slice(-MAX_LOGS);
  if (
    next.length !== logStore.logs.length ||
    next.some((log, index) => log.id !== logStore.logs[index].id)
  )
    logStore.logs = next;
}
function receive(event: LogEvent) {
  merge([event.entry], event.sessionId);
}
export async function refreshLogs() {
  const requestedSession = sessionId;
  try {
    const snapshot = await desktopInvoke<LogSnapshot>('tauri_get_logs');
    // A late response from the previous backend must not overwrite a new session.
    if (sessionId !== requestedSession && sessionId !== snapshot.sessionId) return;
    merge(snapshot.logs, snapshot.sessionId);
    logStore.error = '';
  } catch {
    logStore.error = '读取后台日志失败，请稍后重试';
  } finally {
    logStore.ready = true;
  }
}
export function initializeLogs() {
  initialized ??= (async () => {
    if ('__TAURI_INTERNALS__' in window) {
      // Subscribe first, then merge the snapshot by id to avoid gaps and duplicates.
      await listen<LogEvent>('log-entry', (event) => {
        receive(event.payload);
      });
      await listen<string>('log::disconnected', (event) => {
        void message.error({ content: event.payload, duration: 0 });
      });
    } else if (import.meta.env.DEV) {
      window.setInterval(() => {
        void refreshLogs();
      }, 1000);
    }
    await refreshLogs();
  })().catch(() => {
    logStore.ready = true;
    logStore.error = '订阅后台日志失败，请重新打开窗口';
  });
  return initialized;
}
export function appendLog(text: string) {
  console.info(text);
  void desktopInvoke<LogEvent>('tauri_append_log', { text })
    .then(receive)
    .catch(() => {
      console.warn('无法写入后台日志');
    });
}
