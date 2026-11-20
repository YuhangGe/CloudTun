// Real transport failure test for an installed debug APK. No root or cloud VM needed.
// Credentials stay in memory; app subscription/settings files are restored in finally.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createServer as httpServer } from 'node:http';
import net from 'node:net';
import { homedir } from 'node:os';
import path from 'node:path';

const adb = process.env.ADB ?? path.join(homedir(), 'Library/Android/sdk/platform-tools/adb');
const pkg = 'com.cloudtun.app';
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const run = (...args) =>
  execFileSync(adb, args, {
    encoding: 'utf8',
    timeout: 30000,
    maxBuffer: 16 * 1024 * 1024,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
const read = (file) => {
  try {
    return run('shell', `run-as ${pkg} cat ${file}`);
  } catch {
    return null;
  }
};
function restore(file, value) {
  if (value === null) run('shell', `run-as ${pkg} rm -f ${file}`);
  else
    execFileSync(adb, ['shell', `run-as ${pkg} sh -c 'umask 077; cat > ${file}'`], {
      input: value,
      timeout: 10000,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
}
async function listen(server) {
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return server.address().port;
}
class Relay {
  enabled = true;
  sockets = new Set();
  constructor(node) {
    this.server = net.createServer((client) => {
      if (!this.enabled) {
        client.destroy();
        return;
      }
      const remote = net.connect(node.port, node.address);
      this.sockets.add(client);
      this.sockets.add(remote);
      client.on('error', () => {});
      remote.on('error', () => client.destroy());
      client.on('close', () => {
        this.sockets.delete(client);
        remote.destroy();
      });
      remote.on('close', () => {
        this.sockets.delete(remote);
        client.destroy();
      });
      client.pipe(remote).pipe(client);
    });
  }
  cut() {
    this.enabled = false;
    for (const socket of this.sockets) socket.destroy();
  }
  recover() {
    this.enabled = true;
  }
  close() {
    this.cut();
    this.server.close();
  }
}
class CDP {
  pending = new Map();
  next = 0;
  static async open(port) {
    const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    const page = pages.find((page) => page.type === 'page' && page.url !== 'about:blank');
    assert(page, 'No inspectable application page');
    const client = new CDP();
    client.socket = new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      client.socket.onopen = resolve;
      client.socket.onerror = reject;
    });
    client.socket.onmessage = ({ data }) => {
      const result = JSON.parse(data);
      const pending = client.pending.get(result.id);
      if (pending) {
        client.pending.delete(result.id);
        if (result.error) pending.reject(new Error(result.error.message));
        else pending.resolve(result.result);
      }
    };
    client.socket.onclose = () => {
      for (const pending of client.pending.values())
        pending.reject(new Error('Debugger connection closed'));
      client.pending.clear();
    };
    return client;
  }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
  async evaluate(expression) {
    const response = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    });
    if (response.exceptionDetails)
      throw new Error('WebView command failed; inspect application logs');
    return response.result.value;
  }
  invoke(command, args = {}) {
    return this.evaluate(
      `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`,
    );
  }
  close() {
    this.socket.close();
  }
}
async function appConnection() {
  run('shell', 'am', 'start', '-n', `${pkg}/.MainActivity`);
  for (let i = 0; i < 45; i++) {
    try {
      const sockets = run('shell', 'cat', '/proc/net/unix')
        .split('\n')
        .filter((line) => line.includes('@webview_devtools_remote_'));
      const socket = sockets.at(-1)?.split('@').at(-1);
      if (socket) {
        run('forward', 'tcp:9223', `localabstract:${socket}`);
        const client = await CDP.open(9223);
        if (
          await client.evaluate(
            'Boolean(document.querySelector("header") && window.__TAURI_INTERNALS__)',
          )
        )
          return client;
        client.close();
      }
    } catch {}
    if (i === 10 || i === 25) run('shell', 'am', 'start', '-n', `${pkg}/.MainActivity`);
    await delay(1000);
  }
  throw new Error('Application WebView not available');
}
function vmess(node, name, port) {
  const config = node.vmess;
  return (
    'vmess://' +
    Buffer.from(
      JSON.stringify({
        v: '2',
        ps: name,
        add: '127.0.0.1',
        port: String(port),
        id: config.uuid,
        aid: '0',
        net: config.network,
        tls: config.tls ? 'tls' : '',
        scy: config.cipher,
        host: config.host || node.address,
        sni: config.serverName,
        path: config.path,
      }),
    ).toString('base64')
  );
}
function notifications() {
  const dump = run('shell', 'dumpsys', 'notification', '--noredact');
  return dump
    .split(/(?=NotificationRecord\()/)
    .filter(
      (block) =>
        block.startsWith('NotificationRecord(') && block.slice(0, 800).includes('pkg=' + pkg),
    )
    .join('\n');
}
async function waitFor(label, predicate, timeout = 95000) {
  const start = Date.now();
  let report = 0;
  while (Date.now() - start < timeout) {
    if (await predicate()) {
      console.log('PASS:', label);
      return;
    }
    if (Date.now() - report > 15000) {
      console.log('Waiting:', label);
      report = Date.now();
    }
    await delay(2000);
  }
  throw new Error('Timed out: ' + label);
}

let app;
let frozen = false;
let relayA;
let relayB;
let subscriptionServer;
const reversePorts = [];
const files = ['subscription.json', 'settings.bin', 'android-failover.json'];
let backups;
let mutationStarted = false;
try {
  app = await appConnection();
  await app.invoke('tauri_proxy_status');
  backups = new Map(files.map((file) => [file, read(file)]));
  const original = JSON.parse(backups.get('subscription.json'));
  const first =
    original.nodes.find((node) => node.name === '新加坡-优化-GPT') ??
    original.nodes.find((node) => node.vmess && !node.unsupportedReason);
  const second =
    original.nodes.find((node) => node.name === '香港-优化') ??
    original.nodes.find((node) => node.vmess && !node.unsupportedReason && node.id !== first.id);
  assert(first && second, 'Two real supported subscription nodes are required');
  relayA = new Relay(first);
  relayB = new Relay(second);
  const portA = await listen(relayA.server);
  const portB = await listen(relayB.server);
  const body = [vmess(first, '自动切换测试 A', portA), vmess(second, '自动切换测试 B', portB)].join(
    '\n',
  );
  subscriptionServer = httpServer((request, response) => {
    response.writeHead(200, { 'Content-Type': 'text/plain' });
    response.end(body);
  });
  const subscriptionPort = await listen(subscriptionServer);
  for (const port of [portA, portB, subscriptionPort]) {
    run('reverse', `tcp:${port}`, `tcp:${port}`);
    reversePorts.push(port);
  }
  mutationStarted = true;
  const subscriptionUrl = `http://127.0.0.1:${subscriptionPort}/subscription`;
  await app.evaluate(`(() => {
    const tab = [...document.querySelectorAll('button')].find(e => e.innerText.trim() === '订阅节点');
    tab.click(); return true;
  })()`);
  await waitFor(
    'Android subscription controls finish startup work',
    () =>
      app.evaluate(`(() => {
    const input = document.querySelector('input[aria-label="订阅链接"]');
    const update = [...document.querySelectorAll('button')].find(e => e.innerText.trim() === '更新订阅');
    return Boolean(input && !input.disabled && update && !update.disabled && !document.body.innerText.includes('正在验证节点并建立代理'));
  })()`),
    45000,
  );
  await app.evaluate(`(() => {
    const input = document.querySelector('input[aria-label="订阅链接"]');
    input.value = ${JSON.stringify(subscriptionUrl)};
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.closest('form').requestSubmit(); return true;
  })()`);
  await waitFor(
    'controlled subscription updated and tested through Android UI',
    () =>
      app.evaluate(`
    ['自动切换测试 A', '自动切换测试 B'].every(name => {
      const label = [...document.querySelectorAll('strong')].find(e => e.innerText === name);
      return label && !label.closest('button').disabled;
    })`),
    40000,
  );
  const subscription = await app.invoke('tauri_get_subscription');
  const [a, b] = ['自动切换测试 A', '自动切换测试 B'].map((name) =>
    subscription.nodes.find((node) => node.name === name),
  );
  assert(a && b, 'Controlled subscription was not parsed');
  await app.evaluate(`(() => {
    const toggle = document.querySelector('button[aria-label="自动切换节点"]');
    if (toggle.getAttribute('aria-pressed') !== 'true') toggle.click();
    [...document.querySelectorAll('strong')].find(e => e.innerText === '自动切换测试 A').closest('button').click();
    return true;
  })()`);
  await waitFor(
    'Service failover policy enabled from Android UI',
    () => {
      const policy = JSON.parse(read('android-failover.json'));
      return policy.enabled && policy.node_ids.includes(a.id) && policy.node_ids.includes(b.id);
    },
    10000,
  );
  await app.evaluate(
    `(() => { [...document.querySelectorAll('button')].find(e => e.innerText.trim() === '连接节点').click(); return true; })()`,
  );
  await waitFor(
    'node A connected from Android UI',
    async () => {
      const status = await app.invoke('tauri_proxy_status');
      return status.running && status.nodeId === a.id;
    },
    30000,
  );
  const initial = await app.invoke('tauri_proxy_status');
  assert.equal(initial.nodeId, a.id);
  const before = await app.invoke('tauri_proxy_traffic');
  assert(before);
  await waitFor(
    'running notification names node A and contains traffic counters',
    () => {
      const text = notifications();
      return (
        text.includes('CloudTun · 节点：自动切换测试 A') &&
        text.includes('速率') &&
        text.includes('累计')
      );
    },
    15000,
  );
  await delay(1000); // Flush the normal settings store before recreating the WebView.
  await app.send('Page.reload', { ignoreCache: true });
  await waitFor(
    'WebView reload observes the existing Service session',
    async () => {
      try {
        if (!(await app.evaluate('Boolean(document.querySelector("header"))'))) return false;
        const status = await app.invoke('tauri_proxy_status');
        return status.running && status.nodeId === a.id;
      } catch {
        return false;
      }
    },
    30000,
  );
  const reloaded = await app.invoke('tauri_proxy_traffic');
  assert.equal(
    reloaded.sessionId,
    before.sessionId,
    'WebView reload must not restart the TUN session',
  );
  await app.send('Page.setWebLifecycleState', { state: 'frozen' });
  frozen = true;
  run('shell', 'input', 'keyevent', '3');
  run('shell', 'input', 'keyevent', '223');
  relayA.cut();
  await waitFor('Service switches A -> B with WebView frozen and screen off', () => {
    const text = notifications();
    return text.includes('CloudTun · 节点：自动切换测试 B') && text.includes('已自动切换节点');
  });
  await app.send('Page.setWebLifecycleState', { state: 'active' });
  frozen = false;
  const switched = await app.invoke('tauri_proxy_status');
  assert.equal(switched.nodeId, b.id);
  const after = await app.invoke('tauri_proxy_traffic');
  assert.equal(after.sessionId, before.sessionId);
  assert(after.totalBytes >= before.totalBytes);
  run('shell', 'input', 'keyevent', '224');
  run(
    'shell',
    'am',
    'start',
    '-a',
    'android.intent.action.VIEW',
    '-d',
    'https://www.google.com.hk/?service_failover_verify=1',
    'com.android.chrome',
  );
  run('forward', 'tcp:9224', 'localabstract:chrome_devtools_remote');
  let chrome;
  await waitFor(
    'Google Hong Kong HTTP 200 after Service failover',
    async () => {
      try {
        chrome?.close();
        chrome = await CDP.open(9224);
        const info = await chrome.evaluate(
          '({url:location.href,title:document.title,status:performance.getEntriesByType("navigation")[0]?.responseStatus,search:!!document.querySelector("textarea[name=q],input[name=q]")})',
        );
        return (
          info.url.includes('service_failover_verify') &&
          info.status === 200 &&
          info.title === 'Google' &&
          info.search
        );
      } catch {
        return false;
      }
    },
    45000,
  );
  chrome?.close();
  await waitFor(
    'notification cumulative traffic grows after browsing',
    async () => {
      const snapshot = await app.invoke('tauri_proxy_traffic');
      return snapshot.totalBytes > after.totalBytes;
    },
    15000,
  );
  await app.send('Page.setWebLifecycleState', { state: 'frozen' });
  frozen = true;
  relayB.cut();
  await waitFor('all candidates unavailable produces a Service notification', () =>
    notifications().includes('暂无可用节点'),
  );
  const failures = run('shell', 'logcat', '-d', '-s', 'CloudTunVpnService:I');
  assert(failures.includes('暂无可用节点'));
  relayB.recover();
  await waitFor('network restoration produces a recovery notification', () =>
    notifications().includes('连接已恢复'),
  );
  await app.send('Page.setWebLifecycleState', { state: 'active' });
  frozen = false;
  await app.invoke('tauri_stop_proxy_client');
  assert.equal((await app.invoke('tauri_proxy_status')).running, false);
  assert.equal(await app.invoke('tauri_proxy_traffic'), null);
  relayA.recover();
  await delay(4000);
  assert.equal((await app.invoke('tauri_proxy_status')).running, false);
  const pid = run('shell', 'pidof', pkg).trim();
  assert(!run('shell', `run-as ${pkg} ls -l /proc/${pid}/fd`).includes('-> /dev/tun'));
  console.log('PASS: manual disconnect clears VPN, releases TUN and stays disconnected');
  console.log('All Android Service failover integration checks passed.');
} finally {
  if (frozen)
    try {
      await app.send('Page.setWebLifecycleState', { state: 'active' });
    } catch {}
  if (mutationStarted) {
    try {
      await app.invoke('tauri_stop_proxy_client');
    } catch {}
    run('shell', 'am', 'force-stop', pkg);
    for (const [file, value] of backups) restore(file, value);
    console.log('Restored original subscription, settings and failover policy.');
  }
  app?.close();
  relayA?.close();
  relayB?.close();
  subscriptionServer?.closeAllConnections();
  subscriptionServer?.close();
  for (const port of reversePorts)
    try {
      run('reverse', '--remove', `tcp:${port}`);
    } catch {}
  run('shell', 'input', 'keyevent', '224');
  run('shell', 'am', 'start', '-n', `${pkg}/.MainActivity`);
}
