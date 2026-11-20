import { vm } from 'jinge';
import { type MessageInstance, type MessageUpdateOptions, message } from 'jinge-antd';

import { loadInstanceDependentResources } from '@/service/instance';
import { showNotifyWindow } from '@/service/notify';
import { validateSettings } from '@/service/settings';
import { type CVMInstance, CreateInstance } from '@/service/tencent';
import { IS_MOBILE, generateStrongPassword } from '@/service/util';
import { loadInstance, pingServerOnce } from '@/views/proxy/helper';

import { connectCloudProxy, disconnectProxy } from './desktop';
import { appendLog } from './log';
import { globalSettings } from './settings';

export interface InstanceState {
  data?: CVMInstance;
  loading?: boolean;
  error?: string;
  purchasing?: boolean;
  ip?: string;
  id?: string;
  /**
   * 0: 实例未创建
   * 1: 实例已创建
   * 2: 远程代理服务启动中...
   * 3: 远程代理服务已连接
   * 4: 本地代理客户端已启动
   */
  state: number;
}

let msg: MessageInstance | undefined = undefined;

export const globalInst = vm<InstanceState>({
  loading: true,
  purchasing: false,
  state: 0,
});

function showMsg(content: string, type?: MessageUpdateOptions['type']) {
  if (msg) {
    msg.update({ content, type });
  } else {
    if (type === 'success') {
      message.success(content);
    } else if (type === 'error') {
      message.error(content);
    } else if (type === 'loading') {
      msg = message.loading(content);
    }
  }
}

export async function loadGlobalInst(id?: string, autoConnect = false) {
  const generation = cloudGeneration;
  if (autoConnect) cloudAutoConnect = true;
  globalInst.loading = true;
  globalInst.error = '';
  try {
    const [err, res] = await loadInstance(id);
    if (err) throw err;
    if (generation === cloudGeneration)
      await updateInst(res.InstanceSet[0], autoConnect, generation);
  } catch (cause) {
    const error = cause instanceof Error ? cause : new Error(String(cause));
    globalInst.error = error.message;
    appendLog(`[log::cloud] 加载云主机失败：${error.message}`);
    throw error;
  } finally {
    globalInst.loading = false;
  }
}

const S1 = '正在创建主机...';
const E1 = '创建失败！';
const S2 = '启动远程代理服务...';

export async function createGlobalInst() {
  if (globalInst.purchasing || globalInst.data) return false;
  const loginPwd = globalSettings.loginPwd || generateStrongPassword();
  const settingsError = validateSettings({ ...globalSettings, loginPwd });
  if (settingsError) {
    message.error(settingsError);
    return false;
  }
  globalSettings.loginPwd = loginPwd;
  globalInst.purchasing = true;
  let purchased = false;
  try {
    msg?.close();
    msg = undefined;
    if (!(await disconnectProxy())) return false;
    const generation = cloudGeneration;
    cloudAutoConnect = true;
    cloudAutoNotify = true;
    if (IS_MOBILE) showMsg(S1, 'loading');
    else void showNotifyWindow({ notifyType: 'processing', notifyMessage: S1 });
    const deps = await loadInstanceDependentResources();
    if (!deps) {
      if (IS_MOBILE) showMsg(E1, 'error');
      else void showNotifyWindow({ notifyType: 'error', notifyMessage: E1 });
      return false;
    }
    const [err, res] = await CreateInstance(deps);
    if (err) {
      if (IS_MOBILE) showMsg(E1, 'error');
      else void showNotifyWindow({ notifyType: 'error', notifyMessage: E1 });
      return false;
    }
    purchased = true;
    await loadGlobalInst(res.InstanceId, generation === cloudGeneration);
    return true;
  } finally {
    globalInst.purchasing = false;
    if (!purchased) cloudAutoConnect = false;
  }
}

let tm = 0;
let cloudGeneration = 0;
let cloudAutoConnect = false;
let cloudAutoNotify = true;
export async function resumeExistingCloud(instance: CVMInstance) {
  const generation = cloudGeneration;
  cloudAutoConnect = true;
  cloudAutoNotify = false;
  await updateInst(instance, true, generation);
}
export function pauseCloudProxy() {
  cloudAutoConnect = false;
  ++cloudGeneration;
  if (tm) clearTimeout(tm);
  tm = 0;
}
async function updateInst(inst: CVMInstance | undefined, autoConnect: boolean, generation: number) {
  autoConnect = autoConnect || cloudAutoConnect;
  globalInst.data = inst;
  if (!inst) {
    globalInst.ip = undefined;
    globalInst.state = 0;
    return;
  } else {
    globalInst.state = 1;
  }
  if (inst?.InstanceState === 'RUNNING') {
    globalInst.ip = inst.PublicIpAddresses[0];
  } else {
    globalInst.ip = undefined;
  }

  if (!globalInst.ip) {
    if (!['PENDING', 'RUNNING'].includes(inst.InstanceState)) {
      cloudAutoConnect = false;
      appendLog(`[log::startup] 云主机状态为 ${inst.InstanceState}，暂不能连接`);
      return;
    }
    if (tm) clearTimeout(tm);
    tm = window.setTimeout(() => {
      if (generation === cloudGeneration)
        void loadGlobalInst(inst?.InstanceId, autoConnect).catch(() => {});
    }, 2000);
  } else {
    if (autoConnect && cloudAutoNotify) {
      if (IS_MOBILE) {
        showMsg(S2, 'loading');
      } else {
        void showNotifyWindow({ notifyType: 'processing', notifyMessage: S2 });
      }
    }
    // Render the instance as soon as its metadata arrives; readiness probes run separately.
    if (autoConnect) await updateConnect(autoConnect, generation);
    else void updateConnect(false, generation);
  }
}

async function updateConnect(autoConnect: boolean, generation: number) {
  globalInst.state = 2;
  let ret: boolean;
  try {
    ret = await pingServerOnce(globalInst.ip!, globalInst.data!.InstanceId);
  } catch (error) {
    if (generation !== cloudGeneration) return;
    globalInst.error = error instanceof Error ? error.message : String(error);
    cloudAutoConnect = false;
    appendLog(`[log::cloud] ${globalInst.error}`);
    return;
  }
  if (generation !== cloudGeneration) return;
  if (ret) {
    globalInst.state = 3;
    if (autoConnect && cloudAutoConnect) {
      cloudAutoConnect = false;
      if (!IS_MOBILE) await connectGlobalProxy(cloudAutoNotify);
      else showMsg('远程 CloudTun 服务已就绪，可启动本地 VPN！', 'success');
    }
  } else {
    if (tm) clearTimeout(tm);
    tm = window.setTimeout(() => {
      if (generation === cloudGeneration) void updateConnect(autoConnect, generation);
    }, 2000);
  }
}

export async function connectGlobalProxy(notify = true) {
  if (!globalInst.ip || !globalInst.data) return;
  pauseCloudProxy();
  const connected = await connectCloudProxy(globalInst.ip, globalInst.data.InstanceId);
  if (connected) {
    globalInst.state = 4;
    if (notify)
      await showNotifyWindow({ notifyType: 'success', notifyMessage: 'CloudTun 代理启动成功！' });
  } else {
    showMsg('云上主机代理连接失败，请查看错误信息', 'error');
  }
}
