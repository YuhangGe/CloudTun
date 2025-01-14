import { invoke } from '@tauri-apps/api/core';
import { fetch } from '@tauri-apps/plugin-http';
import { globalStore } from '@/store/global';
import configTpl from '@/assets/v2ray.conf.template.json?raw';
import {
  type CVMInstance,
  CreateCommand,
  DescribeAutomationAgentStatus,
  DescribeCommands,
  DescribeInstances,
  InvokeCommand,
  ModifyCommand,
} from '@/service/tencent';
import { renderTpl } from '@/service/util';
import { appendLog } from '@/store/log';
import { getInstanceAgentShell } from '@/service/instance';

export async function loadInstance(id?: string) {
  return await DescribeInstances({
    Limit: 1,
    Offset: 0,
    ...(id
      ? { InstanceIds: [id] }
      : {
          Filters: [
            {
              Name: 'instance-name',
              Values: [globalStore.get('settings').resourceName],
            },
          ],
        }),
  });
}

export async function waitInstanceReady(inst: CVMInstance) {
  while (true) {
    await new Promise((res) => setTimeout(res, 1000));
    const [err, res] = await loadInstance(inst.InstanceId);
    if (!err && res.InstanceSet.length) {
      inst = res.InstanceSet[0];
    }
    if (inst.InstanceState !== 'RUNNING' || !inst.PublicIpAddresses?.[0]) {
      appendLog('[agent] ==> Remote instance not ready. Try again.');
    } else {
      return inst;
    }
  }
}

export async function waitInstanceAutomationAgentReady(inst: CVMInstance) {
  while (true) {
    const [err, res] = await DescribeAutomationAgentStatus({
      InstanceIds: [inst.InstanceId],
    });
    if (!err && res.AutomationAgentSet.length) {
      const ag = res.AutomationAgentSet[0];
      if (ag.AgentStatus === 'Online') {
        return true;
      }
    }
    appendLog('[agent] ==> Automation Tool on instance not ready. Try again.');
    await new Promise((res) => setTimeout(res, 1000));
  }
}

export async function createOrUpdateCommand(shellContent: string) {
  const [e0, r1] = await DescribeCommands({
    Filters: [{ Name: 'command-name', Values: ['v2ray_agent'] }],
  });
  if (e0) return;
  let id = r1.CommandSet[0]?.CommandId;
  if (id) {
    appendLog('[agent] ==> 更新 V2Ray 安装自动化脚本');
    const [err] = await ModifyCommand({
      CommandId: id,
      Content: shellContent,
    });
    if (err) return;
  } else {
    appendLog('[agent] ==> 创建 V2Ray 安装自动化脚本');
    const [err, res] = await CreateCommand({
      CommandName: 'v2ray_agent',
      WorkingDirectory: '/home/ubuntu',
      Username: 'ubuntu',
      Timeout: 600,
      Content: shellContent,
    });
    if (err) return;
    id = res.CommandId;
  }
  return id;
}

export async function installV2RayAgent(inst: CVMInstance) {
  const shellContent = window.btoa(getInstanceAgentShell());
  const commandId = await createOrUpdateCommand(shellContent);
  if (!commandId) {
    appendLog('[agent] ==> 安装 V2Ray 自动化脚本执行失败');
    return false;
  }
  try {
    await InvokeCommand({
      CommandId: commandId,
      InstanceIds: [inst.InstanceId],
    });
  } catch (ex) {
    console.error(ex);
    appendLog('[agent] ==> 安装 V2Ray 自动化脚本执行失败');
    return false;
  }
  for (let i = 0; i < 150; i++) {
    await new Promise((res) => setTimeout(res, 2000));
    const pinged = await pingV2RayOnce(inst);
    if (pinged) {
      return true;
    }
  }
  return false; // timeout
}

export async function pingV2RayOnce(inst: CVMInstance) {
  const settings = globalStore.get('settings');
  if (!settings.token) return false;
  if (!inst) return false;
  const ip = inst.PublicIpAddresses?.[0];
  if (!ip) return false;
  try {
    const url = `http://${ip}:2081/ping?token=${settings.token}`;
    appendLog(`[ping] ==> ${url}`);
    const res = await fetch(url);
    if (res.status !== 200) throw new Error(`bad response status: ${res.status}`);
    const txt = await res.text();
    return txt === 'pong!';
  } catch (ex) {
    console.error(ex);
    return false;
  }
}

let pingInt = 0;
export function pingV2RayInterval() {
  const settings = globalStore.get('settings');
  if (!settings.token) return false;
  const inst = globalStore.get('instance');
  if (!inst) return false;
  const ip = inst.PublicIpAddresses?.[0];
  if (!ip) return false;
  if (pingInt) clearInterval(pingInt);
  pingInt = window.setInterval(
    async () => {
      const fail = () => {
        appendLog('[ping] ==> 服务器响应异常，可能是竞价实例被回收，请刷新主机信息后重新购买');
      };
      try {
        const res = await fetch(`http://${ip}:2081/ping?token=${settings.token}`);
        if (res.status !== 200) {
          return fail();
        }

        const txt = await res.text();
        if (txt !== 'pong!') {
          return fail();
        }
        appendLog('[ping] ==> 服务器正常响应');
      } catch (ex) {
        console.error(ex);
        fail();
      }
    },
    2 * 60 * 1000,
  );
  return true;
}
export function getV2RayCoreConf() {
  const settings = globalStore.get('settings');
  const inst = globalStore.get('instance');
  if (!inst) return '';
  const ip = inst.PublicIpAddresses?.[0];
  if (!ip) return '';
  return renderTpl(configTpl, {
    REMOTE_IP: ip,
    TOKEN: settings.token,
  });
}
export async function startV2RayCore() {
  const conf = getV2RayCoreConf();
  if (!conf) return false;
  try {
    await invoke('tauri_start_v2ray_server', {
      config: conf,
    });
    return true;
  } catch (ex) {
    console.error(ex);
    return false;
  }
}
