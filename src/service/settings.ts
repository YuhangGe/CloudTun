import { RegionOptions } from './region';

export interface Settings {
  subscriptionSelectedId: string;
  routingMode: 'global' | 'bypassChina';
  subscriptionSystemProxy: boolean;
  resourceName: string;
  instanceType: string;
  imageId: string;
  // imageType: 'PUBLIC_IMAGE' | 'PRIVATE_IMAGE';
  diskType: string;
  zone: string;
  region: string;
  secretKey: string;
  secretId: string;
  loginPwd: string;
  bandWidth: number;
  autoStartApp: boolean;
  mobileProxyApps: string;
  mobileProxyMode: 'global' | 'app';
}

const configuredRegion = import.meta.env.VITE_SETTING_REGION;
const siliconValley = 'na-siliconvalley';
const defaultRegion =
  RegionOptions.find(({ value }) => value === configuredRegion)?.value ?? siliconValley;

export const DefaultSettings: Settings = {
  subscriptionSelectedId: '',
  routingMode: 'bypassChina',
  subscriptionSystemProxy: false,
  region: defaultRegion,
  instanceType:
    import.meta.env.VITE_SETTING_INSTANCETYPE ??
    (defaultRegion === siliconValley ? 'SA5.MEDIUM2' : ''),
  loginPwd: import.meta.env.VITE_SETTING_PASSWORD ?? '',
  // imageType: 'PRIVATE_IMAGE',
  diskType: '',
  imageId: defaultRegion === siliconValley ? 'img-dk53t5vb' : '',
  zone:
    import.meta.env.VITE_SETTING_ZONE ??
    (defaultRegion === siliconValley ? 'na-siliconvalley-1' : ''),
  resourceName: import.meta.env.VITE_RESOURCE_NAME ?? 'cloudtun',
  bandWidth: 200,
  secretKey: import.meta.env.VITE_SECRET_KEY ?? '',
  secretId: import.meta.env.VITE_SECRET_ID ?? '',
  autoStartApp: false,
  mobileProxyApps: 'com.android.chrome',
  mobileProxyMode: 'app',
};

export function validateCloudCredentials(s: Settings) {
  if (!s.secretKey || !s.secretId) {
    return '请先配置密钥参数';
  }
  return null;
}

export function validateSettings(s: Settings) {
  const credentialsError = validateCloudCredentials(s);
  if (credentialsError) return credentialsError;
  if (!s.imageId) {
    return '请先选择镜像';
  }
  if (!s.zone) return '请先选择可用区';
  if (!s.instanceType) return '请先选择实例规格';
  if (!s.diskType) return '请先选择磁盘类型';
  if (!s.loginPwd) return '请先配置主机登录密码';
  if (
    Object.keys(s).some((k) => {
      const v = s[k as keyof Settings];
      if (v == null || typeof v === 'undefined') {
        return true;
      } else {
        return false;
      }
    })
  ) {
    return '请完成全部主机参数配置';
  }
  return null;
}
