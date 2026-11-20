import { cx } from 'jinge';
import { Select } from 'jinge-antd';

import { IS_MOBILE, IS_IOS, IS_ANDROID } from '@/service/util';
import { desktop, changeMode, changeSystemProxy, type RoutingMode } from '@/store/desktop';
import { globalSettings } from '@/store/settings';

export function ProxyOptions() {
  return (
    <div
      className={cx(
        'ml-auto flex min-w-0 items-center gap-2 text-xs font-normal',
        !IS_ANDROID && 'max-[479px]:w-full',
      )}
    >
      {!IS_MOBILE && (
        <div
          role="group"
          aria-label="系统代理"
          title="系统代理"
          className="w-28 min-w-0 max-[479px]:w-0 max-[479px]:flex-1"
        >
          <Select
            size="sm"
            className="w-full"
            value={globalSettings.subscriptionSystemProxy ? 'on' : 'off'}
            disabled={IS_IOS || desktop.connecting || desktop.updating}
            options={[
              { value: 'off', label: '清除系统代理' },
              { value: 'on', label: '设置系统代理' },
            ]}
            on:change={(value) => {
              void changeSystemProxy(value === 'on');
            }}
          />
        </div>
      )}
      <div
        role="group"
        aria-label="分流模式"
        title="分流模式"
        className={cx('w-36 min-w-0', !IS_ANDROID && 'max-[479px]:w-0 max-[479px]:flex-1')}
      >
        <Select
          size="sm"
          className="w-full"
          value={globalSettings.routingMode}
          disabled={IS_IOS || desktop.connecting || desktop.updating}
          options={[
            { value: 'bypassChina', label: '绕过大陆与局域网' },
            { value: 'global', label: '全局代理' },
          ]}
          on:change={(value) => {
            void changeMode(value as RoutingMode);
          }}
        />
      </div>
    </div>
  );
}
