// import type { PluginListener } from '@tauri-apps/api/core';
// import { invoke } from '@tauri-apps/api/core';
// import { App, Switch } from 'antd';
// import type { FC } from 'react';
// import { useEffect, useRef, useState } from 'react';
// import { getV2RayCoreConf } from './helper';
// import { IS_MOBILE } from '@/service/util';
// import tun2socksConf from '@/assets/tun2socks.conf.template.yaml?raw';

// export const Sysproxy: FC = () => {
//   const { message } = App.useApp();
//   const [toggling, setToggling] = useState(false);
//   const [sysproxy, setSysprox] = useState(false);
//   const lis = useRef<PluginListener>(null);
//   // type Arg = {
//   //   vpnFd: number;
//   // };
//   // const handleVpn = async (arg: Arg, enabled: boolean) => {
//   //   try {
//   //     await invoke('plugin:cloudv2ray|tauri_start_tun2socks_server', {
//   //       ...arg,
//   //     });
//   //     setSysprox(enabled);
//   //     void message.success('VPN 启用成功！');
//   //   } catch (ex) {
//   //     void message.error(`${ex}`);
//   //   } finally {
//   //     setToggling(false);
//   //   }
//   // };
//   const toggleMobileVpn = async (enabled: boolean) => {
//     try {
//       setToggling(true);

//       if (enabled) {
//         // if (lis.current) await lis.current.unregister();
//         // lis.current = await addPluginListener('cloudv2ray', 'mobile::vpn', (arg: Arg) => {
//         //   if (lis.current) {
//         //     void lis.current.unregister(); // listen only once
//         //     lis.current = undefined;
//         //   }
//         //   void handleVpn(arg, enabled);
//         // });
//         await invoke('plugin:cloudv2ray|startVpn', {
//           v2rayConf: getV2RayCoreConf(),
//           tun2socksConf,
//         });
//       } else {
//         await invoke('plugin:cloudv2ray|stopVpn');
//         setSysprox(false);
//         setToggling(false);
//         void message.warning('VPN 已关闭！');
//       }
//     } catch (ex) {
//       void message.error(`${ex}`);
//       setToggling(false);
//     }
//   };
//   const toggleSysproxy = async (enabled: boolean) => {
//     try {
//       setToggling(true);
//       await invoke('tauri_set_sysproxy', {
//         port: 7890,
//         enabled,
//       });
//       setSysprox(enabled);
//     } catch (ex) {
//       void message.error(`${ex}`);
//     } finally {
//       setToggling(false);
//     }
//   };
//   useEffect(() => {
//     !IS_MOBILE &&
//       invoke<boolean>('tauri_is_sysproxy_enabled', { port: 7890 }).then(
//         (v) => {
//           setSysprox(v);
//         },
//         (err) => {
//           void message.error(`${err}`);
//         },
//       );
//     return () => {
//       void lis.current?.unregister();
//     };
//   }, []);
//   return (
//     <div className='flex items-center gap-2'>
//       <span>系统代理：</span>
//       <Switch
//         loading={toggling}
//         value={sysproxy}
//         onChange={(v) => {
//           if (IS_MOBILE) {
//             void toggleMobileVpn(v);
//           } else {
//             void toggleSysproxy(v);
//           }
//         }}
//       ></Switch>
//       {sysproxy ? '已开启' : '未开启'}
//     </div>
//   );
// };
