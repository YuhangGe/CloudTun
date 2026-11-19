import './style.css';
import { bootstrap } from 'jinge';

import App from './App';
import { resumeExistingCloud } from './store/instance';
import { appendLog, initializeLogs } from './store/log';
import { loadGlobalSettings } from './store/settings';
import { initializeStartup } from './store/startup';

const root = document.querySelector('#root')!;
if (!root) throw new Error('#root not found');

window.onunhandledrejection = (evt) => {
  console.error(evt);
};
window.onerror = (evt) => {
  console.error(evt);
};

async function startApp() {
  await initializeLogs();
  await loadGlobalSettings();
  const target = await initializeStartup();
  // Start readiness probing in the background; the first frame uses the resolved menu.
  if (target.instance) {
    void resumeExistingCloud(target.instance).catch((error) => {
      appendLog(`[log::startup] 自动连接云主机失败：${String(error)}`);
    });
  }
  bootstrap(App, root as HTMLElement, { initialView: target.view });
}
void startApp().catch((error) => {
  console.error(error);
  root.textContent = '应用初始化失败，请重新打开应用';
});
