import { Layout } from './Layout';
import { ContextMenu } from './ContextMenu';
import { cx, onMount } from 'jinge';
import { IS_ANDROID } from './service/util';
import { invoke } from '@tauri-apps/api/core';
import { message } from 'jinge-antd';

if (IS_ANDROID) {
  message.configContainer({ paddingTop: 32, paddingLeft: 32, paddingRight: 32 });
}

function App() {
  onMount(() => {
    if (IS_ANDROID) {
      void invoke('tauri_android_request_notification_permission');
    }
  });
  return (
    <div className={cx('bg-background flex size-full overflow-hidden', IS_ANDROID && 'pt-8')}>
      <Layout />
      <ContextMenu />
    </div>
  );
}

export default App;
