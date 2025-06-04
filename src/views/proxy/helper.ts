import { invoke } from '@tauri-apps/api/core';
import { fetch } from '@tauri-apps/plugin-http';
import configTpl from '@/assets/v2ray.conf.template.json?raw';
import {
  type CVMInstance,
  CreateCommand,
  DescribeAutomationAgentStatus,
  DescribeCommands,
  DescribeInstances,
  ModifyCommand,
} from '@/service/tencent';
import { renderTpl } from '@/service/util';
import { appendLog } from '@/store/log';
import { globalSettings } from '@/store/settings';
import { savePid } from '@/store/pid';

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
              Values: [globalSettings.resourceName],
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

// export async function installV2RayAgent(inst: CVMInstance) {
//   const shellContent = window.btoa(getInstanceAgentShell());
//   const commandId = await createOrUpdateCommand(shellContent);
//   if (!commandId) {
//     appendLog('[agent] ==> 安装 V2Ray 自动化脚本执行失败');
//     return false;
//   }
//   try {
//     await InvokeCommand({
//       CommandId: commandId,
//       InstanceIds: [inst.InstanceId],
//     });
//   } catch (ex) {
//     console.error(ex);
//     appendLog('[agent] ==> 安装 V2Ray 自动化脚本执行失败');
//     return false;
//   }
//   for (let i = 0; i < 150; i++) {
//     await new Promise((res) => setTimeout(res, 2000));
//     const pinged = await pingV2RayOnce(inst);
//     if (pinged) {
//       return true;
//     }
//   }
//   return false; // timeout
// }

export async function pingV2RayOnce(ip: string) {
  if (!globalSettings.token) return false;
  try {
    const url = `http://${ip}:2081/ping?token=${globalSettings.token}`;
    appendLog(`[ping] ==> ${url}`);
    const res = await fetch(url, { connectTimeout: 5000 });
    if (res.status !== 200) throw new Error(`bad response status: ${res.status}`);
    const txt = await res.text();
    return txt === 'pong!';
  } catch (ex) {
    console.error(ex);
    return false;
  }
}

export function getV2RayCoreConf(ip: string) {
  return renderTpl(configTpl, {
    REMOTE_IP: ip,
    TOKEN: globalSettings.token,
  });
}
export async function startV2RayCore(ip: string) {
  const conf = getV2RayCoreConf(ip);
  if (!conf) return false;
  try {
    const pid = await invoke<{ pid: string }>('tauri_start_v2ray_server', {
      config: conf,
    });
    void savePid(pid.pid);
    return true;
  } catch (ex) {
    console.error(ex);
    return false;
  }
}
