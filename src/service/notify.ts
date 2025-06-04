import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { currentMonitor } from '@tauri-apps/api/window';

let notifyWindow: WebviewWindow | undefined = undefined;

// void notifyWindow.once('tauri://window-created', async () => {
//   debugger
//   // loading embedded asset:
//   notifyWebview = new Webview(notifyWindow, 'notify-window-webview', {
//     url: 'path/to/page.html',

//     // create a webview with specific logical position and size
//     x: 0,
//     y: 0,
//     width: 800,
//     height: 600,
//   });
// })

const WINDOW_WIDTH = 220;
const WINDOW_HEIGHT = 80;

export async function showNotifyWindow() {
  // if (!notifyWebview) return;
  if (!notifyWindow) {
    const screen = await currentMonitor();
    if (!screen) throw new Error('screen not found');
    const sf = screen.scaleFactor;
    const sw = screen.size.width / sf;
    notifyWindow = new WebviewWindow('notifywindow', {
      hiddenTitle: true,
      alwaysOnTop: true,
      // decorations: false,
      title: 'CloudV2Ray - 通知',
      titleBarStyle: 'overlay',
      closable: false,
      minimizable: false,
      maximizable: false,
      resizable: false,
      backgroundColor: [0, 0, 0, 0],
      skipTaskbar: true,
      width: WINDOW_WIDTH,
      height: WINDOW_HEIGHT,
      y: 90,
      x: sw - WINDOW_WIDTH - 40,
      url: '/notify.html',
      // visible: false,
    });

    await new Promise((res) => {
      void notifyWindow?.once('tauri://webview-created', res);
    });
  }
  await notifyWindow.show();
}

export async function hideNotifyWindow() {
  await notifyWindow?.hide();
}
