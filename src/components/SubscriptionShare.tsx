import { onUnmount, vm, watch } from 'jinge';
import { Popover, message } from 'jinge-antd';

import { copyToClipboard } from '@/service/util';

export function SubscriptionShare(props: { url: string }) {
  const state = vm({ open: false, qr: '', loading: false, error: '' });
  let generation = 0;
  async function renderQr() {
    const url = props.url.trim();
    const current = ++generation;
    state.qr = '';
    state.error = '';
    state.loading = true;
    try {
      const { toDataURL } = await import('qrcode');
      const qr = await toDataURL(url, {
        width: 240,
        margin: 4,
        color: { dark: '#000000', light: '#ffffff' },
      });
      if (current === generation) state.qr = qr;
    } catch {
      if (current === generation) state.error = '无法生成二维码，链接可能过长，请使用复制按钮';
    } finally {
      if (current === generation) state.loading = false;
    }
  }
  watch(props, 'url', (url) => {
    ++generation;
    state.qr = '';
    if (!url.trim()) {
      state.open = false;
      state.loading = false;
    } else if (state.open) void renderQr();
  });
  onUnmount(() => {
    ++generation;
  });
  async function copy() {
    try {
      await copyToClipboard(props.url.trim());
      void message.success('已复制订阅链接');
    } catch {
      void message.error('复制失败，请手动复制订阅链接');
    }
  }
  return (
    <div className="flex items-center gap-1">
      <button
        type="button"
        aria-label="复制订阅链接"
        title="复制订阅链接"
        className="grid size-8 cursor-pointer place-items-center rounded-md text-base text-secondary-text hover:bg-primary/5 hover:text-primary disabled:cursor-default disabled:opacity-40"
        disabled={!props.url.trim()}
        on:click={() => {
          void copy();
        }}
      >
        <span className="icon-[ant-design--copy-outlined]" />
      </button>
      <Popover
        placement="bottom-start"
        triggerMethod="click"
        open={state.open}
        on:openChange={(open) => {
          state.open = open && Boolean(props.url.trim());
          if (state.open) void renderQr();
          else {
            ++generation;
            state.loading = false;
          }
        }}
        slot:content={
          <div className="w-60 max-w-[calc(100vw-48px)]">
            <p className="mb-2 text-center text-xs">订阅二维码</p>
            {state.loading ? (
              <p className="p-6 text-center text-xs text-secondary-text">正在生成…</p>
            ) : state.error ? (
              <p className="p-3 text-xs text-error" role="alert">
                {state.error}
              </p>
            ) : (
              state.qr && (
                <img
                  className="aspect-square w-full rounded bg-white"
                  src={state.qr}
                  alt="订阅链接二维码"
                />
              )
            )}
          </div>
        }
      >
        <button
          type="button"
          aria-label="显示订阅二维码"
          title="显示订阅二维码"
          className="grid size-8 cursor-pointer place-items-center rounded-md text-base text-secondary-text hover:bg-primary/5 hover:text-primary disabled:cursor-default disabled:opacity-40"
          disabled={!props.url.trim()}
        >
          <span className="icon-[ant-design--qrcode-outlined]" />
        </button>
      </Popover>
    </div>
  );
}
