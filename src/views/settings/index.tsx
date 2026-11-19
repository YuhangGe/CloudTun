import { vm } from 'jinge';
import { Tabs, message } from 'jinge-antd';

import { globalSettings } from '@/store/settings';

import { InstanceConfigForm } from './Instance';
import { SecretTokenForm } from './Secret';

const TabOptions = [
  {
    label: '腾讯云密钥',
    key: 'secret',
    content: '',
  },
  {
    label: '主机配置',
    key: 'instance',
    content: '',
  },
];

export function CloudSettings() {
  const state = vm({
    tab: !globalSettings.secretKey || !globalSettings.secretId ? 'secret' : 'instance',
  });

  return (
    <>
      <Tabs
        className="max-sm:[&_.ja-tab-item]:text-xs! max-sm:[&_.ja-tab-list]:gap-2!"
        activeKey={state.tab}
        on:change={(t) => {
          if (t === 'instance' && !globalSettings.secretKey) {
            void message.error('请先填写密钥信息');
          } else {
            state.tab = t;
          }
        }}
        options={TabOptions}
      ></Tabs>

      {state.tab === 'secret' && <SecretTokenForm />}
      {state.tab === 'instance' && <InstanceConfigForm />}
    </>
  );
}
