import { vm, watch } from 'jinge';
import { Button, Modal } from 'jinge-antd';

import { validateCloudCredentials } from '@/service/settings';
import { globalSettings } from '@/store/settings';
import { CloudSettings } from '@/views/settings';

import { ProxyView } from './index';

export function CloudPage() {
  const state = vm({
    settingsOpen: false,
    cloudError: validateCloudCredentials(globalSettings) ?? '',
  });
  watch(globalSettings, () => {
    state.cloudError = validateCloudCredentials(globalSettings) ?? '';
  });
  return (
    <div className="mx-auto max-w-[1100px] [&_h1]:my-1.5 [&_h1]:text-2xl [&_h1]:font-semibold [&_h1]:tracking-tight">
      <div className="flex items-center justify-between gap-4 pb-2">
        <h1>云上主机</h1>
        <button
          type="button"
          aria-label="云主机设置"
          title="云主机设置"
          className="grid size-9 shrink-0 cursor-pointer place-items-center rounded-lg border border-border bg-transparent text-lg text-secondary-text hover:text-primary"
          on:click={() => {
            state.settingsOpen = true;
          }}
        >
          <span className="icon-[ant-design--setting-outlined]" />
        </button>
      </div>
      {state.cloudError ? (
        <section className="rounded-xl border border-dashed border-border p-5 sm:p-7.5 [&_h2]:font-semibold [&_p]:mt-3 [&_p]:mb-5 [&_p]:text-xs [&_p]:text-secondary-text">
          <h2>首次使用，请先设置云主机</h2>
          <p>{state.cloudError}。保存腾讯云密钥后即可查看云上主机。</p>
          <Button
            type="primary"
            on:click={() => {
              state.settingsOpen = true;
            }}
          >
            立即设置
          </Button>
        </section>
      ) : (
        <ProxyView />
      )}
      <Modal
        open={state.settingsOpen}
        title="云主机设置"
        className="w-[min(720px,calc(100vw-24px))]"
        footer={false}
        on:openChange={(open) => {
          state.settingsOpen = open;
        }}
      >
        <div className="min-w-0 [&_label]:shrink-0">
          <p className="my-3 text-xs leading-relaxed text-secondary-text">
            各项设置保存后生效。首次使用请先保存腾讯云密钥，再选择主机配置。
          </p>
          <CloudSettings />
        </div>
      </Modal>
    </div>
  );
}
