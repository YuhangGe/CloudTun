import { invoke } from '@tauri-apps/api/core';
import { cx, onMount } from 'jinge';
import { message } from 'jinge-antd';

import { ContextMenu } from './ContextMenu';
import { Layout } from './Layout';
import { IS_ANDROID } from './service/util';

if (IS_ANDROID) {
  message.configContainer({
    paddingTop: 32,
    paddingLeft: 32,
    paddingRight: 32,
  });
}

function App() {
  onMount(() => {
    if (IS_ANDROID) {
      void invoke('tauri_android_request_notification_permission');
    }
  });
  return (
    <div className={cx('flex size-full overflow-hidden bg-background', IS_ANDROID && 'pt-8')}>
      <Layout />
      <ContextMenu />
    </div>
  );
}

export default App;
