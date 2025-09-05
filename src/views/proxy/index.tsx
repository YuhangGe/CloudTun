import { validateSettings } from '@/service/settings';
import { type CVMPrice, InquiryPriceRunInstances } from '@/service/tencent';
import { onMount, ref, vm, watch } from 'jinge';
import { Spin, Tag, message } from 'jinge-antd';
import { Bandwidth } from './Bandwind';
import { Balance } from './Balance';
import { Instance } from './Instance';
import { Control } from './Control';
import { globalSettings } from '@/store/settings';
import { globalInst, loadGlobalInst } from '@/store/instance';
import { IS_RELOAD, IS_REOPEN } from '@/service/util';

export function ProxyView() {
  const state = vm<{
    loading?: boolean;
    price?: CVMPrice;
  }>({
    loading: true,
  });

  const ctrl = ref<typeof Control>();

  async function loadPrice() {
    if (validateSettings(globalSettings) != null) {
      return;
    }
    const [err, res] = await InquiryPriceRunInstances();
    if (!err) {
      state.price = res.Price;
    }
  }

  watch(globalSettings, 'instanceType', () => {
    void loadPrice();
  });
  watch(
    globalSettings,
    'imageId',
    () => {
      void loadPrice();
    },
    {
      immediate: true,
    },
  );

  onMount(() => {
    if (globalInst.state == 0) {
      loadGlobalInst().then(
        () => {
          state.loading = false;
          if (
            !IS_RELOAD &&
            !IS_REOPEN &&
            globalSettings.autoProxy &&
            globalInst.state === 0 &&
            !globalInst.data
          ) {
            void message
              .info('即将创建主机~')
              .waitClose()
              .then(() => {
                ctrl.value?.create();
              });
          }
        },
        (ex) => {
          console.error(ex);
        },
      );
    } else {
      state.loading = false;
    }
  });

  return state.loading ? (
    <div className='flex px-1 pt-5'>
      <Spin size='sm' />
    </div>
  ) : (
    <div className='mt-2'>
      <div className='flex flex-col gap-4'>
        <div className='text-lg font-medium'>代理信息</div>
        <Instance />
        <Balance />
      </div>
      <div className='mt-6 flex flex-col gap-4'>
        <div className='text-lg font-medium'>主机配置</div>
        <div className='flex items-center'>
          <span className='mr-1 whitespace-nowrap'>实例规格：</span>
          <Tag className='w-30'>{globalSettings.instanceType}</Tag>
          {state.price && (
            <span className='text-secondary-text text-sm whitespace-nowrap'>
              （¥{state.price.InstancePrice.UnitPriceDiscount}/小时）
            </span>
          )}
        </div>
        <Bandwidth price={state.price} />
      </div>
      <Control ref={ctrl} />
    </div>
  );
}
