import { invoke } from '@tauri-apps/api/core';

export function desktopInvoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if ('__TAURI_INTERNALS__' in window) return invoke<T>(command, args);
  if (!import.meta.env.DEV) return Promise.reject(new Error('请在 CloudTun 应用中运行'));
  // Debug-only loopback bridge calls the same Rust command handlers as Tauri IPC.
  return fetch('http://127.0.0.1:1422/command', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ command, args }),
  }).then(async (response) => {
    const result = (await response.json()) as { data?: T; error?: string };
    if (!response.ok || result.error) throw new Error(result.error ?? '无法连接本地 Rust 服务');
    return result.data as T;
  });
}
