import { onMount, onUnmount, vm } from 'jinge';
import { Button } from 'jinge-antd';

import { desktopInvoke } from '@/service/desktop';
import { desktop } from '@/store/desktop';

interface GeoStatus {
  available: boolean;
  ipVersion: string | null;
  domainVersion: string | null;
  phase: string;
  error: string | null;
  files: { name: string; downloaded: number; total: number | null; complete: boolean }[];
}
function bytes(value: number) {
  return value < 1024 * 1024
    ? `${(value / 1024).toFixed(1)} KB`
    : `${(value / 1024 / 1024).toFixed(1)} MB`;
}
export function GeoSettings() {
  const state = vm({
    geo: {
      available: false,
      ipVersion: null,
      domainVersion: null,
      phase: 'idle',
      error: null,
      files: [],
    } as GeoStatus,
    label: '读取规则状态…',
    active: false,
    requesting: false,
    error: '',
  });
  let timer = 0;
  let disposed = false;
  let polling = false;
  function apply(status: GeoStatus) {
    if (disposed) return;
    state.geo = status;
    state.active = ['checking', 'downloading', 'installing'].includes(status.phase);
    state.label =
      (
        {
          checking: '检查版本中…',
          downloading: '下载规则中…',
          installing: '校验并应用中…',
          latest: '已是最新版本',
          error: status.available ? '更新失败，继续使用现有规则' : '更新失败，绕过大陆暂未生效',
        } as Record<string, string>
      )[status.phase] ?? (status.available ? '已加载' : '尚未下载，绕过大陆暂未生效');
  }
  async function refresh() {
    if (polling) return;
    polling = true;
    try {
      const status = await desktopInvoke<GeoStatus>('tauri_geo_status');
      apply(status);
      if (!disposed) state.error = '';
    } catch (error) {
      if (!disposed) state.error = error instanceof Error ? error.message : String(error);
    } finally {
      polling = false;
    }
  }
  async function update() {
    state.requesting = true;
    state.error = '';
    try {
      apply(await desktopInvoke<GeoStatus>('tauri_update_geo'));
    } catch (error) {
      if (!disposed) state.error = error instanceof Error ? error.message : String(error);
    } finally {
      if (!disposed) state.requesting = false;
    }
  }
  onMount(() => {
    void refresh();
    timer = window.setInterval(() => void refresh(), 500);
  });
  onUnmount(() => {
    disposed = true;
    clearInterval(timer);
  });
  return (
    <div className="mt-7 border-t border-border pt-6">
      <div className="mb-3 flex items-center justify-between gap-3">
        <h2 className="mb-0!">GEO 规则</h2>
        <Button
          size="sm"
          disabled={!desktop.status.running || state.active || state.requesting}
          on:click={() => void update()}
        >
          <span
            className={
              state.active || state.requesting
                ? 'icon-[ant-design--sync-outlined] animate-spin'
                : 'icon-[ant-design--sync-outlined]'
            }
          />
          {state.active || state.requesting ? '更新中' : '下载更新'}
        </Button>
      </div>
      <p className="text-xs text-secondary-text" role="status">
        {state.label}
      </p>
      {state.geo.available && (
        <p className="mt-2 font-mono text-xs text-secondary-text">
          IP {state.geo.ipVersion?.slice(0, 8)} · 域名 {state.geo.domainVersion?.slice(0, 8)}
        </p>
      )}
      {!desktop.status.running && !state.active && (
        <p className="mt-2 text-xs text-secondary-text">连接代理后可更新</p>
      )}
      {state.geo.phase === 'downloading' && (
        <div className="mt-3 space-y-3" aria-label="GEO 下载进度">
          {state.geo.files.map((file) => (
            <div key={file.name}>
              <div className="mb-1 flex justify-between gap-2 text-xs text-secondary-text">
                <span>{file.name}</span>
                <span>
                  {bytes(file.downloaded)}
                  {file.total !== null ? ` / ${bytes(file.total)}` : ''}
                  {file.complete ? ' · 完成' : ''}
                </span>
              </div>
              {file.total !== null || file.complete ? (
                <progress
                  aria-label={file.name}
                  className="block h-1.5 w-full accent-primary"
                  max={file.total ?? (file.downloaded || 1)}
                  value={file.downloaded}
                />
              ) : (
                <progress aria-label={file.name} className="block h-1.5 w-full accent-primary" />
              )}
            </div>
          ))}
        </div>
      )}
      {(state.error || state.geo.error) && (
        <p className="mt-3 text-xs wrap-break-word text-error" role="alert">
          {state.error || state.geo.error}
        </p>
      )}
    </div>
  );
}
