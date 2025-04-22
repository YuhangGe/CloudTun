import { generateStrongPassword } from "@/service/util";
import { globalStore } from "@/store/global";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "jinge";
import { Button, Controller, Input, message, useForm } from "jinge-antd";
import { z } from "zod";
import { FormItem } from "./FormItem";

export function SecretTokenForm() {

  const { formState, formErrors, validate, control } = useForm(z.object({
    token: z.string(),
    secretId: z.string().min(1),
    secretKey: z.string().min(1),
    loginPwd: z.string().min(1)
  }), { defaultValues: globalStore.settings });

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
    formState.loginPwd = generateStrongPassword()
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

  return <>
    <div className="flex flex-col gap-6 mt-6 max-w-md max-sm:max-w-full text-sm">
      <FormItem label='Secret Id：' required error={formErrors.secretId} >
        <Controller control={control} name='secretId'>
          {(field) => (
            <Input
              value={formState.secretId}
              on:change={field['on:change']}
              on:blur={field['on:blur']}
            />
          )}
        </Controller>
      </FormItem>
      <FormItem label='Secret Key：' required error={formErrors.secretKey} >
        <Controller control={control} name='secretKey'>
          {(field) => (
            <Input
              value={formState.secretKey}
              on:change={field['on:change']}
              on:blur={field['on:blur']}
            />
          )}
        </Controller>
      </FormItem>

      <FormItem label='VMess Id：' required>
        <Controller control={control} name='token'>
          {(field) => (

            <Input
              className='cursor-pointer'
              value={formState.secretKey}
              on:change={field['on:change']}
              on:blur={field['on:blur']}
              on:focus={(evt) => {
                setTimeout(() => evt.target.select());
              }}
            // addonAfter={
            //   <Button.Group size='small'>
            //     <Tooltip title='生成 UUID'>
            //       <Button
            //         onClick={() => {
            //           void resetToken();
            //         }}
            //         icon={<span className='icon-[ant-design--reload-outlined]'></span>}
            //         type='link'
            //       ></Button>
            //     </Tooltip>
            //     <Button
            //       onClick={() => {
            //         const tk = form.getFieldValue('token');
            //         if (tk) {
            //           void copyToClipboard(tk).then(() => {
            //             void message.success('已复制！');
            //           });
            //         }
            //       }}
            //       type='link'
            //       icon={<span className='icon-[ant-design--copy-outlined]'></span>}
            //     />
            //   </Button.Group>
            // }
            />

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
}