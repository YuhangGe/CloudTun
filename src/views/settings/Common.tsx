import { Button, Controller, Select, message, modal, onModalConfirm, useForm } from 'jinge-antd';
import { z } from 'zod';
import { FormItem } from './FormItem';
import { globalSettings } from '@/store/settings';
import { Switch } from './Swtich';
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
import { IS_ANDROID, IS_IOS, IS_MOBILE } from '@/service/util';
import { onMount, vm, vmRaw } from 'jinge';
import { invoke } from '@tauri-apps/api/core';

interface App {
  name: string;
  icon: string;
  id: string;
  selected?: boolean;
}

function PickAppModal(props: { apps?: string[] }) {
  const state = vm<{
    allApps: App[];
  }>({
    allApps: [],
  });

  onMount(() => {
    invoke<App[]>('tauri_android_list_all_apps').then(
      (ret) => {
        const idSet = new Set(props.apps);
        ret.forEach((app) => {
          app.selected = idSet.has(app.id);
        });
        state.allApps = ret;
      },
      (err) => {
        console.error(err);
      },
    );
  });

  onModalConfirm(() => {
    return {
      result: vmRaw(state.allApps)
        .filter((app) => app.selected)
        .map((app) => app.id),
    };
  });

  return (
    <div className='border-border mt-2 flex max-h-[60vh] flex-col overflow-auto rounded-sm border'>
      {state.allApps.map((app) => (
        <div className='border-border flex items-center border-b py-3 pl-3' key={app.id}>
          <div className='mr-3 h-11 w-11 shrink-0'>
            {app.icon ? (
              <img src={app.icon} className='w-full rounded-full' />
            ) : (
              <div className='bg-blue/25 size-full rounded-full'></div>
            )}
          </div>
          <div className='flex flex-1 flex-col justify-between'>
            <p className='font-bold'>{app.name}</p>
            <p className='text-secondary-text text-sm'>{app.id}</p>
          </div>
          <div className='ml-3 w-10 shrink-0'>
            <input
              type='checkbox'
              on:change={(evt) => {
                app.selected = evt.target.checked;
              }}
              checked={app.selected}
            />
          </div>
        </div>
      ))}
    </div>
  );
}

export function CommonSettingsForm() {
  const { formState, formErrors, validate, control } = useForm(
    z.object({
      autoProxy: z.boolean(),
      autoStartApp: z.boolean(),
      mobileProxyMode: z.string(),
    }),
    { defaultValues: globalSettings },
  );

  const state = vm<{
    apps: string[];
  }>({
    apps: globalSettings.mobileProxyApps.split('\n'),
  });

  async function save() {
    const [err, data] = await validate();
    if (err) return;
    const oldAutoProxy = globalSettings.autoProxy;
    if (oldAutoProxy !== data.autoProxy) {
      globalSettings.autoProxy = data.autoProxy;
      if (data.autoProxy) {
        message.success('已配置APP打开后自动启动代理！');
      } else {
        message.success('已取消APP打开后自动启动代理');
      }
    }

    const oldAutoStart = globalSettings.autoStartApp;
    if (oldAutoStart !== data.autoStartApp) {
      globalSettings.autoStartApp = data.autoStartApp;
      if (data.autoStartApp) {
        if (!(await isEnabled())) {
          await enable();
        }
        message.success('已配置开机启动！');
      } else {
        if (await isEnabled()) {
          await disable();
        }
        message.success('已取消开机启动！');
      }
    }

    const mobileProxyApps = state.apps.join('\n');
    if (
      data.mobileProxyMode !== globalSettings.mobileProxyMode ||
      mobileProxyApps !== globalSettings.mobileProxyApps
    ) {
      globalSettings.mobileProxyMode = data.mobileProxyMode as 'global' | 'app';
      globalSettings.mobileProxyApps = mobileProxyApps;
      message.success('保存成功！');
    }
  }

  async function pickApps() {
    const ret = await modal
      .show<string[]>({
        title: '选择应用',
        'slot:content': <PickAppModal apps={state.apps} />,
      })
      .waitForClose();
    if (ret !== undefined) {
      const idSet = new Set(state.apps);
      ret.forEach((app) => {
        if (!idSet.has(app)) {
          state.apps.push(app);
        }
      });
    }
  }

  return (
    <>
      <div className='mt-6 flex max-w-md flex-col gap-6 text-sm max-sm:max-w-full'>
        <FormItem label='自动代理：' error={formErrors.autoProxy}>
          <Controller control={control} name='autoProxy'>
            {(field) => (
              <div className='flex items-center'>
                <Switch
                  value={field.value}
                  on:change={(checked) => {
                    field['on:change'](checked);
                  }}
                />
              </div>
            )}
          </Controller>
        </FormItem>
        {!IS_MOBILE && (
          <FormItem label='开机启动：' error={formErrors.autoStartApp}>
            <Controller control={control} name='autoStartApp'>
              {(field) => (
                <div className='flex items-center'>
                  <Switch
                    value={field.value}
                    on:change={(checked) => {
                      field['on:change'](checked);
                    }}
                  />
                </div>
              )}
            </Controller>
          </FormItem>
        )}
        {IS_MOBILE && (
          <FormItem label='代理模式：' error={formErrors.mobileProxyMode}>
            <Controller control={control} name='mobileProxyMode'>
              {(field) => (
                <div className='flex w-40 items-center'>
                  <Select
                    on:change={field['on:change']}
                    value={field.value}
                    options={[
                      { label: '全局代理', value: 'global' },
                      { label: '指定应用', value: 'app' },
                    ]}
                  />
                </div>
              )}
            </Controller>
          </FormItem>
        )}
        {IS_MOBILE && formState.mobileProxyMode === 'app' && (
          <FormItem label='代理应用：'>
            <div className=''>
              <ul className='border-border max-h-[200px] overflow-auto rounded-md border'>
                {state.apps.map((app, idx) => (
                  <li
                    className='border-border flex items-center px-3 py-3 not-last:border-b'
                    key={app}
                  >
                    <span className='flex-1'>{app}</span>
                    <Button
                      on:click={() => {
                        state.apps.splice(idx, 1);
                      }}
                      type='link'
                      size='sm'
                      slot:icon={<span className='icon-[ant-design--delete-twotone]'></span>}
                    ></Button>
                  </li>
                ))}
              </ul>
              <div className='mt-3 flex'>
                <Button
                  on:click={() => {
                    if (IS_ANDROID) {
                      void pickApps();
                    } else if (IS_IOS) {
                      // todo
                    }
                  }}
                  size='sm'
                  slot:icon={<span className='icon-[ant-design--plus-outlined]'></span>}
                >
                  添加
                </Button>
              </div>
            </div>
          </FormItem>
        )}
      </div>
      <div className='my-10 flex items-center gap-8'>
        <Button
          type='primary'
          on:click={() => {
            void save();
          }}
        >
          保存
        </Button>
      </div>
    </>
  );
}
