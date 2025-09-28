import { Button, Tag } from 'jinge-antd';
import { vm, watch } from 'jinge';
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
      {/* {globalInst.state >= 3 && (
        <div className='flex items-center'>
          <span className='mr-1 whitespace-nowrap'>远程地址：</span>
          <Tag className='mr-2 w-60 flex-shrink overflow-x-auto font-mono'>
            vmess://{globalInst.ip!}:2080
          </Tag>
          <div className='flex w-10 shrink-0 items-center'>
            <ShareCode ip={globalInst.ip!} />
          </div>
        </div>
      )} */}
      {globalInst.state === 4 && (
        <div className='flex items-center'>
          <span className='mr-1 whitespace-nowrap'>本地代理：</span>
          <div className='flex items-center'>
            <Tag className='mr-2 overflow-x-auto font-mono'>http://127.0.0.1:7892</Tag>
            <CopyButton text='http://127.0.0.1:7892' />
          </div>
        </div>
      )}
      <div className='flex items-center'>
        <span className='mr-1 whitespace-nowrap'>当前主机：</span>

        <Tag className='w-35'>{globalInst.data?.InstanceName ?? '-'}</Tag>
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
          <Tag className='mr-2 w-35'>{globalInst.ip}</Tag>
          <CopyButton text={`ssh ubuntu@${globalInst.ip}`} />
        </div>
      )}
    </>
  );
}
