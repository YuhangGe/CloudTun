import imgLogo from '@/assets/logo-128x128.png';
import { Spin } from 'jinge-antd';

function App() {
  return (
    <div className='bg-background/30 flex size-full flex-col overflow-hidden px-4 py-3 backdrop-blur-2xl'>
      <div className='flex items-center gap-2'>
        <img src={imgLogo} className='size-4' />
        <span className='text-sm'>CloudV2Ray - 通知</span>
      </div>
      <p className='mt-4 flex items-center gap-3'>
        <Spin size='sm' />
        <span className='text-secondary-text text-xl'>正在创建实例...</span>
      </p>
    </div>
  );
}

export default App;
