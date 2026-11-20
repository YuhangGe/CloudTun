import { invoke } from '@tauri-apps/api/core';
import { onMount, onUnmount, ref, vm, watch } from 'jinge';
import { Button, Spin, Tag } from 'jinge-antd';

import {
  type CVMPrice,
  InquiryPriceRunInstances,
  canQuoteInstance,
  describeSystemDiskTypes,
} from '@/service/tencent';
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
    diskError: string;
    diskLoading: boolean;
    price?: CVMPrice;
    vpnConnected?: boolean;
  }>({
    loading: true,
    error: '',
    diskError: '',
    diskLoading: false,
  });

  const ctrl = ref<typeof Control>();
  let active = true;
  let priceRequest = 0;
  let priceTimer = 0;
  let diskRequest = 0;

  function schedulePrice() {
    clearTimeout(priceTimer);
    const request = ++priceRequest;
    const config = { ...globalSettings };
    state.price = undefined;
    if (!canQuoteInstance(config)) return;
    priceTimer = window.setTimeout(async () => {
      const [err, res] = await InquiryPriceRunInstances(config);
      if (active && request === priceRequest && !err) state.price = res.Price;
    }, 250);
  }

  for (const field of [
    'region',
    'zone',
    'instanceType',
    'diskType',
    'imageId',
    'bandWidth',
  ] as const) {
    watch(globalSettings, field, schedulePrice);
  }

  async function ensureDiskType() {
    const request = ++diskRequest;
    const { region, zone, instanceType, diskType } = globalSettings;
    state.diskError = '';
    state.diskLoading = false;
    if (!region || !zone || !instanceType || (diskType && diskType !== 'CLOUD_PREMIUM')) return;

    state.diskLoading = true;
    const [error, diskTypes] = await describeSystemDiskTypes({ region, zone, instanceType });
    if (!active || request !== diskRequest) return;
    state.diskLoading = false;
    if (
      region !== globalSettings.region ||
      zone !== globalSettings.zone ||
      instanceType !== globalSettings.instanceType ||
      diskType !== globalSettings.diskType
    )
      return;
    if (error) {
      state.diskError = '查询磁盘类型失败';
      return;
    }
    if (!diskTypes.includes(diskType)) globalSettings.diskType = diskTypes[0] ?? '';
    if (!diskTypes.length) state.diskError = '当前规格没有可用的 20 GiB 系统盘';
  }

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
    void ensureDiskType().then(() => {
      if (active) schedulePrice();
    });
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
  onUnmount(() => {
    active = false;
    clearTimeout(priceTimer);
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
        {state.diskError && (
          <div className="flex items-center gap-2 text-sm text-error" role="alert">
            {state.diskError}
            <Button size="sm" on:click={() => void ensureDiskType()}>
              重试
            </Button>
          </div>
        )}
        {state.diskLoading && <p className="text-sm text-secondary-text">正在查询磁盘类型…</p>}
      </div>
      <Control
        ref={ctrl}
        diskLoading={state.diskLoading}
        vpnConnected={state.vpnConnected}
        on:vpnConnectChanged={(v) => {
          state.vpnConnected = v;
        }}
      />
    </div>
  );
}
