import imgLogo from '@/assets/logo-128x128.png';
import { type UnlistenFn, listen } from '@tauri-apps/api/event';
import { onMount, vm } from 'jinge';
import { Spin } from 'jinge-antd';
import type { Notify } from './service/notify';

function App() {
  const s = new URLSearchParams(location.search);
  const notify = vm<Notify>({
    type: s.get('type') ?? 'processing',
    message: s.get('message') ?? '-',
  } as Notify);

  onMount(() => {
    let lisFn: UnlistenFn | undefined = undefined;
    void listen<Notify>('notify-state-changed', (evt) => {
      Object.assign(notify, evt.payload);
    }).then((fn) => {
      lisFn = fn;
    });
    return () => {
      if (lisFn) lisFn();
    };
  });

  return (
    <div className='bg-background flex size-full flex-col overflow-hidden px-4 py-3 backdrop-blur-2xl select-none'>
      <div className='flex items-center gap-2'>
        <img src={imgLogo} className='size-[18px]' />
        <span className='text-sm'>CloudV2Ray</span>
        <span className='flex-1'></span>
        <span className='icon-[gravity-ui--broadcast-signal] text-primary text-base'></span>
      </div>
      {notify.type === 'processing' && (
        <p className='mt-4 flex items-center gap-3'>
          <Spin size='sm' />
          <span className='text-secondary-text text-[16px]'>{notify.message}</span>
        </p>
      )}

      {notify.type === 'success' && (
        <p className='text-success mt-4 flex items-center gap-2'>
          <span className='icon-[ant-design--check-circle-outlined] text-[21px]'></span>
          <span className='text-[16px]'>{notify.message}</span>
        </p>
      )}
      {notify.type === 'error' && (
        <p className='text-orange mt-4 flex items-center gap-2'>
          <span className='icon-[ant-design--info-circle-outlined] text-[21px]'></span>
          <span className='text-[16px]'>{notify.message}</span>
        </p>
      )}
    </div>
  );
}

export default App;
