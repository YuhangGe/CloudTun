import {
  Button,
  Controller,
  Input,
  InputAddon,
  InputNumber,
  InputWrapper,
  type MenuOption,
  Select,
  message,
  useForm,
} from 'jinge-antd';

import { DescribeImages, DescribeInstanceTypes, DescribeZones } from '@/service/tencent';
import { copyToClipboard, generateStrongPassword } from '@/service/util';
import { RegionOptions } from '@/service/region';
import { onMount, vm, watch } from 'jinge';
import { z } from 'zod';
import { globalSettings } from '@/store/settings';
import { FormItem } from './FormItem';

// const ImageTypeOpts = [
//   {
//     label: '私有镜像',
//     value: 'PRIVATE_IMAGE',
//   },
//   {
//     label: '公共镜像',
//     value: 'PUBLIC_IMAGE',
//   },
// ];

export function InstanceConfigForm() {
  const { formErrors, formState, control, validate } = useForm(
    z.object({
      region: z.string(),
      zone: z.string(),
      instanceType: z.string(),
      // imageType: z.string(),
      imageId: z.string(),
      loginPwd: z.string(),
      resourceName: z.string(),
      bandWidth: z.number().min(1).max(200),
    }),
    {
      defaultValues: globalSettings,
    },
  );

  const state = vm<{
    imageOpts: MenuOption<string>[];
    zoneOpts: MenuOption<string>[];
    instTypeOpts: MenuOption<string>[];
  }>({
    imageOpts: [],
    zoneOpts: [],
    instTypeOpts: [],
  });

  async function updateInstanceTypes(region?: string, zone?: string) {
    if (!region || !zone) return;
    const [err, res] = await DescribeInstanceTypes({
      region,
      Filters: [
        {
          Name: 'zone',
          Values: [zone],
        },
        {
          Name: 'instance-charge-type',
          Values: ['SPOTPAID'],
        },
      ],
    });
    if (err) return;
    const arr = res.InstanceTypeQuotaSet.filter(
      (t) => t.Cpu === 2 && t.Memory === 2 && t.Status === 'SELL',
    );
    state.instTypeOpts = arr.map((instType) => ({
      label: `${instType.InstanceType}(${instType.Cpu} CPU, ${instType.Memory} GB)`,
      value: instType.InstanceType,
    }));
    if (arr.length > 0) {
      if (!arr.some((v) => v.InstanceType === formState.instanceType)) {
        formState.instanceType = arr[0].InstanceType;
      }
    } else {
      formState.instanceType = undefined;
    }
  }

  async function onRegionChange() {
    if (!formState.region) return;
    const [err, res] = await DescribeZones({
      region: formState.region,
    });
    if (err) return;
    state.zoneOpts = res.ZoneSet.map((zone) => ({
      label: zone.ZoneName,
      value: zone.Zone,
    }));
    if (res.TotalCount == 0) return;
    formState.zone ??= res.ZoneSet[0].Zone;
    await updateInstanceTypes(formState.region, formState.zone);
  }

  watch(formState, 'region', (v) => {
    if (!v) return;
    state.zoneOpts = [];
    formState.zone = undefined;
    void onRegionChange();
  });

  onMount(() => {
    void onRegionChange();
  });

  watch(formState, 'zone', (v) => {
    void updateInstanceTypes(formState.region, v);
  });

  async function loadImages() {
    const instanceType = formState.instanceType;
    const region = formState.region;
    if (!(region && instanceType)) {
      return;
    }
    const Filters = [
      { Name: 'image-type', Values: ['PUBLIC_IMAGE'] },
      {
        Name: 'platform',
        Values: ['Ubuntu'],
      },
    ];
    const [err, res] = await DescribeImages({
      region,
      Filters,
      InstanceType: instanceType,
    });
    if (err) return;
    if (res.TotalCount > 0) {
      state.imageOpts = res.ImageSet.map((image) => ({
        label: image.ImageName,
        value: image.ImageId,
      }));
      if (!res.ImageSet.some((v) => v.ImageId === formState.imageId)) {
        formState.imageId = res.ImageSet[0].ImageId;
      }
    } else {
      formState.imageId = undefined;
      state.imageOpts = [];
    }
  }
  // watch(formState, 'imageType', loadImages);
  watch(
    formState,
    'instanceType',
    () => {
      void loadImages();
    },
    { immediate: true },
  );

  function resetPwd() {
    formState.loginPwd = generateStrongPassword();
  }
  async function save() {
    const [err, data] = await validate();
    if (err) return;
    Object.assign(globalSettings, data);
    message.success('保存成功！');
  }

  onMount(() => {
    if (!globalSettings.loginPwd) {
      resetPwd();
    }
  });

  return (
    <div className='mt-6 flex max-w-md flex-col gap-6 text-sm max-sm:max-w-full'>
      <FormItem label='区域：' required error={formErrors.region}>
        <Controller control={control} name='region'>
          {(field) => (
            <Select
              options={RegionOptions}
              value={field.value}
              on:change={(v) => {
                formState.zone = undefined;
                formState.imageId = undefined;
                field['on:change'](v);
              }}
            />
          )}
        </Controller>
      </FormItem>
      <FormItem label='可用区：' required>
        <Controller control={control} name='zone'>
          {(field) => (
            <Select options={state.zoneOpts} value={field.value} on:change={field['on:change']} />
          )}
        </Controller>
      </FormItem>
      <FormItem label='规格：' required>
        <Controller control={control} name='instanceType'>
          {(field) => (
            <Select
              className='max-h-[200px] overflow-y-auto'
              options={state.instTypeOpts}
              value={field.value}
              on:change={field['on:change']}
            />
          )}
        </Controller>
      </FormItem>
      <FormItem label='登录密码：' required>
        <Controller control={control} name='loginPwd'>
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
                    void resetPwd();
                  }}
                  className='!px-2'
                  slot:icon={<span className='icon-[ant-design--reload-outlined] text-base'></span>}
                  type='link'
                />
                <Button
                  on:click={() => {
                    const tk = formState.loginPwd;
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
      <FormItem label='资源名：' required>
        <Controller control={control} name='resourceName'>
          {(field) => (
            <Input value={field.value} on:change={field['on:change']} on:blur={field['on:blur']} />
          )}
        </Controller>
      </FormItem>
      {/* <FormItem label='镜像类型：' required>
        <Controller control={control} name='imageType'>
          {(field) => (
            <Select options={ImageTypeOpts} value={field.value} on:change={field['on:change']} />
          )}
        </Controller>
      </FormItem> */}
      <FormItem label='镜像：' required error={formErrors.imageId}>
        <Controller control={control} name='imageId'>
          {(field) => (
            <Select options={state.imageOpts} value={field.value} on:change={field['on:change']} />
          )}
        </Controller>
      </FormItem>
      <FormItem label='带宽' required error={formErrors.bandWidth}>
        <Controller control={control} name='bandWidth'>
          {(field) => (
            <InputWrapper>
              <InputNumber
                min={1}
                max={200}
                noRoundedR
                value={field.value}
                on:change={field['on:change']}
                on:blur={field['on:blur']}
              />
              <InputAddon className='px-2'>Mbps</InputAddon>
            </InputWrapper>
          )}
        </Controller>
      </FormItem>

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
    </div>
  );
}
