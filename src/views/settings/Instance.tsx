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

import { DescribeImages, DescribeInstanceTypeConfigs, DescribeZones } from '@/service/tencent';
import { copyToClipboard, generateStrongPassword } from '@/service/util';
import { RegionOptions } from '@/service/region';
import { onMount, vm, watch } from 'jinge';
import { z } from 'zod';
import { globalSettings } from '@/store/settings';
import { FormItem } from './FormItem';

const ImageTypeOpts = [
  {
    label: '私有镜像',
    value: 'PRIVATE_IMAGE',
  },
  {
    label: '公共镜像',
    value: 'PUBLIC_IMAGE',
  },
];

export function InstanceConfigForm() {
  const { formErrors, formState, control, validate } = useForm(
    z.object({
      region: z.string(),
      zone: z.string(),
      instanceType: z.string(),
      imageType: z.string(),
      imageId: z.string(),
      loginPwd: z.string(),
      resourceName: z.string(),
      bandWidth: z.number().min(1).max(100),
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
    const [err, res] = await DescribeInstanceTypeConfigs({
      region,
      Filters: [
        {
          Name: 'zone',
          Values: [zone],
        },
      ],
    });
    if (!err) {
      const arr = res.InstanceTypeConfigSet.sort((a, b) => {
        if (a.CPU === b.CPU) return a.Memory - b.Memory;
        return a.CPU - b.CPU;
      });
      state.instTypeOpts = arr.map((instType) => ({
        label: `${instType.InstanceType}(${instType.CPU} CPU, ${instType.Memory} GB)`,
        value: instType.InstanceType,
      }));
    }
  }
  watch(
    formState,
    'region',
    (v) => {
      if (!v) return;
      state.zoneOpts = [];
      void DescribeZones({
        region: v,
      }).then(([err, res]) => {
        if (!err) {
          state.zoneOpts = res.ZoneSet.map((zone) => ({
            label: zone.ZoneName,
            value: zone.Zone,
          }));
        }
      });
      void updateInstanceTypes(v, formState.zone);
      void loadImages(v, formState.imageType);
    },
    { immediate: true },
  );

  watch(formState, 'zone', (v) => {
    void updateInstanceTypes(formState.region, v);
  });

  // const [imageOptions, setImageOptions] = useState<DefaultOptionType[]>([]);
  async function loadImages(region?: string, imageType?: string) {
    if (!region || !imageType) return;
    const Filters = [{ Name: 'image-type', Values: [imageType] }];
    if (imageType === 'PUBLIC_IMAGE') {
      Filters.push({
        Name: 'platform',
        Values: ['Ubuntu'],
      });
    }
    const [err, res] = await DescribeImages({
      region,
      Filters,
    });
    if (err) return;
    if (res.TotalCount > 0) {
      state.imageOpts = res.ImageSet.map((image) => ({
        label: image.ImageName,
        value: image.ImageId,
      }));
      if (formState.imageType === 'PRIVATE_IMAGE' && globalSettings.token) {
        // 私有镜像约定使用 vmess uuid 作为镜像名。如果找到了，则填充 image id。
        const img = res.ImageSet.find((ii) => ii.ImageName == globalSettings.token);
        if (img && globalSettings.imageId !== img.ImageId) {
          formState.imageId = img.ImageId;
        }
      }
    } else {
      state.imageOpts = [];
    }
  }

  function resetPwd() {
    formState.loginPwd = generateStrongPassword();
  }
  async function save() {
    const [err, data] = await validate();
    if (err) return;
    Object.assign(globalSettings, data);
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
            <Select options={RegionOptions} value={field.value} on:change={field['on:change']} />
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
      <FormItem label='镜像类型：' required>
        <Controller control={control} name='imageType'>
          {(field) => (
            <Select options={ImageTypeOpts} value={field.value} on:change={field['on:change']} />
          )}
        </Controller>
      </FormItem>
      <FormItem label='镜像：' required>
        <Controller control={control} name='imageId'>
          {(field) => (
            <Select options={state.imageOpts} value={field.value} on:change={field['on:change']} />
          )}
        </Controller>
      </FormItem>
      <FormItem label='带宽' required>
        <Controller control={control} name='bandWidth'>
          {(field) => (
            <InputWrapper>
              <InputNumber
                min={1}
                max={100}
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
