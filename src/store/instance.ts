import { loadInstanceDependentResources } from '@/service/instance';
import { type CVMInstance, CreateInstance } from '@/service/tencent';
import { IS_MOBILE, IS_REOPEN } from '@/service/util';
import { loadInstance, pingServerOnce, startProxyClient } from '@/views/proxy/helper';
import { vm } from 'jinge';
import { type MessageInstance, message } from 'jinge-antd';
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

// let pingInt = 0;
// export function startPingV2RayInterval() {
//   if (pingInt) clearInterval(pingInt);
//   pingInt = window.setInterval(
//     async () => {
//       if (!globalInst.ip) return;
//       const ret = await pingV2RayOnce(globalInst.ip!);
//       if (!ret) {
//         appendLog('[ping] ==> 服务器响应异常，可能是竞价实例被回收，请刷新主机信息后重新购买');
//         if (IS_MOBILE) {
//           message.error('V2Ray 远程主机失联！');
//         } else {
//           void showNotifyWindow({ notifyType: 'error', notifyMessage: 'V2Ray 远程主机失联！' });
//         }
//       } else {
//         appendLog('[ping] ==> 服务器正常响应');
//       }
//     },
//     2 * 60 * 1000,
//   );
// }

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
  if (globalInst.data) {
    message.error('实例已创建？？');
    return;
  }
  if (IS_MOBILE) {
    msg = message.loading(S1);
  } else {
    void showNotifyWindow({ notifyType: 'processing', notifyMessage: S1 });
  }
  const deps = await loadInstanceDependentResources();
  if (!deps) {
    if (IS_MOBILE) {
      msg?.update({ content: E1, type: 'error' });
    } else {
      void showNotifyWindow({ notifyType: 'error', notifyMessage: E1 });
    }
    return false;
  }
  const [err, res] = await CreateInstance(deps);

  if (err) {
    if (IS_MOBILE) {
      msg?.update({ content: E1, type: 'error' });
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
        msg?.update({ content: S2 });
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
    if (IS_MOBILE) {
      // message.error('启动本地 v2ray core 失败，请尝试退出后重启 CloudV2Ray。');
    } else {
      await showNotifyWindow({ notifyType: 'error', notifyMessage: '启动 CloudTun 代理失败！' });
    }
  } else {
    globalInst.state = 4;
    if (IS_MOBILE) {
      // message.success('远程主机安装 V2Ray 完成，已启动本地 v2ray-core 代理！');
    } else {
      await showNotifyWindow({
        notifyType: 'success',
        notifyMessage: 'CloudTun 代理启动成功！',
      });
      // setTimeout(() => {
      //   void hideNotifyWindow();
      // }, 3000);
    }
  }
}
