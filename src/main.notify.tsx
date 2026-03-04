import './style.css';

import App from './App.notify';
import { bootstrap } from 'jinge';

const root = document.querySelector('#root')!;
if (!root) throw new Error('#root not found');

window.onunhandledrejection = (evt) => {
  console.error(evt);
};
window.onerror = (evt) => {
  console.error(evt);
};

bootstrap(App, root as HTMLElement);
