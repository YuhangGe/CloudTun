import { copyToClipboard } from '@/service/util';
import { Button, message } from 'jinge-antd';

export function CopyButton(props: { text: string; className?: string }) {
  return (
    <Button
      className={props.className}
      on:click={() => {
        void copyToClipboard(props.text).then(() => {
          message.success('已复制！');
        });
      }}
      slot:icon={<span className='icon-[ant-design--copy-outlined] text-base'></span>}
      size='sm'
      type='link'
    />
  );
}
