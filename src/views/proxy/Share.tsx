import { Button, Popover } from 'jinge-antd';
import { CopyButton } from '@/components/Copy';
import { globalSettings } from '@/store/settings';
import { vm } from 'jinge';

export function ShareCode(props: { ip: string }) {
  const config = {
    id: globalSettings.token,
    add: props.ip,
    port: '2080',
    alpn: '',
    fp: '',
    host: '',
    aid: '0',
    net: 'tcp',
    path: '',
    scy: 'none',
    sni: 'tls',
    type: 'none',
    v: '2',
    ps: 'a',
  };
  const url = `vmess://${window.btoa(JSON.stringify(config))}`;
  const state = vm({
    qrcode: '',
  });

  async function renderQr() {
    if (state.qrcode) return;
    const mod = await import('qrcode');
    state.qrcode = await mod.toDataURL(url, { margin: 0 });
  }

  return (
    <>
      <Popover
        placement='bottom-end'
        className='h-[200px] w-[200px] rounded-lg bg-white shadow-lg'
        slot:content={
          <div className=''>
            {state.qrcode && <img className='h-full w-full' src={state.qrcode} />}
          </div>
        }
        on:openChange={(v) => {
          if (v) {
            void renderQr();
          }
        }}
      >
        <Button
          size='sm'
          type='link'
          slot:icon={<span className='icon-[ant-design--qrcode-outlined] text-base'></span>}
        />
      </Popover>
      <CopyButton text={url} />
    </>
  );
}
