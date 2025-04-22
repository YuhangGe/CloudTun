import { globalStore } from './global';
import type { LoadingMessage } from '@/service/util';
import { IS_MOBILE, loadingMessage } from '@/service/util';
import { loadInstanceDependentResources } from '@/service/instance';
import type { CVMInstance } from '@/service/tencent';
import { CreateInstance } from '@/service/tencent';
import {
  installV2RayAgent,
  pingV2RayInterval,
  pingV2RayOnce,
  startV2RayCore,
  waitInstanceAutomationAgentReady,
  waitInstanceReady,
} from '@/views/instance/helper';
import { message } from 'jinge-antd';
const abort = (msg: LoadingMessage) => {
  globalStore.v2rayState = 'NOT_INSTALLED';
  msg.close();
};
export const createInstanceAndInstallV2Ray = async () => {
  const msg = loadingMessage('正在创建主机...');

  globalStore.v2rayState = 'INSTALLING';

  const deps = await loadInstanceDependentResources();
  if (!deps) return abort(msg);
  const [err, res] = await CreateInstance(deps);
  if (err) return abort(msg);
  globalStore.instance = res;
  msg.update('正在等待主机启动...');
  const instWithEip = await waitInstanceReady(res);
  globalStore.instance = instWithEip;
  if (globalStore.settings.imageType === 'PUBLIC_IMAGE') {
    await installV2RayAgentOnInstance(instWithEip, msg);
  } else {
    msg.update('正在等待 v2ray 服务上线...');
    for (let i = 0; i < 150; i++) {
      await new Promise((res) => setTimeout(res, 2000));
      const pinged = await pingV2RayOnce(instWithEip);
      if (pinged) {
        return true;
      }
    }
    await afterInstanceReady(msg);
  }
};

async function afterInstanceReady(msg: LoadingMessage) {
  pingV2RayInterval();

  if (IS_MOBILE) {
    msg.end('远程主机安装 V2Ray 完成！');
  } else {
    const r = await startV2RayCore();
    if (!r) {
      abort(msg);
      void message.error('启动本地 v2ray core 失败，请尝试退出后重启 CloudV2Ray。');
      return;
    } else {
      globalStore.v2rayState = 'INSTALLED';
      msg.end('远程主机安装 V2Ray 完成，已启动本地 v2ray-core 代理！');
    }
  }
}
export const installV2RayAgentOnInstance = async (inst: CVMInstance, msg?: LoadingMessage) => {
  if (!msg) {
    msg = loadingMessage('正在等待远程主机自动化助手上线...');
  } else {
    msg.update('正在等待远程主机自动化助手上线...');
  }
  await waitInstanceAutomationAgentReady(inst);
  msg.update('正在远程主机上安装 v2ray...');
  const x = await installV2RayAgent(inst);
  if (!x) {
    abort(msg);
    void message.error('远程主机安装 v2ray 失败！请尝试重新安装。');
    return;
  }
  await afterInstanceReady(msg);
};
