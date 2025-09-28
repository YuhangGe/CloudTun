/// <reference types="vite/client" />

interface ImportMetaEnv {
  VITE_SECRET_ID?: string;
  VITE_SECRET_KEY?: string;
  VITE_VMESS_ID?: string;
  VITE_SETTING_REGION?: string;
  VITE_SETTING_ZONE?: string;
  VITE_RESOURCE_NAME?: string;
  VITE_SETTING_INSTANCETYPE?: string;
  VITE_SETTING_PASSWORD?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
