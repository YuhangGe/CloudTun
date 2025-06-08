import { LogicalPosition, type Window, currentMonitor } from '@tauri-apps/api/window';
import { currentPlatform } from './util';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

let notifyWindow: Window | undefined = undefined;

const WINDOW_WIDTH = 220;
const WINDOW_HEIGHT = 80;

let aniTm = 0;
let enteredLeft = 0;
let leavedLeft = 0;
let curLeft = 0;
const posY = 90;
let state: 'entered' | 'entering' | 'leaved' | 'leaving' = 'leaved';
async function enter() {
  if (!notifyWindow) return;
  if (state === 'entered' || state === 'entering') {
    return;
  }
  if (state === 'leaving') {
    clearInterval(aniTm);
  } else if (state === 'leaved') {
    await notifyWindow.setPosition(new LogicalPosition(leavedLeft, posY));
    await notifyWindow.show();
  }
  state = 'entering';
  aniTm = window.setInterval(() => {
    curLeft -= 10;
    if (curLeft <= enteredLeft) {
      curLeft = enteredLeft;
      clearInterval(aniTm);
      state = 'entered';
    }
    void notifyWindow?.setPosition(new LogicalPosition(curLeft, posY));
  }, 10);
}

async function leave() {
  if (!notifyWindow) return;
  if (state === 'leaved' || state === 'leaving') {
    return;
  }
  if (state === 'entering') {
    clearInterval(aniTm);
  }
  state = 'entering';
  aniTm = window.setInterval(() => {
    curLeft += 10;
    if (curLeft >= leavedLeft) {
      curLeft = leavedLeft;
      clearInterval(aniTm);
      state = 'leaved';
      void notifyWindow?.destroy();
      notifyWindow = undefined;
    } else {
      void notifyWindow?.setPosition(new LogicalPosition(curLeft, posY));
    }
  }, 10);
}
export interface Notify {
  type: 'error' | 'success' | 'processing';
  message: string;
}

export async function showNotifyWindow(notify: Notify) {
  if (!notifyWindow) {
    const screen = await currentMonitor();
    if (!screen) throw new Error('screen not found');
    const sf = screen.scaleFactor;
    const sw = screen.size.width / sf;
    enteredLeft = Math.round(sw - WINDOW_WIDTH - 40);
    leavedLeft = Math.round(sw);
    curLeft = leavedLeft;
    notifyWindow = new WebviewWindow('notifyWindow', {
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
      y: posY,
      x: 0,
      url: `/notify.html?type=${notify.type}&message=${encodeURIComponent(notify.message)}`,
      visible: false,
    });
    void notifyWindow?.once('tauri://error', (err) => {
      console.error(err);
    });
    await new Promise<void>((res) => {
      void notifyWindow?.once('tauri://created', async () => {
        res();
      });
    });
    await notifyWindow?.setPosition(new LogicalPosition(leavedLeft, posY));
  }

  await notifyWindow.emit('notify-state-changed', notify);
  await enter();
}

export async function hideNotifyWindow() {
  await leave();
}
