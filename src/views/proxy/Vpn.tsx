import { Tag } from 'jinge-antd';

export function Vpn(props: { connected?: boolean }) {
  return (
    <div className='flex items-center'>
      <span className='mr-1 whitespace-nowrap'>VPN状态：</span>
      <Tag className='mr-2 w-35'>{props.connected ? '已开启' : '未开启'}</Tag>
    </div>
  );
}
