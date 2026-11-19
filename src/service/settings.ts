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

export const DefaultSettings: Settings = {
  subscriptionSelectedId: '',
  routingMode: 'bypassChina',
  subscriptionSystemProxy: false,
  region: import.meta.env.VITE_SETTING_REGION ?? 'ap-singapore',
  instanceType: import.meta.env.VITE_SETTING_INSTANCETYPE ?? '',
  loginPwd: import.meta.env.VITE_SETTING_PASSWORD ?? '',
  // imageType: 'PRIVATE_IMAGE',
  diskType: 'CLOUD_PREMIUM',
  imageId: '',
  zone: import.meta.env.VITE_SETTING_ZONE ?? '',
  resourceName: import.meta.env.VITE_RESOURCE_NAME ?? 'cloudtun',
  bandWidth: 200,
  secretKey: import.meta.env.VITE_SECRET_KEY ?? '',
  secretId: import.meta.env.VITE_SECRET_ID ?? '',
  autoStartApp: false,
  mobileProxyApps: 'com.android.chrome',
  mobileProxyMode: 'app',
};

export function validateSettings(s: Settings) {
  if (!s.secretKey || !s.secretId) {
    return '请先配置密钥参数';
  }
  if (!s.imageId) {
    return '请先选择镜像';
  }
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
