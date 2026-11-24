#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudMonitor {
  pub secret_id: String,
  pub secret_key: String,
  pub region: String,
}

pub fn instance_missing(value: &serde_json::Value, id: &str) -> anyhow::Result<bool> {
  let response = value
    .get("Response")
    .ok_or_else(|| anyhow::anyhow!("实例查询响应无效"))?;
  if let Some(error) = response.get("Error") {
    if error["Code"].as_str() == Some("InvalidInstanceId.NotFound") {
      return Ok(true);
    }
    anyhow::bail!("实例查询失败，不能确认实例丢失");
  }
  let instances = response["InstanceSet"]
    .as_array()
    .ok_or_else(|| anyhow::anyhow!("实例查询缺少实例列表"))?;
  if instances.is_empty() && response["TotalCount"].as_u64() == Some(0) {
    return Ok(true);
  }
  if instances
    .iter()
    .any(|instance| instance["InstanceId"].as_str() == Some(id))
  {
    return Ok(false);
  }
  anyhow::bail!("实例查询结果不匹配，不能确认实例丢失")
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  #[test]
  fn only_confirmed_absence_counts_as_instance_loss() {
    assert!(
      instance_missing(
        &json!({"Response":{"TotalCount":0,"InstanceSet":[]}}),
        "ins-test"
      )
      .unwrap()
    );
    assert!(
      instance_missing(
        &json!({"Response":{"Error":{"Code":"InvalidInstanceId.NotFound"}}}),
        "ins-test"
      )
      .unwrap()
    );
    for state in ["RUNNING", "STOPPED", "SHUTDOWN", "TERMINATING"] {
      assert!(!instance_missing(&json!({"Response":{"TotalCount":1,"InstanceSet":[{"InstanceId":"ins-test","InstanceState":state}]}}), "ins-test").unwrap());
    }
  }
  #[test]
  fn errors_incomplete_or_unrelated_results_never_confirm_loss() {
    for value in [
      json!({"Response":{"Error":{"Code":"AuthFailure"}}}),
      json!({"Response":{"Error":{"Code":"RequestLimitExceeded"}}}),
      json!({"Response":{"InstanceSet":[]}}),
      json!({"Response":{"TotalCount":1,"InstanceSet":[]}}),
      json!({"Response":{"TotalCount":1,"InstanceSet":[{"InstanceId":"ins-other"}]}}),
      json!({"Response":{"TotalCount":0}}),
      json!({}),
    ] {
      assert!(instance_missing(&value, "ins-test").is_err(), "{value}");
    }
  }
}

impl CloudMonitor {
  pub async fn confirm_missing(&self, id: &str) -> anyhow::Result<bool> {
    anyhow::ensure!(
      !self.secret_id.is_empty() && !self.secret_key.is_empty() && !self.region.is_empty(),
      "实例核查配置不完整"
    );
    let body = serde_json::json!({"InstanceIds":[id],"Limit":1}).to_string();
    let timestamp = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)?
      .as_secs() as i64;
    let signature = cloudtun_common::tencent::tencent_cloud_api_signature(
      &self.secret_id,
      &self.secret_key,
      "cvm.tencentcloudapi.com",
      "cvm",
      timestamp,
      &body,
    );
    let client = crate::http_client_builder()?
      .no_proxy()
      .redirect(reqwest::redirect::Policy::none())
      .connect_timeout(std::time::Duration::from_secs(5))
      .timeout(std::time::Duration::from_secs(20))
      .build()?;
    let mut response = client
      .post("https://cvm.tencentcloudapi.com")
      .header("Authorization", signature)
      .header("Content-Type", "application/json")
      .header("X-TC-Action", "DescribeInstances")
      .header("X-TC-Version", "2017-03-12")
      .header("X-TC-Timestamp", timestamp.to_string())
      .header("X-TC-Region", &self.region)
      .body(body)
      .send()
      .await?;
    anyhow::ensure!(response.status().is_success(), "实例核查 HTTP 失败");
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
      anyhow::ensure!(bytes.len() + chunk.len() <= 1024 * 1024, "实例核查响应过大");
      bytes.extend_from_slice(&chunk);
    }
    instance_missing(&serde_json::from_slice(&bytes)?, id)
  }
}
pub const INSTANCE_LOST_MESSAGE: &str = "云主机实例已丢失，代理已自动关闭";
