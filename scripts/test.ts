import { WebSocket } from 'http';

const ws = new WebSocket('ws://127.0.0.1:24816/ws', {
  headers: {
    'x-connect-port': '443',
    'x-connect-host': 'baidu.com',
  },
});
ws.addEventListener('open', () => {
  console.log('ws open');
  ws.send('');
});
ws.addEventListener('message', (evt) => {
  console.log('ws message', evt);
});
ws.addEventListener('error', (evt) => {
  console.log('ws error', evt);
});
console.log('xxx');
