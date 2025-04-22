import { invoke } from '@tauri-apps/api/core';
import { fetch } from '@tauri-apps/plugin-http';
import type { InstanceDeps } from './instance';
import { globalStore } from '@/store/global';
import { message } from 'jinge-antd';

export type ApiResult<T> = [Error] | [undefined, T];
const ServiceVersionMap = {
  cvm: '2017-03-12',
  tat: '2020-10-28',
  vpc: '2017-03-12',
  billing: '2018-07-09',
};

async function callTencentApi<T>({
  service,
  region,
  action,
  data,
}: {
  service: 'cvm' | 'tat' | 'vpc' | 'billing';
  region?: string;
  action: string;
  data?: Record<string, unknown>;
}): Promise<ApiResult<T>> {
  const params = globalStore.settings;
  const timestamp = Math.floor(Date.now() / 1000);
  const body = data ? JSON.stringify(data) : '{}';
  const sign = await invoke<string>('tauri_calc_tencent_cloud_api_signature', {
    secretId: params.secretId,
    secretKey: params.secretKey,
    service,
    timestamp,
    body,
  });
  const host = `${service}.tencentcloudapi.com`;
  const headers = {
    authorization: sign,
    ['content-type']: 'application/json',
    host: host,
    'X-TC-Action': action,
    'X-TC-Version': ServiceVersionMap[service],
    'X-TC-Timestamp': timestamp.toString(),
    'X-TC-Region': region ?? params.region,
  };
  const res = await fetch(`https://${host}`, {
    method: 'POST',
    headers,
    body,
  });
  if (res.status !== 200) {
    throw new Error(`bad status code: ${res.status}`);
  }
  // console.log('start call api...');
  // const res = await invoke<string>('tauri_call_tencent_cloud_api', {
  //   service,
  //   version: ServiceVersionMap[service],
  //   region: region ?? params.region,
  //   action,
  //   body,
  //   auth: sign,
  //   timestamp,
  // });
  const resData = (await res.json()) as {
    Response: {
      Error?: { Code: string; Message: string };
      RequestId: string;
    };
  };
  // eslint-disable-next-line no-console
  console.log(`-- Tecent Clound Api -- `, action, resData);
  const Err = resData.Response.Error;
  if (Err) {
    const err = new Error(Err.Message);
    err.name = Err.Code;
    message.error(`${Err.Code}::${Err.Message}`);
    return [err];
  }
  return [undefined, resData.Response as T];
}
export interface ApiFilter {
  Name: string;
  Values: string[];
}
export interface DescribeParams {
  region?: string;
  Filters?: ApiFilter[];
  Limit?: number;
  Offset?: number;
}
export interface CVMInstance {
  InstanceId: string;
  InstanceName: string;
  PublicIpAddresses: string[];
  InternetAccessible: {
    InternetChargeType: 'TRAFFIC_POSTPAID_BY_HOUR';
    InternetMaxBandwidthOut: number;
  };
  InstanceState: 'PENDING' | 'RUNNING' | 'STOPPED' | 'SHUTDOWN' | 'TERMINATING' | 'LAUNCH_FAILED';
}
export function DescribeInstances({
  region,
  ...data
}: DescribeParams & {
  InstanceIds?: string[];
}) {
  return callTencentApi<{
    TotalCount: number;
    InstanceSet: CVMInstance[];
  }>({
    service: 'cvm',
    region,
    action: 'DescribeInstances',
    data,
  });
}

export interface CVMInstanceType {
  InstanceType: string;
  CPU: number;
  Memory: number;
}
export function DescribeInstanceTypeConfigs({
  region,
  ...data
}: {
  region?: string;
  Filters?: ApiFilter[];
}) {
  return callTencentApi<{
    InstanceTypeConfigSet: CVMInstanceType[];
  }>({
    service: 'cvm',
    region,
    action: 'DescribeInstanceTypeConfigs',
    data,
  });
}
function getInstanceApiParams() {
  const settings = globalStore.settings;
  if (!settings.zone || !settings.imageId) throw new Error('settings missing');
  return {
    InstanceChargeType: 'SPOTPAID',
    InstanceType: settings.instanceType,
    InstanceCount: 1,
    LoginSettings: { Password: settings.loginPwd },
    Placement: { Zone: settings.zone, ProjectId: 0 },
    SystemDisk: {
      DiskSize: 20,
      DiskType: 'CLOUD_PREMIUM', // 'CLOUD_BSSD'
    },
    ImageId: settings.imageId,
    InstanceName: settings.resourceName,
    HostName: 'vray',
    EnhancedService: {
      AutomationService: { Enabled: true },
      MonitorService: { Enabled: true },
      SecurityService: { Enabled: true },
    },

    // InternetAccessible: {
    //   InternetMaxBandwidthOut: 0,
    //   PublicIpAssigned: false,
    // },
    InternetAccessible: {
      InternetMaxBandwidthOut: settings.bandWidth,
      PublicIpAssigned: true,
      InternetChargeType: 'TRAFFIC_POSTPAID_BY_HOUR',
      InternetServiceProvider: 'BGP',
    },
  };
}

export async function CreateInstance(deps: InstanceDeps) {
  const data = {
    ...getInstanceApiParams(),
    SecurityGroupIds: [deps.sgId],
    VirtualPrivateCloud: {
      AsVpcGateway: false,
      Ipv6AddressCount: 0,
      SubnetId: deps.subnetId,
      VpcId: deps.vpcId,
    },
  };
  return await callTencentApi<{
    InstanceIdSet: string[];
  }>({
    service: 'cvm',
    action: 'RunInstances',
    data,
  }).then((res) => {
    if (res[0]) return res;
    return [
      undefined,
      {
        InstanceId: res[1].InstanceIdSet[0],
        InstanceState: 'PENDING',
        InstanceName: data.InstanceName,
      },
    ] as ApiResult<CVMInstance>;
  });
}

export async function TerminateInstance(id: string) {
  return await callTencentApi({
    service: 'cvm',
    action: 'TerminateInstances',
    data: {
      InstanceIds: [id],
    },
  });
}
export interface CVMPrice {
  InstancePrice: { ChargeUnit: string; UnitPriceDiscount: number };
  BandwidthPrice: { ChargeUnit: string; UnitPriceDiscount: number };
}
export function InquiryPriceRunInstances() {
  return callTencentApi<{
    Price: CVMPrice;
  }>({
    service: 'cvm',
    action: 'InquiryPriceRunInstances',
    data: getInstanceApiParams(),
  });
}

export function ResetInstancesInternetMaxBandwidth(instanceId: string, maxBandwidth: number) {
  return callTencentApi({
    service: 'cvm',
    action: 'ResetInstancesInternetMaxBandwidth',
    data: {
      InstanceIds: [instanceId],
      InternetAccessible: {
        InternetMaxBandwidthOut: maxBandwidth,
      },
    },
  });
}

export function AssociateAddress({
  region,
  ...data
}: {
  region?: string;
  AddressId: string;
  InstanceId: string;
}) {
  return callTencentApi({
    service: 'vpc',
    region,
    action: 'AssociateAddress',
    data,
  });
}

export interface Vpc {
  VpcId: string;
  VpcName: string;
}
export function DescribeVpcs({ region, ...data }: DescribeParams) {
  return callTencentApi<{
    TotalCount: number;
    VpcSet: Vpc[];
  }>({
    service: 'vpc',
    region,
    action: 'DescribeVpcs',
    data,
  });
}
export function CreateVpc({
  region,
  ...data
}: {
  region?: string;
  VpcName: string;
  CidrBlock: '10.0.0.0/12' | '172.16.0.0/12' | '192.168.0.0/16';
}) {
  return callTencentApi<{
    Vpc: Vpc;
  }>({
    service: 'vpc',
    region,
    action: 'CreateVpc',
    data,
  });
}

export interface Subnet {
  SubnetId: string;
  SubnetName: string;
}
export function DescribeSubnets({ region, ...data }: DescribeParams) {
  return callTencentApi<{
    TotalCount: number;
    SubnetSet: Subnet[];
  }>({
    service: 'vpc',
    region,
    action: 'DescribeSubnets',
    data,
  });
}
export function CreateSubnet({
  region,
  ...data
}: {
  region?: string;
  SubnetName: string;
  VpcId: string;
  Zone: string;
  CidrBlock: string;
}) {
  return callTencentApi<{
    Subnet: Subnet;
  }>({
    service: 'vpc',
    region,
    action: 'CreateSubnet',
    data,
  });
}

export interface SecurityGroup {
  SecurityGroupId: string;
  SecurityGroupName: string;
}
export function DescribeSecurityGroups({ region, ...data }: DescribeParams) {
  return callTencentApi<{
    TotalCount: number;
    SecurityGroupSet: SecurityGroup[];
  }>({
    service: 'vpc',
    region,
    action: 'DescribeSecurityGroups',
    data,
  });
}
interface SecurityGroupPolicy {
  Port: string;
  Protocol: 'TCP' | 'UDP' | 'ICMP' | 'ICMPv6' | 'ALL';
  Action: 'ACCEPT' | 'DROP';
  CidrBlock: string;
  PolicyDescription?: string;
}
export function CreateSecurityGroupWithPolicies({
  region,
  ...data
}: {
  region?: string;
  GroupName: string;
  GroupDescription: string;
  SecurityGroupPolicySet: {
    /** 出站规则，通常为 all */
    Egress: SecurityGroupPolicy[];
    /** 入站规则，比如 tcp:22 */
    Ingress: SecurityGroupPolicy[];
  };
}) {
  return callTencentApi<{
    SecurityGroup: SecurityGroup;
  }>({
    service: 'vpc',
    region,
    action: 'CreateSecurityGroupWithPolicies',
    data,
  });
}
export interface CVMImage {
  ImageId: string;
  ImageName: string;
}
export function DescribeImages({ region, ...data }: DescribeParams) {
  return callTencentApi<{
    TotalCount: number;
    ImageSet: CVMImage[];
  }>({
    service: 'cvm',
    region,
    action: 'DescribeImages',
    data,
  });
}

export interface Zone {
  Zone: string;
  ZoneName: string;
  ZoneState: 'AVAILABLE';
}
export function DescribeZones({ region }: { region?: string }) {
  return callTencentApi<{
    TotalCount: number;
    ZoneSet: Zone[];
  }>({
    service: 'cvm',
    region,
    action: 'DescribeZones',
  }).then((res) => {
    if (!res[0]) {
      res[1].ZoneSet = res[1].ZoneSet.filter((z) => z.ZoneState === 'AVAILABLE');
    }
    return res;
  });
}

export interface CVMCommand {
  CommandId: string;
  CommandName: string;
  Content: string;
}
export function DescribeCommands({ region, ...data }: DescribeParams) {
  return callTencentApi<{
    TotalCount: number;
    CommandSet: CVMCommand[];
  }>({
    service: 'tat',
    region,
    action: 'DescribeCommands',
    data,
  });
}
export function DescribeAutomationAgentStatus({
  region,
  ...data
}: DescribeParams & {
  InstanceIds: string[];
}) {
  return callTencentApi<{
    TotalCount: number;
    AutomationAgentSet: { AgentStatus: 'Offline' | 'Online' }[];
  }>({
    service: 'tat',
    region,
    action: 'DescribeAutomationAgentStatus',
    data,
  });
}
export function ModifyCommand({
  region,
  ...data
}: {
  region?: string;
  CommandId: string;
  Content: string;
}) {
  return callTencentApi({
    service: 'tat',
    region,
    action: 'ModifyCommand',
    data,
  });
}
export function CreateCommand({
  region,
  ...data
}: {
  region?: string;
  Username: string;
  Timeout: number;
  WorkingDirectory: string;
  CommandName: string;
  Content: string;
}) {
  return callTencentApi<{
    CommandId: string;
  }>({
    service: 'tat',
    region,
    action: 'CreateCommand',
    data,
  });
}
export function InvokeCommand({
  region,
  ...data
}: {
  region?: string;
  CommandId: string;
  InstanceIds: string[];
}) {
  return callTencentApi<{
    InvocationId: string;
  }>({
    service: 'tat',
    region,
    action: 'InvokeCommand',
    data,
  });
}
export function DeleteCommand({ region, ...data }: { region?: string; CommandId: string }) {
  return callTencentApi({
    service: 'tat',
    region,
    action: 'DeleteCommand',
    data,
  });
}

export interface CVMBalance {
  RealBalance: number;
  CashAccountBalance: number;
}
export function DescribeAccountBalance(params?: { region?: string }) {
  return callTencentApi<CVMBalance>({
    service: 'billing',
    region: params?.region,
    action: 'DescribeAccountBalance',
  });
}
