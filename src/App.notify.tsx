import imgLogo from '@/assets/logo-128x128.png';
import { vm } from 'jinge';
import { Spin } from 'jinge-antd';

function App() {
  const state = vm({
    type: 0
  });

  return (
    <div className='bg-background flex size-full flex-col overflow-hidden px-4 py-3 backdrop-blur-2xl select-none'>
      <div className='flex items-center gap-2'>
        <img src={imgLogo} className='size-[18px]' />
        <span className='text-sm'>CloudV2Ray</span>
        <span className='flex-1'></span>
        <span className="icon-[gravity-ui--broadcast-signal] text-base text-primary"></span>
      </div>
      {state.type === 0 && <p className='mt-4 flex items-center gap-3'>
        <Spin size='sm' />
        <span className='text-secondary-text text-[16px]'>正在创建实例...</span>
      </p>}
      {state.type === 1 && <p className='mt-4 flex items-center gap-3'>
        <Spin size='sm' />
        <span className='text-secondary-text text-[16px]'>正在启动V2Ray...</span>
      </p>}
      {state.type === 2 && <p className='mt-4 flex items-center gap-2 text-success'>
        <span className="icon-[ant-design--check-circle-outlined] text-[21px]"></span>
        <span className='text-[16px]'>已启动本地代理！</span>
      </p>}
      {state.type === 3 && <p className='mt-4 flex items-center gap-2 text-orange'>
        <span className="icon-[ant-design--info-circle-outlined] text-[21px]"></span>
        <span className='text-[16px]'>远程主机失联！</span>
      </p>}

    </div>
  );
}

export default App;
