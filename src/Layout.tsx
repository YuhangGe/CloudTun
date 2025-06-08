import { Button, Dropdown, type MenuOption, Tooltip, message } from 'jinge-antd';
import { invoke } from '@tauri-apps/api/core';
import { globalSettings } from './store/settings';

import imgLogo from '@/assets/logo-128x128.png';
import { validateSettings } from './service/settings';
import { cx, vm, watch } from 'jinge';
import { SettingsView } from './views/settings';
import { LogView } from './views/logview';
import { ProxyView } from './views/proxy';

const ViewItems: MenuOption<string>[] = [
  {
    label: '代理',
    'slot:icon': <span className='icon-[material-symbols--wifi-proxy-outline]'></span>,
    value: 'proxy',
  },
  {
    label: '日志',
    'slot:icon': <span className='icon-[tabler--logs]'></span>,
    value: 'logs',
  },
  {
    label: '设置',
    'slot:icon': <span className='icon-[ant-design--setting-outlined]'></span>,
    value: 'settings',
  },
];

export function Layout() {
  const state = vm({
    view: validateSettings(globalSettings) != null ? 'settings' : 'proxy',
    title: '',
  });

  watch(
    state,
    'view',
    (v) => {
      state.title = ViewItems.find((it) => it.value === v)?.label!;
    },
    { immediate: true },
  );

  return (
    <>
      <div className='border-border flex w-28 flex-shrink-0 flex-col border-r border-solid max-sm:hidden'>
        <div className='pt-[5px] pl-5 max-sm:pl-3'>
          <img src={imgLogo} className='size-16' />
        </div>
        {ViewItems.map((item) => (
          <div
            key={item.value}
            on:click={() => {
              const err = validateSettings(globalSettings);
              if (err != null) {
                void message.error(err);
                return;
              }
              state.view = item.value;
            }}
            className={cx(
              'hover:bg-hover flex w-full cursor-pointer items-center py-5 pl-5 text-lg hover:text-white',
              state.view === item.value && 'text-blue',
            )}
          >
            {item['slot:icon']}
            <span className='ml-2'>{item.label}</span>
          </div>
        ))}
        <div className='flex-1'></div>
        <Tooltip content='退出 CloudV2Ray，结束本地代理' placement='top-start'>
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
      <div className='flex flex-1 flex-col overflow-x-hidden px-6 pt-6 max-sm:px-4'>
        <div className='mb-4 flex items-center sm:mb-5'>
          <div className='flex items-center text-2xl sm:hidden'>
            <img src={imgLogo} className='block size-10' />
            <span className='ml-2 font-medium'>CloudV2Ray</span>
            <span className='mx-2'>-</span>
          </div>
          <div className='max-sm:text-secondary-text text-2xl whitespace-nowrap'>{state.title}</div>
          <div className='flex-1' />
          {/* <button
            on:click={() => {
              void showNotifyWindow(1);
            }}
          >
            SHOW
          </button>
          <button on:click={() => {
            void hideNotifyWindow();
          }}>
            HIDE
          </button> */}
          <Dropdown
            placement='bottom-end'
            options={ViewItems}
            on:change={(v) => {
              state.view = v;
            }}
          >
            <Button
              className='sm:!hidden'
              type='link'
              slot:icon={
                <span className='icon-[ant-design--menu-outlined] shrink-0 text-xl'></span>
              }
            />
          </Dropdown>
        </div>
        {state.view === 'proxy' && <ProxyView />}
        {state.view === 'settings' && <SettingsView />}
        {state.view === 'logs' && <LogView />}
      </div>
    </>
  );
}
