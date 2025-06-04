// import { App, Switch, Tag } from 'antd';
// import type { FC } from 'react';
// import { useState } from 'react';
// import { fetch } from '@tauri-apps/plugin-http';
// import type { CVMInstance } from '@/service/tencent';
// import { globalStore } from '@/store/global';
// import { appendLog } from '@/store/log';

// export async function enableV2RaySocks(inst: CVMInstance, enabled: boolean) {
//   const settings = globalStore.get('settings');
//   if (!settings.token) return false;
//   if (!inst) return false;
//   const ip = inst.PublicIpAddresses?.[0];
//   if (!ip) return false;
//   try {
//     const url = `http://${ip}:2081/socks?token=${settings.token}&enable=${enabled ? 'true' : 'false'}`;
//     appendLog(`[socks] ==> ${url}`);
//     const res = await fetch(url);
//     if (res.status !== 200) throw new Error(`bad response status: ${res.status}`);
//     const txt = await res.text();
//     return txt === 'ok!';
//   } catch (ex) {
//     console.error(ex);
//     return false;
//   }
// }

// export const Socks: FC<{ inst: CVMInstance }> = ({ inst }) => {
//   const { message } = App.useApp();
//   const [toggling, setToggling] = useState(false);
//   const [enabled, setEnabled] = useState(false);
//   const toggleSocksEnabled = async (enabled: boolean) => {
//     setEnabled(enabled);
//     try {
//       setToggling(true);
//       if (await enableV2RaySocks(inst, enabled)) {
//         void message.success(`远程主机已${enabled ? '开启' : '关闭'} socks 服务`);
//       } else {
//         throw 'failed';
//       }
//     } catch (ex) {
//       void message.error(`${ex}`);
//       setEnabled(!enabled); // rollback
//     } finally {
//       setToggling(false);
//     }
//   };

//   return (
//     <div className='flex gap-2'>
//       <span className='relative translate-y-[1.5px] whitespace-nowrap'>萨克代理：</span>
//       <div className='flex flex-1 flex-col gap-3'>
//         <div className='flex items-center gap-2'>
//           <Switch
//             loading={toggling}
//             value={enabled}
//             onChange={(v) => {
//               void toggleSocksEnabled(v);
//             }}
//           ></Switch>
//           <span>{enabled ? '已开启' : '未开启'}</span>
//         </div>
//         {enabled && (
//           <div className='flex w-full items-center overflow-x-hidden'>
//             <Tag className='mr-0 shrink basis-56 overflow-x-auto font-mono'>
//               socks5://{inst.PublicIpAddresses?.[0] ?? '-'}:2082
//             </Tag>
//             {/* 占位，和远程地址保持对齐 */}
//             <div className='ml-2 w-10'></div>
//           </div>
//         )}
//       </div>
//     </div>
//   );
// };
