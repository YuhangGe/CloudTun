import { Button, Popconfirm } from 'jinge-antd';

import { invoke } from '@tauri-apps/api/core';
import { TerminateInstance } from '@/service/tencent';
import { type WithEvents, type WithExpose, expose, vm } from 'jinge';
import { createGlobalInst, globalInst } from '@/store/instance';
import { IS_MOBILE } from '@/service/util';
import { globalSettings } from '@/store/settings';

export function Control(
  props: {
    vpnConnected?: boolean;
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
      await invoke('tauri_stop_v2ray_server');
    }
  }
  async function create() {
    state.creating = true;
    await createGlobalInst();
    state.creating = false;
  }
  async function toggleVpn() {
    if (state.vpnConnected) {
      await invoke('tauri_android_stop_vpn');
    } else {
      await invoke('tauri_android_start_vpn', {
        serverIp: globalInst.ip,
        cvmId: globalInst.data!.InstanceId,
        token: globalSettings.token,
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
    <div className='mt-6 flex flex-wrap items-center gap-2 border-t border-t-blue-200 pt-3'>
      {globalInst.data ? (
        <Popconfirm
          title='确认销毁主机？'
          placement='top-start'
          content='销毁后代理服务不可用，请重新创建主机'
          on:confirm={() => {
            void destroy();
          }}
        >
          <Button loading={state.destroing} size='sm' className='text-xs'>
            销毁主机
          </Button>
        </Popconfirm>
      ) : (
        <Button
          loading={state.creating}
          on:click={() => {
            void create();
          }}
          type='primary'
          size='sm'
          className='text-xs'
        >
          创建主机
        </Button>
      )}
      {IS_MOBILE && globalInst.state >= 3 && (
        <Button
          size='sm'
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
