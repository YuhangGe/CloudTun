
import type { CVMPrice } from '@/service/tencent';
import { InquiryPriceRunInstances } from '@/service/tencent';
import { globalStore } from '@/store/global';
import { validateSettings } from '@/service/settings';
import { vm, watch } from 'jinge';
import { Button } from 'jinge-antd';

export function Price() {
  const state = vm<{
    price?: CVMPrice,
    loading: boolean
  }>({
    loading: false,
  })

  const loadPrice = async () => {
    if (validateSettings(globalStore.settings) != null) {
      return;
    }
    state.loading = (true);
    const [err, res] = await InquiryPriceRunInstances();
    state.loading = (false);
    if (!err) {
      state.price = res.Price;
    }
  };

  watch(globalStore.settings, 'instanceType', () => {
    void loadPrice();
  })
  watch(globalStore.settings, 'imageId', () => {
    void loadPrice();
  });

  return (
    <div className='flex items-center gap-2'>
      <span className='whitespace-nowrap'>当前价格：</span>
      {state.price && (
        <>
          <span className='whitespace-nowrap'>
            ¥{state.price.InstancePrice.UnitPriceDiscount.toFixed(2)}/小时
          </span>
          <span className='whitespace-nowrap'>¥{state.price.BandwidthPrice.UnitPriceDiscount}/GB</span>
        </>
      )}
      <Button
        loading={state.loading}
        className='relative translate-y-[1.5px]'
        on:click={() => {
          void loadPrice();
        }}
        slot:icon={<span className='icon-[ant-design--reload-outlined]'></span>}
        size='sm'
        type='link'
      ></Button>
    </div>
  );
};
