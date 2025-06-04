import { loadInstanceDependentResources } from '@/service/instance';
import { type CVMInstance, CreateInstance } from '@/service/tencent';
import { IS_MOBILE } from '@/service/util';
import { loadInstance, pingV2RayOnce, startV2RayCore } from '@/views/proxy/helper';
import { vm } from 'jinge';
import { message } from 'jinge-antd';

export interface InstanceState {
  data?: CVMInstance;
  loading?: boolean;
  ip?: string;
  /**
   * 0: 实例未创建
   * 1: 实例已创建
   * 2: v2ray 安装中...
   * 3: v2ray 已连接
   * 4: 本地 v2ray 已启动
   */
  state: number;
}

export const globalInst = vm<InstanceState>({
  loading: true,
  state: 0,
});

export async function loadGlobalInst(id?: string) {
  globalInst.loading = true;
  const [err, res] = await loadInstance(id);
  globalInst.loading = false;
  await updateInst(err ? undefined : res.InstanceSet[0]);
}

export async function createGlobalInst() {
  const deps = await loadInstanceDependentResources();
  if (!deps) {
    message.error('创建失败！');

    return false;
  }
  const [err, res] = await CreateInstance(deps);

  if (err) {
    message.error('创建失败！');
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
    await updateConnect();
  }
}

async function updateConnect() {
  globalInst.state = 2;
  const ret = await pingV2RayOnce(globalInst.ip!);
  if (ret) {
    globalInst.state = 3;
    // state.status = getStatus();
    if (!IS_MOBILE) {
      await enableProxy();
    }
  } else {
    if (tm) clearTimeout(tm);
    tm = window.setTimeout(() => {
      void updateConnect();
    }, 2000);
  }
}

async function enableProxy() {
  const r = await startV2RayCore(globalInst.ip!);
  if (!r) {
    message.error('启动本地 v2ray core 失败，请尝试退出后重启 CloudV2Ray。');
    return;
  } else {
    globalInst.state = 4;
    message.success('远程主机安装 V2Ray 完成，已启动本地 v2ray-core 代理！');
  }
}
