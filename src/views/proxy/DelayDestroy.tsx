// import { App, Button, InputNumber, Modal } from 'antd';
// import { type FC, useState } from 'react';
// import { invoke } from '@tauri-apps/api/core';
// import { globalStore } from '@/store/global';
// import { isNumber } from '@/service/util';

// export const DelayDestroy: FC = () => {
//   const [inst] = globalStore.useStore('instance');

//   const { message } = App.useApp();
//   const [v, setV] = useState(30);
//   const [open, setOpen] = useState(false);
//   const [submitting, setSubmitting] = useState(false);
//   const submit = async () => {
//     const token = globalStore.get('settings').token;
//     const ip = inst?.PublicIpAddresses?.[0];
//     if (!ip || !token) {
//       setOpen(false);
//       return;
//     }
//     setSubmitting(true);
//     try {
//       const res = await invoke('tauri_ping_v2ray_delay', {
//         url: `http://${ip}:2081/delay?token=${token}&minutes=${v}`,
//       });
//       if (res === 'ok!') {
//         void message.success('推迟自动销毁成功！');
//         setOpen(false);
//       } else {
//         throw 'failed';
//       }
//     } catch (ex) {
//       console.error(ex);
//       void message.error('推迟自动销毁失败！');
//     } finally {
//       setSubmitting(false);
//     }
//   };
//   return (
//     <>
//       <Button
//         onClick={() => {
//           setOpen(true);
//         }}
//         loading={submitting}
//         className='text-xs'
//         size='small'
//       >
//         推迟自动销毁
//       </Button>
//       <Modal
//         open={open}
//         onCancel={() => {
//           if (submitting) return;
//           setOpen(false);
//         }}
//         okButtonProps={{
//           loading: submitting,
//         }}
//         title='推迟自动销毁'
//         onOk={() => {
//           void submit();
//         }}
//       >
//         <p className='pb-2 pt-2 text-xs text-secondary-text'>
//           远程主机在连续 10 分钟没有 CloudV2Ray 客户端 Ping
//           的情况下会自动销毁。可设置推迟销毁的时间，在该时间段内不自动销毁。
//         </p>
//         <div className='flex items-center'>
//           <label>推迟时间：</label>
//           <InputNumber
//             value={v}
//             onChange={(v) => {
//               if (isNumber(v)) {
//                 setV(v);
//               }
//             }}
//             min={1}
//             max={240}
//             suffix='分钟'
//           />
//         </div>
//       </Modal>
//     </>
//   );
// };
