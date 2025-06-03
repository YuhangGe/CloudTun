import { copyToClipboard, generateStrongPassword } from '@/service/util';
import { globalStore } from '@/store/global';
import { invoke } from '@tauri-apps/api/core';
import { onMount } from 'jinge';
import { Button, Controller, Input, InputAddon, InputWrapper, message, useForm } from 'jinge-antd';
import { z } from 'zod';
import { FormItem } from './FormItem';

export function SecretTokenForm() {
  const { formState, formErrors, validate, control } = useForm(
    z.object({
      token: z.string(),
      secretId: z.string().min(1),
      secretKey: z.string().min(1),
      loginPwd: z.string().min(1),
    }),
    { defaultValues: globalStore.settings },
  );

  function resetToken() {
    invoke('tauri_generate_uuid').then(
      (id) => {
        formState.token = id as string;
      },
      (err) => {
        message.error(`${err}`);
      },
    );
  }
  function resetPwd() {
    formState.loginPwd = generateStrongPassword();
  }
  async function save() {
    const [err, data] = await validate();
    if (err) return;
    Object.assign(globalStore.settings, data);
  }

  onMount(() => {
    if (!globalStore.settings.token) {
      void resetToken();
    }
    if (!globalStore.settings.loginPwd) {
      resetPwd();
    }
  });

  return (
    <>
      <div className='mt-6 flex max-w-md flex-col gap-6 text-sm max-sm:max-w-full'>
        <FormItem label='Secret Id：' required error={formErrors.secretId}>
          <Controller control={control} name='secretId'>
            {(field) => (
              <Input
                value={field.value}
                on:change={field['on:change']}
                on:blur={field['on:blur']}
              />
            )}
          </Controller>
        </FormItem>
        <FormItem label='Secret Key：' required error={formErrors.secretKey}>
          <Controller control={control} name='secretKey'>
            {(field) => (
              <Input
                value={field.value}
                on:change={field['on:change']}
                on:blur={field['on:blur']}
              />
            )}
          </Controller>
        </FormItem>

        <FormItem label='VMess Id：' required>
          <Controller control={control} name='token'>
            {(field) => (
              <InputWrapper>
                <Input
                  className='cursor-pointer'
                  noRoundedR
                  value={field.value}
                  on:change={field['on:change']}
                  on:blur={field['on:blur']}
                  on:focus={(evt) => {
                    setTimeout(() => evt.target.select());
                  }}
                />
                <InputAddon>
                  <Button
                    on:click={() => {
                      void resetToken();
                    }}
                    className='!px-2'
                    slot:icon={
                      <span className='icon-[ant-design--reload-outlined] text-base'></span>
                    }
                    type='link'
                  />
                  <Button
                    on:click={() => {
                      const tk = formState.token;
                      if (tk) {
                        void copyToClipboard(tk).then(() => {
                          void message.success('已复制！');
                        });
                      }
                    }}
                    className='!border-l-border !rounded-none !px-2'
                    type='link'
                    slot:icon={<span className='icon-[ant-design--copy-outlined] text-base'></span>}
                  />
                </InputAddon>
              </InputWrapper>
            )}
          </Controller>
        </FormItem>
      </div>
      <div className='my-4 flex items-center gap-8'>
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
