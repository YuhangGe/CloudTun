import { defaultWindowIcon } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { Menu } from '@tauri-apps/api/menu';
import { TrayIcon } from '@tauri-apps/api/tray';
import { getCurrentWindow } from '@tauri-apps/api/window';

let tray: TrayIcon | undefined = undefined;
export async function initTray() {
  tray = await TrayIcon.new({
    id: 'app-tray',
    icon: (await defaultWindowIcon())!,
    tooltip: 'CloudV2Ray - 基于云主机的 v2ray 客户端',
    showMenuOnLeftClick: false,
    action: async (event) => {
      if (event.type === 'Click' && event.button === 'Left') {
        const win = getCurrentWindow();
        await win.show();
        await win.setFocus();
      }
    },
  });
  await tray.setMenu(
    await Menu.new({
      items: [
        {
          id: 'quit',
          text: '退出CloudV2Ray',
          action: async () => {
            await invoke('tauri_stop_v2ray_server');
            await invoke('tauri_exit_process');
          },
        },
      ],
    }),
  );
}
export function clearTray() {
  void tray?.close();
  tray = undefined;
}
