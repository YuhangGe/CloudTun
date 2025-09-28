import { loadInstanceDependentResources } from '@/service/instance';
import { type CVMInstance, CreateInstance } from '@/service/tencent';
import { IS_MOBILE, IS_REOPEN } from '@/service/util';
import { loadInstance, pingServerOnce, startProxyClient } from '@/views/proxy/helper';
import { vm } from 'jinge';
import { type MessageInstance, type MessageUpdateOptions, message } from 'jinge-antd';
// import { appendLog } from './log';
import { showNotifyWindow } from '@/service/notify';
import { invoke } from '@tauri-apps/api/core';
import { globalSettings } from './settings';

export interface InstanceState {
  data?: CVMInstance;
  loading?: boolean;
  ip?: string;
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

export async function loadGlobalInst(id?: string) {
  globalInst.loading = true;
  const [err, res] = await loadInstance(id);
  globalInst.loading = false;
  await updateInst(err ? undefined : res.InstanceSet[0]);
}

const S1 = '正在创建主机...';
const E1 = '创建失败！';
const S2 = '启动远程代理服务...';

export async function createGlobalInst() {
  msg?.close();
  msg = undefined;

  if (globalInst.data) {
    showMsg('实例已创建？？', 'error');
    return;
  }
  if (IS_MOBILE) {
    showMsg(S1, 'loading');
  } else {
    void showNotifyWindow({ notifyType: 'processing', notifyMessage: S1 });
  }
  const deps = await loadInstanceDependentResources();
  if (!deps) {
    if (IS_MOBILE) {
      showMsg(E1, 'error');
    } else {
      void showNotifyWindow({ notifyType: 'error', notifyMessage: E1 });
    }
    return false;
  }
  const [err, res] = await CreateInstance(deps);

  if (err) {
    if (IS_MOBILE) {
      showMsg(E1, 'error');
    } else {
      void showNotifyWindow({ notifyType: 'error', notifyMessage: E1 });
    }
    return false;
  } else {
    await loadGlobalInst(res.InstanceId);
  }

  return true;
}

let tm = 0;
async function updateInst(inst?: CVMInstance) {
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
    if (tm) clearTimeout(tm);
    tm = window.setTimeout(() => {
      void loadGlobalInst(inst?.InstanceId);
    }, 2000);
  } else {
    if (!IS_REOPEN) {
      if (IS_MOBILE) {
        showMsg(S2, 'loading');
      } else {
        void showNotifyWindow({ notifyType: 'processing', notifyMessage: S2 });
      }
    }
    await updateConnect();
  }
}

async function updateConnect() {
  globalInst.state = 2;
  const ret = await pingServerOnce(globalInst.ip!);
  if (ret) {
    globalInst.state = 3;
    if (!IS_REOPEN) {
      if (!IS_MOBILE) {
        await invoke('tauri_interval_ping_start', {
          ip: globalInst.ip!,
          token: globalSettings.token,
        });
        await enableProxy();
      } else {
        showMsg('远程 CloudTun 服务已就绪，可启动本地 VPN！', 'success');
      }
    } else {
      globalInst.state = 4;
    }
  } else {
    if (tm) clearTimeout(tm);
    tm = window.setTimeout(() => {
      void updateConnect();
    }, 2000);
  }
}

async function enableProxy() {
  const r = await startProxyClient(
    globalInst.ip!,
    globalSettings.token,
    globalInst.data!.InstanceId,
  );
  if (!r) {
    showMsg('远程 CloudTun 服务启动失败！', 'error');
  } else {
    globalInst.state = 4;
    await showNotifyWindow({
      notifyType: 'success',
      notifyMessage: 'CloudTun 代理启动成功！',
    });
  }
}
