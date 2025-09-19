use std::fmt::Display;
use std::time::{self, SystemTime};

use anyhow::bail;
use chrono::{TimeZone, Utc};
use hmac::digest::Output;
use hmac::{Hmac, Mac};
use reqwest::header::HeaderValue;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub enum TencentService {
  Cvm,
  Vpc,
}

macro_rules! tencent_host {
  ($service: tt) => {
    concat!($service, ".tencentcloudapi.com")
  };
}

impl TencentService {
  pub fn get_host(&self) -> &'static str {
    match self {
      Self::Cvm => tencent_host!("cvm"),
      Self::Vpc => tencent_host!("vpc"),
    }
  }
  pub fn get_api_url(&self) -> &'static str {
    match self {
      Self::Cvm => concat!("https://", tencent_host!("cvm")),
      Self::Vpc => concat!("https://", tencent_host!("vpc")),
    }
  }
  pub fn get_service_name(&self) -> &'static str {
    match self {
      Self::Cvm => "cvm",
      Self::Vpc => "vpc",
    }
  }
  pub fn get_service_version(&self) -> &'static str {
    /*
    cvm: '2017-03-12',
    tat: '2020-10-28',
    vpc: '2017-03-12',
    billing: '2018-07-09',
     */
    match self {
      Self::Cvm => "2017-03-12",
      Self::Vpc => "2017-03-12",
    }
  }
}

fn hmac(message: &str, key: &[u8]) -> Output<Sha256> {
  let mut hmac = HmacSha256::new_from_slice(key).unwrap();
  hmac.update(message.as_bytes());
  hmac.finalize().into_bytes()
}

pub fn tencent_cloud_api_signature(
  secret_id: &str,
  secret_key: &str,
  service_host: &str,
  service_name: &str,
  timestamp: i64,
  body: &str,
) -> String {
  let hashed_payload = format!("{:x}", Sha256::digest(body.as_bytes()));
  let canonical_request = format!(
    "POST\n/\n\ncontent-type:application/json\nhost:{}\n\ncontent-type;host\n{}",
    service_host, &hashed_payload
  );
  let hashed_canonical_request = format!("{:x}", Sha256::digest(canonical_request.as_bytes()));
  // println!("{} {} {}", body, hashed_payload, hashed_canonical_request);
  // println!("{}", canonical_request);
  let date = Utc
    .timestamp_opt(timestamp, 0)
    .unwrap()
    .format("%Y-%m-%d")
    .to_string();
  let credential_scope = format!("{}/{}/tc3_request", date, service_name);
  let string_to_sign = format!(
    "TC3-HMAC-SHA256\n{}\n{}\n{}",
    timestamp, credential_scope, hashed_canonical_request
  );
  // println!("{}", string_to_sign);
  let secret_date = hmac(&date, format!("TC3{}", secret_key).as_bytes());
  let secret_service = hmac(service_name, &secret_date);
  let secret_signing = hmac("tc3_request", &secret_service);
  // println!("{:x}", &secret_date);
  // println!("{:x}", &secret_service);
  // println!("{:x}", &secret_signing);

  let signature = hmac(&string_to_sign, &secret_signing);
  format!(
    "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders=content-type;host, Signature={:x}",
    secret_id, credential_scope, signature
  )
}

use reqwest::{StatusCode, header};
use serde::Deserialize;

// 定义响应结构体
#[derive(Debug, Deserialize)]
pub struct ErrorInfo {
  #[serde(rename = "Code")]
  pub code: String,
  #[serde(rename = "Message")]
  pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ResponseData<T> {
  #[serde(rename = "Error")]
  pub error: Option<ErrorInfo>, // 使用 Option 表示可能不存在
  #[serde(rename = "RequestId")]
  pub request_id: String,
  #[serde(flatten)]
  pub result: Option<T>,
}

#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
  #[serde(rename = "Response")]
  pub response: ResponseData<T>,
}

#[derive(Debug)]
pub struct TencentErr(String);
impl Display for TencentErr {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_fmt(format_args!("Failed call tencent api due to: {}", self.0))
  }
}
impl std::error::Error for TencentErr {}

async fn tx_call_api<T: for<'a> Deserialize<'a>>(
  secret_id: &str,
  secret_key: &str,
  service: TencentService,
  region: &str,
  action: &str,
  data: String,
) -> anyhow::Result<T> {
  let ts_secs = SystemTime::now()
    .duration_since(time::UNIX_EPOCH)
    .unwrap()
    .as_secs();
  let sign = tencent_cloud_api_signature(
    secret_id,
    secret_key,
    service.get_host(),
    service.get_service_name(),
    ts_secs as i64,
    &data,
  );
  let req = reqwest::Client::new();
  let mut headers = header::HeaderMap::new();
  headers.insert(header::AUTHORIZATION, HeaderValue::from_str(&sign).unwrap());
  headers.insert(
    header::CONTENT_TYPE,
    HeaderValue::from_static("application/json"),
  );
  headers.insert(header::HOST, HeaderValue::from_static(service.get_host()));
  headers.insert("X-TC-Action", HeaderValue::from_str(action)?);
  headers.insert(
    "X-TC-Version",
    HeaderValue::from_static(service.get_service_version()),
  );
  headers.insert(
    "X-TC-Timestamp",
    HeaderValue::from_str(&ts_secs.to_string())?,
  );
  headers.insert("X-TC-Region", HeaderValue::from_str(region)?);
  let res = req
    .post(service.get_api_url())
    .headers(headers)
    .body(data)
    .send()
    .await?;

  if res.status() != StatusCode::OK {
    bail!("bad status code {}", res.status());
  }

  let res: ApiResponse<T> = res.json().await?;

  if let Some(err) = res.response.error {
    bail!("{}", err.message);
  }

  res
    .response
    .result
    .ok_or_else(|| anyhow::anyhow!("missing result"))
}

#[derive(Debug, Deserialize)]
pub struct CvmInstance {
  #[serde(rename = "InstanceId")]
  id: String,
  #[serde(rename = "InstanceName")]
  name: String,
}

#[derive(Debug)]
pub struct TencentCloudClient {
  secret_id: String,
  secret_key: String,
}

impl TencentCloudClient {
  pub fn new(secret_id: String, secret_key: String) -> Self {
    Self {
      secret_id,
      secret_key,
    }
  }

  pub async fn describe_instances(&self, region: &str) -> anyhow::Result<Vec<CvmInstance>> {
    #[derive(Debug, Deserialize)]
    struct X {
      #[serde(rename = "InstanceSet")]
      instances: Vec<CvmInstance>,
    }

    let res = tx_call_api::<X>(
      &self.secret_id,
      &self.secret_key,
      TencentService::Cvm,
      region,
      "DescribeInstances",
      "{}".to_string(),
    )
    .await?;

    Ok(res.instances)
  }
}
