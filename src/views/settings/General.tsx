import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
import { onMount, onUnmount, vm, watch } from 'jinge';

import { IS_MOBILE, currentInWebMock } from '@/service/util';
import { desktop, saveProxyConfig, refreshProxyConfig, type ProxyConfig } from '@/store/desktop';
import { globalSettings } from '@/store/settings';

import { GeoSettings } from './Geo';

export function GeneralSettings() {
  const state = vm({
    port: String(desktop.config.port),
    lan: desktop.config.listenHost === '0.0.0.0',
    error: '',
    autoStart: globalSettings.autoStartApp,
    busy: false,
  });
  let timer = 0;
  onUnmount(() => {
    if (timer) {
      clearTimeout(timer);
      timer = 0;
      void save();
    }
  });
  function schedule() {
    clearTimeout(timer);
    state.error = '';
    timer = window.setTimeout(() => {
      timer = 0;
      void save();
    }, 400);
  }
  watch(desktop, 'config', (config) => {
    state.port = String(config.port);
    state.lan = config.listenHost === '0.0.0.0';
  });
  onMount(() => {
    if (!IS_MOBILE && !currentInWebMock)
      void isEnabled()
        .then((value) => {
          state.autoStart = value;
        })
        .catch(() => {});
  });
  async function save() {
    state.error = '';
    const port = Number(state.port);
    if (!/^\d+$/.test(state.port) || !Number.isInteger(port) || port < 1 || port > 65535) {
      state.error = '端口必须是 1–65535 之间的整数';
      return;
    }
    try {
      await saveProxyConfig({
        port,
        listenHost: state.lan ? '0.0.0.0' : '127.0.0.1',
      } as ProxyConfig);
    } catch (error) {
      state.error = error instanceof Error ? error.message : String(error);
      await refreshProxyConfig().catch(() => {});
      state.port = String(desktop.config.port);
      state.lan = desktop.config.listenHost === '0.0.0.0';
    }
  }
  async function setAutoStart(value: boolean) {
    state.busy = true;
    state.error = '';
    try {
      if (value) await enable();
      else await disable();
      state.autoStart = value;
      globalSettings.autoStartApp = value;
    } catch (error) {
      state.error = String(error);
    } finally {
      state.busy = false;
    }
  }
  return (
    <section className="max-w-2xl [&_h2]:mb-4 [&_h2]:text-base [&_h2]:font-semibold">
      <h2>本地 HTTP 代理</h2>
      {IS_MOBILE && (
        <p className="mb-4 rounded-lg border border-primary/20 bg-primary/5 p-3 text-xs leading-relaxed text-secondary-text">
          移动端监听设置将在订阅内核接入后启用。
        </p>
      )}
      <label className="mb-4 flex flex-wrap items-center gap-4 text-sm [&_input]:h-10 [&_input]:w-32 [&_input]:rounded-lg [&_input]:border [&_input]:border-border [&_input]:bg-transparent [&_input]:px-3 [&_input]:outline-none [&_input:disabled]:opacity-50 [&_input:focus]:border-primary">
        <span>监听端口</span>
        <input
          aria-label="监听端口"
          type="number"
          inputMode="numeric"
          min="1"
          max="65535"
          value={state.port}
          disabled={IS_MOBILE || desktop.connecting || desktop.savingConfig || !desktop.ready}
          on:input={(event) => {
            state.port = event.target.value;
            schedule();
          }}
        />
      </label>
      <label className="flex min-h-10 items-center gap-2.5 text-sm [&_input]:size-4 [&_input]:accent-primary">
        <input
          type="checkbox"
          checked={state.lan}
          disabled={IS_MOBILE || desktop.connecting || desktop.savingConfig || !desktop.ready}
          on:change={(event) => {
            state.lan = event.target.checked;
            clearTimeout(timer);
            timer = 0;
            void save();
          }}
        />
        <span>允许局域网连接</span>
      </label>
      {state.error && (
        <p
          className="mt-4 rounded-lg border border-error/25 bg-error/5 px-4 py-3 text-xs wrap-break-word text-error"
          role="alert"
        >
          {state.error}
        </p>
      )}
      {!IS_MOBILE && <GeoSettings />}
      {!IS_MOBILE && (
        <div className="mt-7 border-t border-border pt-6">
          <h2>应用</h2>
          <label className="flex min-h-10 items-center gap-2.5 text-sm [&_input]:size-4 [&_input]:accent-primary">
            <input
              type="checkbox"
              checked={state.autoStart}
              disabled={state.busy || currentInWebMock}
              on:change={(event) => {
                void setAutoStart(event.target.checked);
              }}
            />
            <span>开机启动 CloudTun</span>
          </label>
        </div>
      )}
    </section>
  );
}
