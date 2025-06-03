import { Tabs, message } from 'jinge-antd';

import { globalStore } from '@/store/global';
import { vm } from 'jinge';

import { SecretTokenForm } from './Secret';
import { InstanceConfigForm } from './Instance';

const TabOptions = [
  {
    label: '密钥参数',
    key: 'secret',
  },
  {
    label: '主机参数',
    key: 'instance',
  },
];

export function SettingsView() {
  const state = vm({
    tab: globalStore.settings.secretKey ? 'instance' : 'secret',
  });

  return (
    <>
      <Tabs
        activeKey={state.tab}
        on:change={(t) => {
          if (t !== 'secret' && !globalStore.settings.secretKey) {
            void message.error('请先填写密钥信息');
          } else {
            state.tab = t;
          }
        }}
        options={TabOptions}
      ></Tabs>

      {state.tab === 'secret' ? <SecretTokenForm /> : <InstanceConfigForm />}
    </>
  );
}
