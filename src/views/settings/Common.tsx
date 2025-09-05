import { Button, Controller, message, useForm } from 'jinge-antd';
import { z } from 'zod';
import { FormItem } from './FormItem';
import { globalSettings } from '@/store/settings';
import { Switch } from './Swtich';
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';

export function CommonSettingsForm() {
  const { formErrors, validate, control } = useForm(
    z.object({
      autoProxy: z.boolean(),
      autoStartApp: z.boolean(),
    }),
    { defaultValues: globalSettings },
  );

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
      </div>
      <div className='my-20 flex items-center gap-8'>
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
