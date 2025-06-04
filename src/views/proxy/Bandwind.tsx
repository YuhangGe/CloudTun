import {
  Button,
  InputAddon,
  InputNumber,
  InputWrapper,
  Tag,
  Tooltip,
  message,
  modal,
  onModalConfirm,
} from 'jinge-antd';

import {
  type CVMInstance,
  type CVMPrice,
  ResetInstancesInternetMaxBandwidth,
} from '@/service/tencent';
import { isNumber, vm } from 'jinge';
import { globalSettings } from '@/store/settings';

function BandwidthEditModal(props: { bw: number; instanceId: string }) {
  const state = vm({
    bw: props.bw,
  });

  onModalConfirm(async () => {
    const [err] = await ResetInstancesInternetMaxBandwidth(props.instanceId, state.bw);
    if (!err) {
      void message.success('更新带宽大小成功！');
      return { result: state.bw };
    }
  });

  return (
    <>
      <div>
        <p className='text-secondary-text pt-2 pb-1 text-xs'>
          当前版本公网IP按使用流量计费，带宽大小不直接影响费用。
        </p>
        <div className='flex w-full items-center'>
          <label className='whitespace-nowrap'>带宽：</label>
          <InputWrapper className='flex-1'>
            <InputNumber
              noRoundedR
              value={state.bw}
              on:change={(v) => {
                state.bw = v;
              }}
              min={1}
              max={1000}
            />
            <InputAddon className='px-2'>Mbps</InputAddon>
          </InputWrapper>
        </div>
      </div>
    </>
  );
}

export function Bandwidth(props: { price?: CVMPrice; inst?: CVMInstance }) {
  const bandWidth = globalSettings.bandWidth ?? 1;
  const state = vm({
    submitting: false,
    bandWidth: bandWidth,
  });

  async function openModal() {
    const ret = await modal
      .show<{}>({
        title: '调整公网带宽大小',
        className: 'max-sm:w-[70vw]',
        'slot:content': (
          <BandwidthEditModal instanceId={props.inst!.InstanceId} bw={state.bandWidth} />
        ),
      })
      .waitForClose();
    if (isNumber(ret)) {
      state.bandWidth = ret;
      globalSettings.bandWidth = ret;
    }
  }
  return (
    <div className='flex items-center'>
      <span className='mr-1 whitespace-nowrap'>公网带宽：</span>
      <Tag className='w-30'>
        {globalSettings.bandWidth}
        <span className='ml-0.5'>Mbps</span>
      </Tag>
      {props.inst?.InternetAccessible && (
        <Tooltip content='调整带宽大小'>
          <Button
            on:click={() => {
              void openModal();
            }}
            className='translate-y-[1.5px] text-lg'
            slot:icon={<span className='icon-[tdesign--arrow-up-down-3] text-base'></span>}
            type='link'
            size='sm'
          />
        </Tooltip>
      )}
      {props.price && (
        <span className='text-secondary-text text-sm whitespace-nowrap'>
          （¥{props.price.BandwidthPrice.UnitPriceDiscount}/GB）
        </span>
      )}
    </div>
  );
}
