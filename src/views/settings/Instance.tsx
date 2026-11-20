import { onMount, onUnmount, vm, watch } from 'jinge';
import {
  Button,
  Controller,
  Input,
  InputAddon,
  InputNumber,
  InputWrapper,
  Select,
  message,
  useForm,
} from 'jinge-antd';
import { z } from 'zod';

import { RegionOptions } from '@/service/region';
import {
  type CVMPrice,
  DescribeImages,
  DescribeInstanceTypes,
  DescribeZones,
  InquiryPriceRunInstances,
  canQuoteInstance,
  describeSystemDiskTypes,
} from '@/service/tencent';
import { copyToClipboard, generateStrongPassword } from '@/service/util';
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

const diskLabels: Record<string, string> = {
  CLOUD_BASIC: '普通云硬盘',
  CLOUD_PREMIUM: '高性能云硬盘',
  CLOUD_BSSD: '通用型 SSD',
  CLOUD_SSD: 'SSD 云硬盘',
  CLOUD_HSSD: '增强型 SSD',
  CLOUD_TSSD: '极速型 SSD',
};
type SelectOption = { label: string; value: string };

export function InstanceConfigForm() {
  const { formErrors, formState, control, validate } = useForm(
    z.object({
      region: z.string(),
      zone: z.string(),
      instanceType: z.string(),
      // imageType: z.string(),
      diskType: z.string().min(1, '请选择磁盘类型'),
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
    imageOpts: SelectOption[];
    zoneOpts: SelectOption[];
    instTypeOpts: SelectOption[];
    diskOpts: SelectOption[];
    diskLoading: boolean;
    diskError: string;
    price?: CVMPrice;
    priceLoading: boolean;
    priceError: string;
  }>({
    imageOpts: [],
    zoneOpts: [],
    instTypeOpts: [],
    diskOpts: [],
    diskLoading: false,
    diskError: '',
    priceLoading: false,
    priceError: '',
  });
  let active = true;
  let diskRequest = 0;
  let priceRequest = 0;
  let priceTimer = 0;
  let lastDiskKey: string | undefined;

  async function loadDiskTypes() {
    const { region, zone, instanceType } = formState;
    const key = JSON.stringify([region, zone, instanceType]);
    if (key === lastDiskKey) return;
    const isInitial = lastDiskKey === undefined;
    lastDiskKey = key;
    const request = ++diskRequest;
    const savedDisk = isInitial ? formState.diskType : undefined;
    state.diskOpts = [];
    state.diskError = '';
    state.diskLoading = !!(region && zone && instanceType);
    formState.diskType = undefined;
    if (!region || !zone || !instanceType) return;

    const [error, diskTypes] = await describeSystemDiskTypes({ region, zone, instanceType });
    if (!active || request !== diskRequest) return;
    state.diskLoading = false;
    if (error) {
      state.diskError = '查询磁盘类型失败';
      return;
    }
    state.diskOpts = diskTypes.map((value) => ({ value, label: diskLabels[value] ?? value }));
    if (!diskTypes.length) state.diskError = '当前规格没有可用的 20 GiB 系统盘';
    formState.diskType = savedDisk && diskTypes.includes(savedDisk) ? savedDisk : diskTypes[0];
  }

  function schedulePrice() {
    clearTimeout(priceTimer);
    const request = ++priceRequest;
    const config = { ...formState };
    state.price = undefined;
    state.priceError = '';
    state.priceLoading = !state.diskLoading && canQuoteInstance(config);
    if (state.diskLoading || !canQuoteInstance(config)) return;
    priceTimer = window.setTimeout(async () => {
      const [error, result] = await InquiryPriceRunInstances(config);
      if (!active || request !== priceRequest) return;
      state.priceLoading = false;
      if (error) state.priceError = `询价失败：${error.message}`;
      else state.price = result.Price;
    }, 250);
  }

  for (const field of ['region', 'zone', 'instanceType'] as const) {
    watch(formState, field, () => void loadDiskTypes());
  }
  for (const field of [
    'region',
    'zone',
    'instanceType',
    'diskType',
    'imageId',
    'bandWidth',
  ] as const) {
    watch(formState, field, schedulePrice);
  }
  onMount(() => {
    void loadDiskTypes();
  });
  onUnmount(() => {
    active = false;
    clearTimeout(priceTimer);
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
    if (err || !active || region !== formState.region || zone !== formState.zone) return;
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
    const region = formState.region;
    if (!region) return;
    const [err, res] = await DescribeZones({
      region,
    });
    if (err || !active || region !== formState.region) return;
    state.zoneOpts = res.ZoneSet.map((zone) => ({
      label: zone.ZoneName,
      value: zone.Zone,
    }));
    if (!res.ZoneSet.length) return;
    if (!res.ZoneSet.some(({ Zone }) => Zone === formState.zone)) {
      formState.zone = res.ZoneSet[0].Zone;
      return;
    }
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
    if (err || !active || region !== formState.region || instanceType !== formState.instanceType)
      return;
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
    <div className="mt-3 flex max-w-md flex-col gap-6 text-sm max-sm:max-w-full">
      <FormItem label="区域：" required error={formErrors.region}>
        <Controller control={control} name="region">
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
      <FormItem label="可用区：" required>
        <Controller control={control} name="zone">
          {(field) => (
            <Select options={state.zoneOpts} value={field.value} on:change={field['on:change']} />
          )}
        </Controller>
      </FormItem>
      <FormItem label="规格：" required>
        <Controller control={control} name="instanceType">
          {(field) => (
            <Select
              className="max-h-50 overflow-y-auto"
              options={state.instTypeOpts}
              value={field.value}
              on:change={field['on:change']}
            />
          )}
        </Controller>
      </FormItem>
      <FormItem label="登录密码：" required>
        <Controller control={control} name="loginPwd">
          {(field) => (
            <InputWrapper>
              <Input
                className="cursor-pointer"
                noRoundedR
                value={field.value}
                on:change={field['on:change']}
                on:blur={field['on:blur']}
                on:focus={(evt) => {
                  const input = evt.currentTarget;
                  if (input instanceof HTMLInputElement) setTimeout(() => input.select());
                }}
              />
              <InputAddon>
                <Button
                  on:click={() => {
                    resetPwd();
                  }}
                  className="px-2!"
                  slot:icon={<span className="icon-[ant-design--reload-outlined] text-base"></span>}
                  type="link"
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
                  className="rounded-none! border-l-border! px-2!"
                  type="link"
                  slot:icon={<span className="icon-[ant-design--copy-outlined] text-base"></span>}
                />
              </InputAddon>
            </InputWrapper>
          )}
        </Controller>
      </FormItem>
      <FormItem label="资源名：" required>
        <Controller control={control} name="resourceName">
          {(field) => (
            <Input value={field.value} on:change={field['on:change']} on:blur={field['on:blur']} />
          )}
        </Controller>
      </FormItem>
      <FormItem label="磁盘类型：" required error={state.diskError || formErrors.diskType}>
        <Controller control={control} name="diskType">
          {(field) => (
            <Select
              options={state.diskOpts}
              value={field.value}
              disabled={state.diskLoading || !state.diskOpts.length}
              on:change={field['on:change']}
            />
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
      <FormItem label="镜像：" required error={formErrors.imageId}>
        <Controller control={control} name="imageId">
          {(field) => (
            <Select options={state.imageOpts} value={field.value} on:change={field['on:change']} />
          )}
        </Controller>
      </FormItem>
      <FormItem label="带宽" required error={formErrors.bandWidth}>
        <Controller control={control} name="bandWidth">
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
              <InputAddon className="px-2">Mbps</InputAddon>
            </InputWrapper>
          )}
        </Controller>
      </FormItem>

      <div className="my-4 flex items-center gap-3">
        <Button
          className="shrink-0"
          type="primary"
          disabled={state.diskLoading || !state.diskOpts.length}
          on:click={() => {
            void save();
          }}
        >
          保存
        </Button>
        <div className="flex min-w-0 flex-wrap gap-x-2 text-xs text-secondary-text" role="status">
          {state.price?.InstancePrice && (
            <span>主机 ¥{state.price.InstancePrice.UnitPriceDiscount}/小时</span>
          )}
          {state.price?.BandwidthPrice && (
            <span>流量 ¥{state.price.BandwidthPrice.UnitPriceDiscount}/GB</span>
          )}
          {state.priceLoading || state.diskLoading ? '询价中…' : state.priceError}
        </div>
      </div>
    </div>
  );
}
