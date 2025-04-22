import { platform } from '@tauri-apps/plugin-os';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { sendNotification } from '@tauri-apps/plugin-notification';
import { message } from 'jinge-antd';

export const currentPlatform = platform();

const pwd = [
  'abcdefghijklmnopqrstuvwxyz', // lower chars
  'ABCDEFGHIJKLMNOPQRSTUVWXYZ', // upper chars
  '0123456789', // number
  '@$%',
  // "`!?$?%^&*()_-+={[}]:;@'~#|\\<>.?/];", //special chars
];
export function generateStrongPassword() {
  return new Array(20)
    .fill(0)
    .map(() => {
      const c = pwd[Math.floor(Math.random() * pwd.length)];
      return c[Math.floor(Math.random() * c.length)];
    })
    .join('');
}
export const copyToClipboard = (textToCopy: string) => {
  return writeText(textToCopy);
};
export function uid() {
  return Date.now().toString(32) + Math.floor(Math.random() * 0xffffff).toString(32);
}

export function renderTpl(tpl: string, ctx: Record<string, unknown>) {
  Object.entries(ctx).forEach(([k, v]) => {
    const r = new RegExp(`\\$${k}\\$`, 'g');
    tpl = tpl.replace(r, v as string);
  });
  return tpl;
}

export interface LoadingMessage {
  update: (title: string) => void;
  end: (title: string, type?: 'success' | 'error') => void;
  close: () => void;
}
export function loadingMessage(title: string): LoadingMessage {
  const msg = message.loading({
    content: title,
  });

  sendNotification({ title });
  return {
    update(title: string) {
      msg.update({
        content: title,
      });
    },
    end(title: string, type = 'success') {
      msg.update({
        duration: 4,
        content: title,
        type,
      });
    },
    close() {
      msg.close();
    },
  };
}

export const IS_MOBILE = currentPlatform === 'android' || currentPlatform === 'ios';
export const IS_IOS = currentPlatform === 'ios';
export const IS_ANDROID = currentPlatform === 'android';
