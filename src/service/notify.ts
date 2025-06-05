import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { currentMonitor } from '@tauri-apps/api/window';
import { currentPlatform } from './util';

let notifyWindow: WebviewWindow | undefined = undefined;

const WINDOW_WIDTH = 220;
const WINDOW_HEIGHT = 80;

let aniTm = 0;
let shownLeft = 0;
let hidenLeft = 0;
const vis = false;
let entering = true;
async function enter() {
  if (!notifyWindow) return;
  if (!vis) {
    await notifyWindow.show();
  }
  if (aniTm) clearTimeout(aniTm);
  aniTm = setTimeout(() => {
    void enter();
  });
}

function leave() {}

export async function showNotifyWindow() {
  // if (!notifyWebview) return;
  if (!notifyWindow) {
    const screen = await currentMonitor();
    if (!screen) throw new Error('screen not found');
    const sf = screen.scaleFactor;
    const sw = screen.size.width / sf;
    shownLeft = Math.round(sw - WINDOW_WIDTH - 40);
    hidenLeft = Math.round(sw);
    notifyWindow = new WebviewWindow('notifywindow', {
      hiddenTitle: true,
      alwaysOnTop: true,
      decorations: currentPlatform === 'macos' ? true : false,
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
      x: hidenLeft,
      url: '/notify.html',
      visible: false,
    });

    await new Promise((res) => {
      void notifyWindow?.once('tauri://webview-created', res);
    });
  }
  entering = true;
  enter();
}

export async function hideNotifyWindow() {
  await notifyWindow?.hide();
}
