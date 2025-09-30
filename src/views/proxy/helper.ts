import { invoke } from '@tauri-apps/api/core';
import { fetch } from '@tauri-apps/plugin-http';
import {
  type CVMInstance,
  CreateCommand,
  DescribeAutomationAgentStatus,
  DescribeCommands,
  DescribeInstances,
  ModifyCommand,
} from '@/service/tencent';
import { appendLog } from '@/store/log';
import { globalSettings } from '@/store/settings';

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

export async function pingServerOnce(ip: string) {
  if (!globalSettings.token) return false;
  try {
    const url = `http://${ip}:24816/ping`;
    appendLog(`[log::info] Ping ${url}`);
    const res = await fetch(url, {
      connectTimeout: 5000,
      headers: {
        'x-token': globalSettings.token,
      },
    });
    if (res.status !== 200) throw new Error(`bad response status: ${res.status}`);
    const txt = await res.text();
    if (txt === 'pong!') {
      appendLog('[log::info] 远程代理服务服务器正常响应！');
      return true;
    } else {
      return false;
    }
  } catch (ex) {
    console.error(ex);
    return false;
  }
}

export async function startProxyClient(
  ip: string,
  token: string,
  instanceId: string,
  proxyRules: string,
) {
  try {
    await invoke('tauri_start_proxy_client', {
      serverIp: ip,
      token,
      cvmId: instanceId,
      proxyRules,
    });
    return true;
  } catch (ex) {
    console.error(ex);
    return false;
  }
}
