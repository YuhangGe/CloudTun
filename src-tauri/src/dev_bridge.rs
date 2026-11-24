//! Explicitly enabled, debug-build-only bridge for Chrome CDP integration tests.
//! Requests execute the real Tauri command handlers and never expose node credentials.
use crate::proxy::*;
use axum::{
  extract::State,
  http::{HeaderMap, HeaderValue, StatusCode},
  response::{IntoResponse, Response},
  routing::{get, post},
  Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Deserialize)]
struct Request {
  command: String,
  #[serde(default)]
  args: Value,
}
fn allowed(headers: &HeaderMap) -> Option<HeaderValue> {
  let origin = headers.get("origin")?;
  if origin == "http://localhost:1420" || origin == "http://127.0.0.1:1420" {
    Some(origin.clone())
  } else {
    None
  }
}
fn cors(mut response: Response, origin: HeaderValue) -> Response {
  response
    .headers_mut()
    .insert("access-control-allow-origin", origin);
  response.headers_mut().insert(
    "access-control-allow-methods",
    HeaderValue::from_static("POST, OPTIONS"),
  );
  response.headers_mut().insert(
    "access-control-allow-headers",
    HeaderValue::from_static("content-type"),
  );
  response
    .headers_mut()
    .insert("vary", HeaderValue::from_static("Origin"));
  response
}
async fn preflight(headers: HeaderMap) -> Response {
  match allowed(&headers) {
    Some(origin) => cors(StatusCode::NO_CONTENT.into_response(), origin),
    None => StatusCode::FORBIDDEN.into_response(),
  }
}
fn result<T: serde::Serialize, E: serde::Serialize>(value: Result<T, E>) -> Result<Value, String> {
  value
    .map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
    .map_err(|error| {
      let value = serde_json::to_value(error).unwrap_or(Value::Null);
      value.as_str().unwrap_or("Rust command failed").to_owned()
    })
}
async fn command<R: Runtime>(
  State(app): State<AppHandle<R>>,
  headers: HeaderMap,
  Json(request): Json<Request>,
) -> Response {
  let Some(origin) = allowed(&headers) else {
    return StatusCode::FORBIDDEN.into_response();
  };
  let args = request.args;
  let value = match request.command.as_str() {
    "tauri_call_tencent_api" => result(
      crate::tencent::tauri_call_tencent_api(
        app.clone(),
        args["secretId"].as_str().unwrap_or_default().into(),
        args["secretKey"].as_str().unwrap_or_default().into(),
        args["service"].as_str().unwrap_or_default().into(),
        args["region"].as_str().unwrap_or_default().into(),
        args["action"].as_str().unwrap_or_default().into(),
        args["body"].as_str().unwrap_or("{}").into(),
      )
      .await,
    ),
    "tauri_new_cloud_access_token" => Ok(serde_json::json!(
      crate::cloud_access::tauri_new_cloud_access_token()
    )),
    "tauri_has_cloud_access" => Ok(serde_json::json!(
      crate::cloud_access::tauri_has_cloud_access(
        app.state(),
        args["instanceId"].as_str().unwrap_or_default().into()
      )
    )),
    "tauri_remember_cloud_access" => result(crate::cloud_access::tauri_remember_cloud_access(
      app.state(),
      args["instanceId"].as_str().unwrap_or_default().into(),
      args["token"].as_str().unwrap_or_default().into(),
    )),
    "tauri_get_logs" => result(crate::logs::tauri_get_logs(app.state())),
    "tauri_debug_log_status" => result(crate::logs::tauri_debug_log_status(app.state()).await),
    "tauri_set_debug_logging" => result(
      crate::logs::tauri_set_debug_logging(app.state(), args["enabled"].as_bool().unwrap_or(false))
        .await,
    ),
    "tauri_export_debug_logs" => {
      result(crate::logs::tauri_export_debug_logs(app.clone(), app.state()).await)
    }
    "tauri_append_log" => result(crate::logs::tauri_append_log(
      app.clone(),
      app.state(),
      args["text"].as_str().unwrap_or_default().into(),
    )),
    "tauri_geo_status" => Ok(serde_json::to_value(tauri_geo_status(app.state())).unwrap()),
    "tauri_update_geo" => result(tauri_update_geo(app.clone(), app.state()).await),
    "tauri_get_proxy_config" => result(tauri_get_proxy_config(app.state()).await),
    "tauri_set_proxy_config" => match serde_json::from_value(args["config"].clone()) {
      Ok(config) => result(tauri_set_proxy_config(app.clone(), app.state(), config).await),
      Err(_) => Err("监听配置无效".into()),
    },
    "tauri_start_proxy_client" => result(
      tauri_start_proxy_client(
        app.clone(),
        app.state(),
        args["serverIp"].as_str().unwrap_or_default().into(),
        args["cvmId"].as_str().unwrap_or_default().into(),
        serde_json::from_value(args["mode"].clone()).unwrap_or_default(),
        serde_json::from_value(args["cloud"].clone()).unwrap_or(None),
      )
      .await,
    ),
    "tauri_interval_ping_start" => result(
      crate::ping::tauri_interval_ping_start(
        args["ip"].as_str().unwrap_or_default(),
        args["token"].as_str().unwrap_or_default(),
        serde_json::from_value(args["cloud"].clone()).unwrap_or(None),
        app.clone(),
        app.state(),
      )
      .await,
    ),
    "tauri_ping_cloud_server" => result(
      crate::cloud_access::tauri_ping_cloud_server(
        app.state(),
        args["ip"].as_str().unwrap_or_default().into(),
        args["token"].as_str().unwrap_or_default().into(),
      )
      .await,
    ),
    "tauri_proxy_status" => result(tauri_proxy_status(app.clone(), app.state()).await),
    "tauri_proxy_traffic" => result(tauri_proxy_traffic(app.clone(), app.state()).await),
    "tauri_get_subscription" => result(tauri_get_subscription(app.state()).await),
    "tauri_update_subscription" => result(
      tauri_update_subscription(
        app.clone(),
        app.state(),
        args["url"].as_str().unwrap_or_default().into(),
      )
      .await,
    ),
    "tauri_ping_subscription_nodes" => result(
      tauri_ping_subscription_nodes(
        app.state(),
        args["nodeId"].as_str().map(str::to_owned),
        serde_json::from_value(args["nodeIds"].clone()).unwrap_or_default(),
      )
      .await,
    ),
    "tauri_check_subscription_node" => result(
      tauri_check_subscription_node(
        app.state(),
        args["nodeId"].as_str().unwrap_or_default().into(),
      )
      .await,
    ),
    "tauri_connect_subscription" => match serde_json::from_value(args["mode"].clone()) {
      Ok(mode) => result(
        tauri_connect_subscription(
          app.clone(),
          app.state(),
          args["nodeId"].as_str().unwrap_or_default().into(),
          mode,
        )
        .await,
      ),
      Err(_) => Err("invalid routing mode".into()),
    },
    "tauri_stop_proxy_client" => result(tauri_stop_proxy_client(app.clone(), app.state()).await),
    "tauri_set_routing_mode" => match serde_json::from_value(args["mode"].clone()) {
      Ok(mode) => result(tauri_set_routing_mode(app.clone(), app.state(), mode).await),
      Err(_) => Err("invalid routing mode".into()),
    },
    "tauri_set_system_proxy" => result(
      tauri_set_system_proxy(
        app.clone(),
        app.state(),
        args["enabled"].as_bool().unwrap_or(false),
      )
      .await,
    ),
    "tauri_interval_ping_stop" => result(crate::ping::tauri_interval_ping_stop(app.state()).await),
    _ => Err("Command is unavailable in the development bridge".into()),
  };
  cors(
    match value {
      Ok(data) => Json(json!({ "data": data })).into_response(),
      Err(error) => (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response(),
    },
    origin,
  )
}
pub fn start<R: Runtime>(app: AppHandle<R>) {
  if std::env::var("CLOUDTUN_DEV_BRIDGE").as_deref() != Ok("1") {
    return;
  }
  tauri::async_runtime::spawn(async move {
    let router = Router::new()
      .route("/health", get(|| async { "CloudTun Rust debug bridge" }))
      .route("/command", post(command::<R>).options(preflight))
      .with_state(app);
    match tokio::net::TcpListener::bind("127.0.0.1:1422").await {
      Ok(listener) => {
        log::info!("Rust debug bridge listening on 127.0.0.1:1422");
        if let Err(error) = axum::serve(listener, router).await {
          log::error!("Debug bridge stopped: {error}");
        }
      }
      Err(error) => log::error!("Cannot start debug bridge: {error}"),
    }
  });
}
