import { onMount, vm } from 'jinge';

import { desktopInvoke } from '@/service/desktop';

interface DebugStatus {
  enabled: boolean;
  droppedEntries: number;
  error: string | null;
}

export function DebugSettings() {
  const state = vm({ enabled: false, ready: false, busy: false, error: '', path: '' });
  function apply(status: DebugStatus) {
    state.enabled = status.enabled;
    state.error = status.error ?? '';
    if (status.droppedEntries)
      state.error += `${state.error ? '；' : ''}日志写入繁忙，已丢弃 ${status.droppedEntries} 条记录`;
  }
  onMount(() => {
    void desktopInvoke<DebugStatus>('tauri_debug_log_status')
      .then(apply)
      .catch((error) => {
        state.error = String(error);
      })
      .finally(() => {
        state.ready = true;
      });
  });
  async function toggle(enabled: boolean) {
    state.busy = true;
    state.error = '';
    try {
      apply(await desktopInvoke<DebugStatus>('tauri_set_debug_logging', { enabled }));
    } catch (error) {
      state.error = String(error);
    } finally {
      state.busy = false;
    }
  }
  async function download() {
    state.busy = true;
    state.error = '';
    state.path = '';
    try {
      const result = await desktopInvoke<{ path: string }>('tauri_export_debug_logs');
      state.path = result.path;
      apply(await desktopInvoke<DebugStatus>('tauri_debug_log_status'));
    } catch (error) {
      state.error = String(error);
    } finally {
      state.busy = false;
    }
  }
  return (
    <div className="mt-7 border-t border-border pt-6">
      <h2>调试日志</h2>
      <label className="flex min-h-10 items-center gap-2.5 text-sm [&_input]:size-4 [&_input]:accent-primary">
        <input
          type="checkbox"
          checked={state.enabled}
          disabled={!state.ready || state.busy}
          on:change={(event) => {
            void toggle(event.target.checked);
          }}
        />
        <span>开启调试日志</span>
      </label>
      <p className="mt-2 text-xs leading-relaxed text-secondary-text">
        开启后记录连接目标、耗时及错误详情，文件最多保留约 6 MB。关闭后停止记录，重启应用默认关闭。
        日志包含访问域名，不记录请求正文和认证头。
      </p>
      {state.enabled && (
        <button
          type="button"
          disabled={state.busy}
          className="mt-3 cursor-pointer rounded-md border border-border px-2.5 py-1 text-xs hover:border-primary hover:text-primary disabled:cursor-default disabled:opacity-50"
          on:click={() => {
            void download();
          }}
        >
          {state.busy ? '正在处理…' : '下载日志'}
        </button>
      )}
      {state.path && (
        <p className="mt-3 text-xs leading-relaxed wrap-anywhere text-success" role="status">
          日志已保存到下载目录：{state.path}
        </p>
      )}
      {state.error && (
        <p className="mt-3 text-xs wrap-anywhere text-error" role="alert">
          {state.error}
        </p>
      )}
    </div>
  );
}
