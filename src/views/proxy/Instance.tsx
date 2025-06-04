import { Button, Tag } from 'jinge-antd';
import { vm, watch } from 'jinge';
import { ShareCode } from './Share';
import { CopyButton } from '@/components/Copy';
import { globalInst, loadGlobalInst } from '@/store/instance';

export function Instance() {
  const state = vm<{
    status?: string;
  }>({});

  function getStatus() {
    const inst = globalInst.data;
    if (!inst) return '未创建';
    if (inst.InstanceState === 'PENDING') {
      return '创建中...';
    } else if (inst.InstanceState === 'RUNNING') {
      return globalInst.state >= 3 ? '代理就绪' : '安装中...';
    } else {
      return inst.InstanceState;
    }
  }

  watch(
    globalInst,
    'state',
    () => {
      state.status = getStatus();
    },
    { immediate: true },
  );

  return (
    <>
      {globalInst.state >= 3 && (
        <div className='flex items-center'>
          <span className='mr-1 whitespace-nowrap'>远程地址：</span>
          <Tag className='mr-2 w-56 flex-shrink overflow-x-auto font-mono'>
            vmess://{globalInst.ip!}:2080
          </Tag>
          <div className='flex w-10 shrink-0 items-center'>
            <ShareCode ip={globalInst.ip!} />
          </div>
        </div>
      )}
      {globalInst.state === 4 && (
        <div className='flex'>
          <span className='mr-1 pt-1 whitespace-nowrap'>本地代理：</span>
          <div className='flex-shrink basis-56'>
            <div className='flex items-center'>
              <Tag className='mr-2 w-56 overflow-x-auto font-mono tracking-[0.08em]'>
                socks5://127.0.0.1:7890
              </Tag>
              <CopyButton text='socks5://127.0.0.1:7890' />
            </div>
            <div className='flex items-center'>
              <Tag className='mt-2 mr-2 w-56 overflow-x-auto font-mono tracking-[0.15em]'>
                http://127.0.0.1:7891
              </Tag>
              <CopyButton text='http://127.0.0.1:7891' />
            </div>
          </div>
        </div>
      )}
      <div className='flex items-center'>
        <span className='mr-1 whitespace-nowrap'>当前主机：</span>

        <Tag className='w-30'>{globalInst.data?.InstanceName ?? '-'}</Tag>
        {state.status && <span className='text-secondary-text text-sm'>（{state.status}）</span>}

        <Button
          loading={globalInst.loading}
          on:click={() => {
            void loadGlobalInst(globalInst.data?.InstanceId);
          }}
          slot:icon={<span className='icon-[ant-design--reload-outlined] text-base'></span>}
          size='sm'
          type='link'
        ></Button>
      </div>
      {globalInst.ip && (
        <div className='flex items-center'>
          <span className='mr-1 whitespace-nowrap'>公网地址：</span>
          <Tag className='mr-2 w-30'>{globalInst.ip}</Tag>
          <CopyButton text={`ssh ubuntu@${globalInst.ip}`} />
        </div>
      )}
    </>
  );
}
