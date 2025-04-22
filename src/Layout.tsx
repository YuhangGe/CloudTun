import { Button, Spin, Tooltip, message } from 'jinge-antd';
import { invoke } from '@tauri-apps/api/core';
import { IS_MOBILE } from './service/util';
import { globalStore } from './store/global';

import {
  loadInstance,
  pingV2RayInterval,
  pingV2RayOnce,
  startV2RayCore,
} from './views/instance/helper';
import imgLogo from '@/assets/logo-128x128.png';
import { validateSettings } from './service/settings';
import { cx, onMount, vm, watch } from 'jinge';
import { appendLog } from './store/log';
import { SettingsView } from './views/settings';

const ViewItems = [
  {
    label: '概览',
    'slot:icon': <span className='icon-[material-symbols--overview-key-outline]'></span>,
    key: 'overview',
  },
  {
    label: '主机',
    'slot:icon': <span className='icon-[ant-design--cloud-server-outlined]'></span>,
    key: 'instance',
  },
  {
    label: '设置',
    'slot:icon': <span className='icon-[ant-design--setting-outlined]'></span>,
    key: 'settings',
  },
  {
    label: '日志',
    'slot:icon': <span className='icon-[tabler--logs]'></span>,
    key: 'logs',
  },
];

export function Layout() {
  const state = vm({
    loaded: false,
    view: validateSettings(globalStore.settings) != null ? 'settings' : 'overview',
    title: '',
  });

  watch(
    state,
    'view',
    (v) => {
      state.title = ViewItems.find((it) => it.key === v)?.label!;
    },
    { immediate: true },
  );

  const initialize = async () => {
    try {
      const [err, res] = await loadInstance();

      if (err || !res.InstanceSet.length) return;
      const inst = res.InstanceSet[0];
      globalStore.instance = inst;
      if (!(await pingV2RayOnce(inst))) {
        return;
      }
      globalStore.v2rayState = 'INSTALLED';
      appendLog('[ping] ==> 开始定时 Ping 服务');
      if (!pingV2RayInterval()) {
        void message.error('pingV2RayInterval 失败，请尝试退出后重启 CloudV2Ray。');
        return;
      }
      if (!IS_MOBILE && !(await startV2RayCore())) {
        void message.error('本地 v2ray-core 启动失败，请尝试退出后重启 CloudV2Ray。');
      }
    } catch (ex) {
      void message.error(`${ex}`);
    } finally {
      if (!globalStore.instance || globalStore.v2rayState !== 'INSTALLED') {
        state.view = 'instance';
      }
      state.loaded = true;
    }
  };
  onMount(() => {
    if (validateSettings(globalStore.settings) != null) {
      state.loaded = true;
    } else {
      void initialize();
    }
  });

  // const [x, setX] = useState(false);

  return state.loaded ? (
    <>
      <div className='border-border flex w-28 flex-shrink-0 flex-col border-r border-solid max-sm:hidden'>
        <div className='pt-[5px] pl-5'>
          <img src={imgLogo} className='size-16' />
        </div>
        {ViewItems.map((item) => (
          <div
            key={item.key}
            on:click={() => {
              const err = validateSettings(globalStore.settings);
              if (err != null) {
                void message.error(err);
                return;
              }
              state.view = item.key;
            }}
            className={cx(
              'hover:bg-hover flex w-full cursor-pointer items-center py-5 pl-5 text-lg hover:text-white',
              state.view === item.key && 'text-blue',
            )}
          >
            {item['slot:icon']}
            <span className='ml-2'>{item.label}</span>
          </div>
        ))}
        <div className='flex-1'></div>
        <Tooltip content='退出 CloudV2Ray，结束本地代理'>
          <Button
            on:click={async () => {
              await invoke('plugin:cloudv2ray|tauri_stop_v2ray_server');
              await invoke('tauri_exit_process');
            }}
            className='flex w-full items-center justify-center pt-2 pb-4'
            slot:icon={<span className='icon-[grommet-icons--power-shutdown]'></span>}
            type='link'
          />
        </Tooltip>
      </div>
      <div className='flex flex-1 flex-col overflow-x-hidden px-6 pt-6'>
        <div className='mb-4 flex items-center sm:mb-5'>
          <div className='flex items-center text-2xl sm:hidden'>
            <img src={imgLogo} className='block size-10' />
            <span className='ml-2 font-medium'>CloudV2Ray</span>
            <span className='mx-2'>-</span>
          </div>
          <div className='max-sm:text-secondary-text text-2xl whitespace-nowrap'>{state.title}</div>
          <div className='flex-1' />
          {/* <Button
            loading={x}
            onClick={async () => {
              setX(true);
              const r = await invoke('plugin:cloudv2ray|startVpn');
              console.log(r);
              setX(false);
            }}
          >
            T
          </Button> */}
          {/* <Dropdown
            
            menu={{
              items: ViewItems.map((item) => ({
                label: (
                  <div className='flex items-center gap-3 py-2 pl-1 pr-2'>
                    <span className='translate-y-0.5'>{item.icon}</span>
                    {item.label}
                  </div>
                ),
                key: item.key,
              })),
              onClick(info) {
                setView(info.key);
              },
            }}
          >
            <Button
              className='sm:hidden'
              slot:icon={<span className='icon-[ant-design--menu-outlined] shrink-0'></span>}
            />
          </Dropdown> */}
        </div>
        {state.view === 'settings' && <SettingsView />}
      </div>
    </>
  ) : (
    <div className='flex w-full items-center justify-center'>
      <Spin />
    </div>
  );
}
