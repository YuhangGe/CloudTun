import { invoke } from '@tauri-apps/api/core';
import { type UnlistenFn, listen } from '@tauri-apps/api/event';
import { onMount, vm } from 'jinge';
import { Spin } from 'jinge-antd';

import imgLogo from '@/assets/logo-512.png';

import { type Notify } from './service/notify';

function App() {
  const s = new URLSearchParams(location.search);
  const notify = vm<Notify>({
    notifyType: s.get('type') ?? 'processing',
    notifyMessage: s.get('message') ?? '-',
  } as Notify);

  let closeTm = 0;

  onMount(() => {
    let lisFn: UnlistenFn | undefined = undefined;
    let disposed = false;
    const scheduleClose = () => {
      clearTimeout(closeTm);
      if (notify.notifyType !== 'processing') {
        closeTm = window.setTimeout(() => {
          void invoke('tauri_close_notify_window');
        }, 5000);
      }
    };
    scheduleClose();
    void listen<Notify>('notify-state-changed', (evt) => {
      Object.assign(notify, evt.payload);
      scheduleClose();
    }).then((fn) => {
      if (disposed) fn();
      else lisFn = fn;
    });
    return () => {
      disposed = true;
      clearTimeout(closeTm);
      if (lisFn) lisFn();
    };
  });

  return (
    <div className="flex size-full flex-col overflow-hidden bg-background px-4 py-3 backdrop-blur-2xl select-none">
      <div className="flex items-center gap-2">
        <img src={imgLogo} className="size-4.5" />
        <span className="text-sm">CloudTun</span>
        <span className="flex-1"></span>
        <span className="text-base text-primary icon-[gravity-ui--broadcast-signal]"></span>
      </div>
      {notify.notifyType === 'processing' && (
        <p className="mt-4 flex items-center gap-3">
          <Spin size="sm" />
          <span className="text-[16px] text-secondary-text">{notify.notifyMessage}</span>
        </p>
      )}

      {notify.notifyType === 'success' && (
        <p className="mt-4 flex items-center gap-2 text-success">
          <span className="icon-[ant-design--check-circle-outlined] text-[21px]"></span>
          <span className="text-[16px]">{notify.notifyMessage}</span>
        </p>
      )}
      {notify.notifyType === 'error' && (
        <p className="mt-4 flex items-center gap-2 text-orange">
          <span className="icon-[ant-design--info-circle-outlined] text-[21px]"></span>
          <span className="text-[16px]">{notify.notifyMessage}</span>
        </p>
      )}
    </div>
  );
}

export default App;
