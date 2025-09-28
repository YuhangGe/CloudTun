import { Button, Tag } from 'jinge-antd';
import { DescribeAccountBalance } from '@/service/tencent';
import { onMount, vm } from 'jinge';

export function Balance() {
  const state = vm<{
    price?: string;
    loading?: boolean;
  }>({});
  const loadPrice = async () => {
    state.loading = true;
    const [err, res] = await DescribeAccountBalance();
    state.loading = false;
    if (!err) {
      state.price = (res.CashAccountBalance / 100).toFixed(2);
    }
  };
  onMount(() => {
    void loadPrice();
  });

  return (
    <div className='flex items-center'>
      <span className='mr-1 whitespace-nowrap'>账户余额：</span>
      {state.price && <Tag className='mr-2 w-35'>¥{state.price}</Tag>}
      <Button
        loading={state.loading}
        on:click={() => {
          void loadPrice();
        }}
        slot:icon={<span className='icon-[ant-design--reload-outlined] text-base'></span>}
        size='sm'
        type='link'
      ></Button>
    </div>
  );
}
