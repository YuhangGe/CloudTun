import { invoke } from '@tauri-apps/api/core';
import { Dropdown, type MenuOption } from 'jinge-antd';
import { Portal, onMount, ref, registerEvent, vm } from 'jinge';
import { clearTray } from './tray';

const ContextMenuOptions: MenuOption<string>[] = [
  {
    value: 'reload',
    'slot:icon': <span className='icon-[ant-design--reload-outlined]'></span>,
    label: '重新加载',
  },
  {
    value: 'dev',
    label: '开发面板',
    'slot:icon': <span className='icon-[oui--app-devtools]'></span>,
  },
  {
    value: 'quit',
    label: '退出程序',
    'slot:icon': <span className='icon-[grommet-icons--power-shutdown]'></span>,
  },
];
export function ContextMenu() {
  const state = vm({
    open: false,
  });
  const el = ref<HTMLDivElement>();
  onMount(() => {
    const handle = (ev: MouseEvent) => {
      if ((ev.target as HTMLElement).tagName === 'INPUT') return;
      ev.preventDefault();
      if (!el.value) return;
      el.value.style.left = `${ev.pageX}px`;
      el.value.style.top = `${ev.pageY}px`;
      state.open = true;
    };
    return registerEvent(document, 'contextmenu', handle);
  });

  return (
    <Portal>
      <Dropdown
        placement='bottom-start'
        open={state.open}
        on:openChange={(v) => (state.open = v)}
        options={ContextMenuOptions}
        on:change={(v) => {
          if (v === 'reload') {
            clearTray();
            history.replaceState(null, '', '/');
            location.reload();
          } else if (v == 'quit') {
            void invoke('tauri_exit_process');
          } else if (v == 'dev') {
            void invoke('tauri_open_devtools');
          }
        }}
      >
        <div className='bg-red fixed z-50 size-0' ref={el}></div>
      </Dropdown>
    </Portal>
  );
}
