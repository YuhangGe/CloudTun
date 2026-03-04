export interface Settings {
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
  token: string;
  autoProxy: boolean;
  autoStartApp: boolean;
  proxyRules: string;
  mobileProxyApps: string;
  mobileProxyMode: 'global' | 'app';
}

const DEFAULT_PROXY_RULES = `tencent.com: direct
.cn: direct
qcloud.com: direct
npmmirror.com: direct`;

export const DefaultSettings: Settings = {
  region: import.meta.env.VITE_SETTING_REGION ?? 'ap-singapore',
  instanceType: import.meta.env.VITE_SETTING_INSTANCETYPE ?? '',
  token: import.meta.env.VITE_VMESS_ID ?? '',
  loginPwd: import.meta.env.VITE_SETTING_PASSWORD ?? '',
  // imageType: 'PRIVATE_IMAGE',
  diskType: 'CLOUD_PREMIUM',
  imageId: '',
  zone: import.meta.env.VITE_SETTING_ZONE ?? '',
  resourceName: import.meta.env.VITE_RESOURCE_NAME ?? 'cloudtun',
  bandWidth: 200,
  secretKey: import.meta.env.VITE_SECRET_KEY ?? '',
  secretId: import.meta.env.VITE_SECRET_ID ?? '',
  autoProxy: false,
  autoStartApp: false,
  proxyRules: DEFAULT_PROXY_RULES,
  mobileProxyApps: 'com.android.chrome',
  mobileProxyMode: 'app',
};

export function validateSettings(s: Settings) {
  if (!s.secretKey || !s.secretId || !s.token) {
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
