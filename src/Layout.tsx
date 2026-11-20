import { cx, vm, watch } from 'jinge';
import { message } from 'jinge-antd';

import imgLogo from '@/assets/logo-512.png';
import { ProxyOptions } from '@/components/ProxyOptions';
import { ProxyTraffic } from '@/components/ProxyTraffic';

import type { StartupView } from './store/startup';

import { desktopInvoke } from './service/desktop';
import { copyToClipboard, IS_MOBILE } from './service/util';
import { desktop } from './store/desktop';
import { LogView } from './views/logview';
import { NodesView } from './views/nodes';
import { CloudPage } from './views/proxy/CloudPage';
import { GeneralSettings } from './views/settings/General';

const items = [
  { value: 'nodes', label: '订阅节点', icon: 'icon-[ant-design--global-outlined]' },
  {
    value: 'cloud',
    label: '云上主机',
    icon: 'icon-[ant-design--cloud-server-outlined]',
  },
  { value: 'logs', label: '运行日志', icon: 'icon-[ant-design--file-text-outlined]' },
  { value: 'settings', label: '设置', icon: 'icon-[ant-design--setting-outlined]' },
];
export function Layout(props: { initialView: StartupView }) {
  const state = vm({
    view: props.initialView as string,
    title: items.find((item) => item.value === props.initialView)?.label ?? '',
  });
  watch(state, 'view', (view) => {
    state.title = items.find((item) => item.value === view)?.label ?? '';
  });
  async function copyProxyAddress(url: string) {
    try {
      await copyToClipboard(url);
      void message.success('已复制代理地址');
    } catch {
      void message.error('复制失败，请手动复制地址');
    }
  }
  function navigate(view: string) {
    if (desktop.connecting || desktop.updating) return;
    state.view = view;
  }
  return (
    <div className="flex size-full flex-col bg-background pt-[env(safe-area-inset-top)] text-sm text-normal-text sm:flex-row">
      <aside className="order-2 shrink-0 border-t border-border bg-background pb-[env(safe-area-inset-bottom)] sm:order-0 sm:flex sm:w-49 sm:flex-col sm:border-t-0 sm:border-r sm:bg-hover/20 sm:px-3.5 sm:pb-4.5">
        <div className="hidden h-17 shrink-0 items-center gap-2.5 px-2.5 sm:flex">
          <img className="size-9.5 rounded-xl" src={imgLogo} alt="CloudTun" />
          <div>
            <strong className="block text-[19px] tracking-tight">CloudTun</strong>
          </div>
        </div>
        <nav
          aria-label="主导航"
          className="grid grid-cols-4 gap-1 px-2 py-1.5 sm:block sm:p-0 sm:pt-6.5"
        >
          {items.map((item) => (
            <button
              type="button"
              key={item.value}
              className={cx(
                'flex min-h-12 w-full cursor-pointer flex-col items-center justify-center gap-1 rounded-lg border-0 px-1 py-2 text-[10px] transition-colors hover:bg-primary/5 disabled:cursor-wait disabled:opacity-50 sm:mb-1 sm:flex-row sm:justify-start sm:gap-3 sm:px-3.5 sm:py-3 sm:text-sm',
                state.view === item.value
                  ? 'bg-primary/10 font-semibold text-primary'
                  : 'bg-transparent text-normal-text',
              )}
              disabled={desktop.connecting || desktop.updating}
              aria-current={state.view === item.value ? 'page' : undefined}
              on:click={() => {
                navigate(item.value);
              }}
            >
              <span className={cx(item.icon, 'text-xl')} />
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="mt-auto hidden items-center gap-2 px-3.5 pt-6 pb-1 text-xs sm:flex">
          <span
            className={cx(
              'inline-block size-2 rounded-full',
              desktop.status.running ? 'bg-emerald-500 ring-4 ring-emerald-500/10' : 'bg-gray-400',
            )}
          />
          <span>{desktop.status.running ? '代理运行中' : '代理未连接'}</span>
        </div>
        {!IS_MOBILE && desktop.status.running && <ProxyTraffic />}
        {!IS_MOBILE && (
          <>
            <div className="hidden space-y-1 px-3.5 pt-1 pb-4 font-mono text-[10px] sm:block">
              {desktop.addresses.length ? (
                desktop.addresses.map((address) => (
                  <div
                    key={address.url}
                    title={address.label}
                    className="flex min-h-5 items-center gap-1"
                  >
                    <span className="min-w-0 flex-1 opacity-50">
                      {address.url.replace('http://', '')}
                    </span>
                    <button
                      type="button"
                      title={`复制${address.label}代理地址`}
                      aria-label={`复制${address.label}代理地址`}
                      className="grid size-5 shrink-0 cursor-pointer place-items-center border-0 bg-transparent text-xs text-secondary-text hover:text-primary"
                      on:click={() => void copyProxyAddress(address.url)}
                    >
                      <span className="icon-[ant-design--copy-outlined]" />
                    </button>
                  </div>
                ))
              ) : (
                <div className="opacity-50">127.0.0.1:{desktop.config.port}</div>
              )}
            </div>
            <button
              type="button"
              className="hidden cursor-pointer border-0 bg-transparent px-3.5 py-2.5 text-left text-xs opacity-50 sm:block"
              on:click={() => {
                void desktopInvoke('tauri_exit_process');
              }}
            >
              退出 CloudTun
            </button>
          </>
        )}
      </aside>
      <main className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex min-h-13 shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b border-border px-4 py-3 text-[13px] font-medium sm:min-h-17 sm:px-8">
          <span>{state.title}</span>
          <ProxyOptions />
        </header>
        <div className="min-h-0 flex-1 overflow-auto px-4 pt-5 pb-6 sm:px-8 sm:pt-7 sm:pb-9">
          {state.view === 'nodes' && <NodesView />}
          {state.view === 'cloud' && <CloudPage />}
          {state.view === 'logs' && <LogView />}
          {state.view === 'settings' && (
            <div className="mx-auto max-w-[1100px] [&_h1]:my-1.5 [&_h1]:text-2xl [&_h1]:font-semibold [&_h1]:tracking-tight">
              <h1 className="mb-7!">设置</h1>
              <GeneralSettings />
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
