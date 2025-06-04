import './style.css';
import { bootstrap } from 'jinge';
import App from './App';
import { loadGlobalSettings } from './store/settings';
import { killPreviousPid } from './store/pid';

const root = document.querySelector('#root')!;
if (!root) throw new Error('#root not found');

window.onunhandledrejection = (evt) => {
  console.error(evt);
};
window.onerror = (evt) => {
  console.error(evt);
};

void Promise.all([killPreviousPid(), loadGlobalSettings()]).then(() => {
  bootstrap(App, root as HTMLElement);
});
