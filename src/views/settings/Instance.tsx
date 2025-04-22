import type { FormInstance } from 'antd';
import type { FC } from 'react';
import { useEffect, useState } from 'react';
import { App, Button, Form, Input, InputNumber, Select, Tooltip } from 'antd';
import type { DefaultOptionType } from 'antd/es/select';
import type { Settings } from '@/service/settings';
import { globalStore } from '@/store/global';
import { DescribeImages, DescribeInstanceTypeConfigs, DescribeZones } from '@/service/tencent';
import { copyToClipboard, generateStrongPassword } from '@/service/util';
import { RegionOptions } from '@/service/region';

export const InstancePanel: FC<{
  form: FormInstance<Settings>;
}> = ({ form }) => {
  const [settings] = globalStore.useStore('settings');
  const { message } = App.useApp();
  const [region, setRegion] = useState(settings.region);
  useEffect(() => {
    setRegion(settings.region);
  }, [settings.region]);
  const [zoneOptions, setZoneOptions] = useState<DefaultOptionType[]>([]);
  const [instTypeOptions, setInstTypeOptions] = useState<DefaultOptionType[]>([]);
  useEffect(() => {
    setZoneOptions([]);
    void DescribeZones({
      region,
    }).then(([err, res]) => {
      if (!err) {
        setZoneOptions(
          res.ZoneSet.map((zone) => ({
            label: zone.ZoneName,
            value: zone.Zone,
          })),
        );
      }
    });
  }, [region]);

  const zone = Form.useWatch(['zone'], form);
  useEffect(() => {
    if (!region || !zone) {
      setInstTypeOptions([]);
      return;
    }
    void DescribeInstanceTypeConfigs({
      region,
      Filters: [
        {
          Name: 'zone',
          Values: [zone],
        },
      ],
    }).then(([err, res]) => {
      if (!err) {
        const arr = res.InstanceTypeConfigSet.sort((a, b) => {
          if (a.CPU === b.CPU) return a.Memory - b.Memory;
          return a.CPU - b.CPU;
        });
        setInstTypeOptions(
          arr.map((instType) => ({
            label: `${instType.InstanceType}(${instType.CPU} CPU, ${instType.Memory} GB)`,
            value: instType.InstanceType,
          })),
        );
      }
    });
  }, [region, zone]);

  const [imageOptions, setImageOptions] = useState<DefaultOptionType[]>([]);
  const loadImages = async (imageType: Settings['imageType']) => {
    const Filters = [{ Name: 'image-type', Values: [imageType as string] }];
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
      setImageOptions(
        res.ImageSet.map((image) => ({
          label: image.ImageName,
          value: image.ImageId,
        })),
      );
      if (settings.imageType === 'PRIVATE_IMAGE' && settings.token) {
        // 私有镜像约定使用 vmess uuid 作为镜像名。如果找到了，则填充 image id。
        const img = res.ImageSet.find((ii) => ii.ImageName == settings.token);
        if (img && settings.imageId !== img.ImageId) {
          form.setFieldValue('imageId', img.ImageId);
        }
      }
    } else {
      setImageOptions([]);
    }
  };
  useEffect(() => {
    if (!region) return;
    void loadImages(settings.imageType);
  }, [region]);
  const resetPwd = () => {
    form.setFieldValue('loginPwd', generateStrongPassword());
    void form.validateFields(['loginPwd']); // clear error
  };

  useEffect(() => {
    if (!settings.loginPwd) {
      resetPwd();
    }
  }, []);
  return (
    <>
      <Form.Item label='区域' name='region' required rules={[{ required: true }]}>
        <Select
          options={RegionOptions}
          onChange={(v) => {
            form.setFieldValue('zone', '');
            form.setFieldValue('instanceType', '');
            setRegion(v);
          }}
        />
      </Form.Item>
      <Form.Item label='可用区' name='zone' required rules={[{ required: true }]}>
        <Select options={zoneOptions} />
      </Form.Item>
      <Form.Item label='规格' name='instanceType' required rules={[{ required: true }]}>
        <Select options={instTypeOptions} />
      </Form.Item>
      <Form.Item label='资源名' name='resourceName' required rules={[{ required: true }]}>
        <Input />
      </Form.Item>
      <Form.Item label='镜像类型' name='imageType' required rules={[{ required: true }]}>
        <Select
          onChange={(v) => {
            void loadImages(v);
          }}
          options={[
            {
              label: '私有镜像',
              value: 'PRIVATE_IMAGE',
            },
            {
              label: '公共镜像',
              value: 'PUBLIC_IMAGE',
            },
          ]}
        />
      </Form.Item>
      <Form.Item label='镜像' name='imageId' required rules={[{ required: true }]}>
        <Select options={imageOptions} placeholder='选择镜像' />
      </Form.Item>
      <Form.Item label='带宽' name='bandWidth' required rules={[{ required: true }]}>
        <InputNumber className='w-full' min={1} max={1000} suffix='Mbps' />
      </Form.Item>

      <Form.Item label='登录密码' name='loginPwd' required rules={[{ required: true }]}>
        <Input
          className='cursor-pointer [&_.ant-input-group-addon]:p-0'
          onFocus={(evt) => {
            setTimeout(() => evt.target.select());
          }}
          addonAfter={
            <Button.Group size='small'>
              <Tooltip title='生成密码'>
                <Button
                  onClick={() => {
                    resetPwd();
                  }}
                  icon={<span className='icon-[ant-design--reload-outlined]'></span>}
                  type='link'
                ></Button>
              </Tooltip>
              <Button
                onClick={() => {
                  const tk = form.getFieldValue('loginPwd');
                  if (tk) {
                    void copyToClipboard(tk).then(() => {
                      void message.success('已复制！');
                    });
                  }
                }}
                type='link'
                icon={<span className='icon-[ant-design--copy-outlined]'></span>}
              />
            </Button.Group>
          }
        />
      </Form.Item>
    </>
  );
};
