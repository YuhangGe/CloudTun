import { desktopInvoke } from '@/service/desktop';
import {
  type CVMInstance,
  CreateCommand,
  DescribeAutomationAgentStatus,
  DescribeCommands,
  DescribeInstances,
  DescribeInstancesUserData,
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

const accessRequests = new Map<string, Promise<void>>();
async function ensureCloudAccess(instanceId: string) {
  if (await desktopInvoke<boolean>('tauri_has_cloud_access', { instanceId })) return;
  let pending = accessRequests.get(instanceId);
  if (!pending) {
    pending = (async () => {
      const [error, data] = await DescribeInstancesUserData(instanceId);
      if (error) throw error;
      const encoded = data.InstanceSet.find((item) => item.InstanceId === instanceId)?.Attributes
        .UserData;
      const token =
        encoded &&
        /(?:^|\n)export CLOUDTUN_ACCESS_TOKEN=([a-f0-9]{64})(?:\r?\n|$)/.exec(atob(encoded))?.[1];
      if (!token) throw new Error('此云主机使用旧版协议；请发布新版服务端后升级或重建主机');
      await desktopInvoke('tauri_remember_cloud_access', { instanceId, token });
      appendLog('[log::cloud] 已从腾讯云实例配置恢复 WSS 访问凭据');
    })();
    accessRequests.set(instanceId, pending);
  }
  try {
    await pending;
  } finally {
    accessRequests.delete(instanceId);
  }
}
export async function pingServerOnce(ip: string, token: string) {
  await ensureCloudAccess(token);
  return await desktopInvoke<boolean>('tauri_ping_cloud_server', { ip, token });
}
