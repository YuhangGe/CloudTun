import { invoke } from '@tauri-apps/api/core';
import { onMount, ref, vm, watch } from 'jinge';
import { Button, Spin, Tag } from 'jinge-antd';

import { validateSettings } from '@/service/settings';
import { type CVMPrice, InquiryPriceRunInstances } from '@/service/tencent';
import { IS_ANDROID, IS_MOBILE } from '@/service/util';
import { globalInst, loadGlobalInst } from '@/store/instance';
import { globalSettings } from '@/store/settings';

import { Balance } from './Balance';
import { Bandwidth } from './Bandwind';
import { Control } from './Control';
import { Instance } from './Instance';
import { Vpn } from './Vpn';

export function ProxyView() {
  const state = vm<{
    loading?: boolean;
    error: string;
    price?: CVMPrice;
    vpnConnected?: boolean;
  }>({
    loading: true,
    error: '',
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

  async function load() {
    state.loading = !globalInst.data;
    state.error = '';
    try {
      await loadGlobalInst();
    } catch (cause) {
      if (!globalInst.data) state.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      state.loading = false;
    }
  }

  onMount(() => {
    if (IS_ANDROID) {
      invoke<boolean>('tauri_android_get_vpn_connected').then(
        (v) => {
          state.vpnConnected = v;
        },
        (err) => {
          console.error(err);
        },
      );
    }
    if (!IS_MOBILE || globalInst.state == 0) {
      void load();
    } else {
      state.loading = false;
    }
  });

  return state.loading ? (
    <div className="flex px-1 pt-5">
      <Spin size="sm" />
    </div>
  ) : state.error ? (
    <div className="mt-4 rounded-lg border border-error/25 p-4" role="alert">
      <p className="mb-3 text-sm wrap-break-word text-error">加载云主机失败：{state.error}</p>
      <Button size="sm" on:click={() => void load()}>
        重新加载
      </Button>
    </div>
  ) : (
    <div className="mt-3">
      <div className="flex flex-col gap-4">
        <div className="text-lg font-medium">代理信息</div>
        <Instance />
        {IS_MOBILE && <Vpn connected={state.vpnConnected} />}
        <Balance />
      </div>
      <div className="mt-6 flex flex-col gap-4">
        <div className="text-lg font-medium">主机配置</div>
        <div className="flex items-center">
          <span className="mr-1 whitespace-nowrap">实例规格：</span>
          <Tag className="w-35">{globalSettings.instanceType}</Tag>
          {state.price && (
            <span className="text-sm whitespace-nowrap text-secondary-text">
              （¥{state.price.InstancePrice.UnitPriceDiscount}/小时）
            </span>
          )}
        </div>
        <Bandwidth price={state.price} />
      </div>
      <Control
        ref={ctrl}
        vpnConnected={state.vpnConnected}
        on:vpnConnectChanged={(v) => {
          state.vpnConnected = v;
        }}
      />
    </div>
  );
}
