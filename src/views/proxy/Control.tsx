import { invoke } from '@tauri-apps/api/core';
import { type WithEvents, type WithExpose, expose, vm } from 'jinge';
import { Button, Popconfirm } from 'jinge-antd';

import { TerminateInstance } from '@/service/tencent';
import { IS_ANDROID, IS_MOBILE } from '@/service/util';
import { desktop, disconnectProxy } from '@/store/desktop';
import {
  createGlobalInst,
  globalInst,
  connectGlobalProxy,
  pauseCloudProxy,
} from '@/store/instance';
import { globalSettings } from '@/store/settings';

export function Control(
  props: {
    vpnConnected?: boolean;
    diskLoading: boolean;
  } & WithExpose<{
    create: () => void;
  }> &
    WithEvents<{
      vpnConnectChanged(v: boolean): void;
    }>,
) {
  const state = vm<{
    creating?: boolean;
    destroing?: boolean;
    vpnConnected?: boolean;
  }>({
    vpnConnected: props.vpnConnected,
  });
  async function destroy() {
    if (!globalInst.data) return;
    if (props.vpnConnected) {
      //
    }
    state.destroing = true;
    const [err] = await TerminateInstance(globalInst.data!.InstanceId);
    state.destroing = false;
    if (!err) {
      globalInst.data = undefined;
      globalInst.state = 0;
      globalInst.ip = undefined;
      if (IS_MOBILE) {
        if (IS_ANDROID) {
          await invoke('tauri_android_stop_vpn');
        }
      } else {
        pauseCloudProxy();
        if (desktop.status.source === 'cloud') await disconnectProxy();
      }
    }
  }
  async function create() {
    if (props.diskLoading) return;
    state.creating = true;
    try {
      await createGlobalInst();
    } finally {
      state.creating = false;
    }
  }
  async function toggleVpn() {
    if (state.vpnConnected) {
      await invoke('tauri_android_stop_vpn');
    } else {
      await invoke('tauri_android_start_vpn', {
        serverIp: globalInst.ip,
        cvmId: globalInst.data!.InstanceId,
        proxyApps: globalSettings.mobileProxyMode === 'app' ? globalSettings.mobileProxyApps : '',
      });
    }
    state.vpnConnected = !state.vpnConnected;
    props['on:vpnConnectChanged'](state.vpnConnected);
  }

  expose({
    create,
  });

  return (
    <div className="mt-6 flex flex-wrap items-center gap-2 border-t border-t-blue-200 pt-3">
      {globalInst.data ? (
        <Popconfirm
          title="确认销毁主机？"
          placement="top-start"
          content="销毁后代理服务不可用，请重新创建主机"
          on:confirm={() => {
            void destroy();
          }}
        >
          <Button loading={state.destroing} size="sm" className="text-xs">
            销毁主机
          </Button>
        </Popconfirm>
      ) : (
        <Button
          loading={state.creating || globalInst.purchasing}
          disabled={props.diskLoading || desktop.connecting || desktop.updating}
          on:click={() => {
            void create();
          }}
          type="primary"
          size="sm"
          className="text-xs"
        >
          购买主机
        </Button>
      )}
      {!IS_MOBILE && desktop.error && (
        <p className="w-full text-xs text-error" role="alert">
          {desktop.error}
        </p>
      )}
      {!IS_MOBILE && globalInst.data && (
        <Button
          type={desktop.status.source === 'cloud' && desktop.status.running ? 'default' : 'primary'}
          size="sm"
          loading={desktop.connecting}
          disabled={
            state.creating ||
            state.destroing ||
            desktop.updating ||
            (globalInst.state < 3 && desktop.status.source !== 'cloud')
          }
          on:click={() => {
            if (desktop.status.source === 'cloud' && desktop.status.running) void disconnectProxy();
            else void connectGlobalProxy();
          }}
        >
          {desktop.status.source === 'cloud' && desktop.status.running ? '断开代理' : '连接代理'}
        </Button>
      )}
      {IS_MOBILE && globalInst.state >= 3 && (
        <Button
          size="sm"
          on:click={() => {
            void toggleVpn();
          }}
        >
          {state.vpnConnected ? '关闭 VPN' : '开启 VPN'}
        </Button>
      )}
    </div>
  );
}
