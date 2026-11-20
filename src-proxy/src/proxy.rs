use crate::{
  outbound::{AnyStream, Outbound},
  route::MatchType,
  routing::Routing,
  traffic::{DiagnosticStream, TransferStats},
};
use axum::{
  body::Body,
  response::{IntoResponse, Response},
};
use cloudtun_common::proxy::CONNECT_TIMEOUT;
use hyper::{
  Method, Request, StatusCode, Uri,
  body::Incoming,
  header::{CONNECTION, HOST, HeaderMap, HeaderValue, UPGRADE},
};
use hyper_util::rt::TokioIo;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{convert::Infallible, sync::Arc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

pub(crate) fn tunnel_error_level(error: &std::io::Error) -> &'static str {
  // An EOF in an opaque CONNECT stream does not establish application failure.
  // Keep its full details as a warning; authentication/protocol/timeout errors stay red.
  if error.kind() == std::io::ErrorKind::UnexpectedEof {
    "proxy::warn"
  } else {
    "proxy::error"
  }
}

pub(crate) async fn proxy_request<F: Fn(&str, &str) + Send + Sync + 'static>(
  mut req: Request<Incoming>,
  outbound: Outbound,
  routing: Routing,
  log: Arc<F>,
  cancel: CancellationToken,
  tracker: TaskTracker,
) -> Result<Response, Infallible> {
  let connect = req.method() == Method::CONNECT;
  let authority = match req.uri().authority().cloned() {
    Some(authority) => authority,
    None => {
      return Ok(
        (
          StatusCode::BAD_REQUEST,
          "Use absolute HTTP URLs or CONNECT host:port",
        )
          .into_response(),
      );
    }
  };
  if !connect && !matches!(req.uri().scheme_str(), Some("http" | "ws")) {
    return Ok((StatusCode::BAD_REQUEST, "HTTPS requests require CONNECT").into_response());
  }
  let host = authority
    .host()
    .trim_start_matches('[')
    .trim_end_matches(']')
    .to_owned();
  let port = match authority.port_u16() {
    Some(port) if port != 0 => port,
    None if !connect => 80,
    _ => return Ok(StatusCode::BAD_REQUEST.into_response()),
  };
  let request_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
  let started = std::time::Instant::now();
  let route = routing.decide(&host, port).await;
  let outbound_label = match route {
    MatchType::Direct => "direct".into(),
    MatchType::Deny => "deny".into(),
    MatchType::Proxy => outbound.diagnostic_label(),
  };
  let context = format!(
    "req={request_id} target={host}:{port} method={} outbound={}",
    req.method(),
    outbound_label
  );
  let log = Arc::new(move |kind: &str, text: &str| log(kind, &format!("[{context}] {text}")));
  log("proxy::debug", &format!("stage=route route={route}"));
  if matches!(route, MatchType::Deny) {
    return Ok(StatusCode::FORBIDDEN.into_response());
  }
  let result = tokio::time::timeout(CONNECT_TIMEOUT, async {
    if matches!(route, MatchType::Direct) {
      let tcp = crate::tcp::connect(&host, port).await?;
      tcp.set_nodelay(true)?;
      Ok::<AnyStream, std::io::Error>(Box::new(tcp))
    } else {
      outbound
        .connect(&host, port, cancel.clone(), tracker.clone(), log.clone())
        .await
    }
  })
  .await;
  let stream = match result {
    Ok(Ok(stream)) => stream,
    Ok(Err(error)) => {
      log(
        "proxy::error",
        &format!(
          "stage=outbound.connect elapsed_ms={} kind={:?} Unable to connect: {error:?}",
          started.elapsed().as_millis(),
          error.kind()
        ),
      );
      return Ok(StatusCode::BAD_GATEWAY.into_response());
    }
    Err(_) => {
      log(
        "proxy::error",
        &format!(
          "stage=outbound.connect timeout elapsed_ms={}",
          started.elapsed().as_millis()
        ),
      );
      return Ok(StatusCode::GATEWAY_TIMEOUT.into_response());
    }
  };
  log(
    "proxy::debug",
    &format!(
      "stage=outbound.connected elapsed_ms={}",
      started.elapsed().as_millis()
    ),
  );
  let stats = TransferStats::default();
  let mut stream = DiagnosticStream(stream, stats.clone());
  log(
    "proxy::info",
    &format!(
      "{} ==> {host}:{port}",
      if matches!(route, MatchType::Direct) {
        "Direct"
      } else {
        "Proxy"
      }
    ),
  );
  if connect {
    tracker.spawn(async move {
      tokio::select! {
        biased;
        _ = cancel.cancelled() => { log("proxy::debug", &format!("stage=connect.cancel elapsed_ms={} {stats}", started.elapsed().as_millis())); },
        result = async {
          let upgraded = hyper::upgrade::on(req).await.map_err(std::io::Error::other)?;
          log("proxy::debug", "stage=connect.upgraded");
          tokio::io::copy_bidirectional(&mut TokioIo::new(upgraded), &mut stream).await
        } => {
          match result {
            Ok(_) => log("proxy::debug", &format!("stage=connect.closed elapsed_ms={} {stats}", started.elapsed().as_millis())),
            Err(error) => log(tunnel_error_level(&error), &format!("stage=connect.transfer elapsed_ms={} {stats} kind={:?} CONNECT tunnel closed: {error:?}", started.elapsed().as_millis(), error.kind())),
          }
        }
      }
    });
    return Ok(Response::new(Body::empty()));
  }
  // A normal HTTP proxy request uses an absolute URI. Origin servers expect path/query.
  let path = req
    .uri()
    .path_and_query()
    .map(|v| v.as_str())
    .unwrap_or("/");
  let Ok(uri) = path.parse::<Uri>() else {
    return Ok(StatusCode::BAD_REQUEST.into_response());
  };
  *req.uri_mut() = uri;
  let websocket = req.method() == Method::GET && is_websocket_upgrade(req.headers());
  let client_upgrade = websocket.then(|| hyper::upgrade::on(&mut req));
  strip_hop_headers(req.headers_mut());
  if websocket {
    req
      .headers_mut()
      .insert(CONNECTION, HeaderValue::from_static("upgrade"));
    req
      .headers_mut()
      .insert(UPGRADE, HeaderValue::from_static("websocket"));
  }
  if let Ok(value) = authority.as_str().parse() {
    req.headers_mut().insert(HOST, value);
  }
  let (mut sender, connection) = match tokio::time::timeout(
    CONNECT_TIMEOUT,
    hyper::client::conn::http1::handshake(TokioIo::new(stream)),
  )
  .await
  {
    Ok(Ok(pair)) => pair,
    Ok(Err(error)) => {
      log("proxy::error", &format!("stage=origin.handshake {error:?}"));
      return Ok(StatusCode::BAD_GATEWAY.into_response());
    }
    Err(_) => {
      log("proxy::error", "stage=origin.handshake timeout");
      return Ok(StatusCode::GATEWAY_TIMEOUT.into_response());
    }
  };
  let token = cancel.clone();
  let connection_log = log.clone();
  let connection_stats = stats.clone();
  tracker.spawn(async move {
    tokio::select! {
      biased;
      _ = token.cancelled() => { connection_log("proxy::debug", "stage=origin.cancel"); },
      result = connection.with_upgrades() => {
        match result {
          Ok(()) => connection_log("proxy::debug", &format!("stage=origin.closed elapsed_ms={} {connection_stats}", started.elapsed().as_millis())),
          Err(error) => connection_log("proxy::error", &format!("stage=origin.http elapsed_ms={} {connection_stats} {error:?}", started.elapsed().as_millis())),
        }
      }
    }
  });
  match tokio::time::timeout(CONNECT_TIMEOUT, sender.send_request(req)).await {
    Ok(Ok(mut response)) => {
      log(
        "proxy::debug",
        &format!(
          "stage=origin.response status={} elapsed_ms={}",
          response.status(),
          started.elapsed().as_millis()
        ),
      );
      if response.status() == StatusCode::SWITCHING_PROTOCOLS {
        if !websocket || !is_websocket_upgrade(response.headers()) {
          return Ok(StatusCode::BAD_GATEWAY.into_response());
        }
        let server_upgrade = hyper::upgrade::on(&mut response);
        let client_upgrade = client_upgrade.expect("validated WebSocket request");
        tracker.spawn(async move {
          tokio::select! {
            biased;
            _ = cancel.cancelled() => {},
            result = async {
              let (client, server) = tokio::try_join!(client_upgrade, server_upgrade).map_err(std::io::Error::other)?;
              tokio::io::copy_bidirectional(&mut TokioIo::new(client), &mut TokioIo::new(server)).await
            } => { if let Err(error) = result { log(tunnel_error_level(&error), &format!("stage=websocket.transfer elapsed_ms={} {stats} WebSocket tunnel closed: {error:?}", started.elapsed().as_millis())); } }
          }
        });
        let (mut parts, _) = response.into_parts();
        strip_hop_headers(&mut parts.headers);
        parts
          .headers
          .insert(CONNECTION, HeaderValue::from_static("upgrade"));
        parts
          .headers
          .insert(UPGRADE, HeaderValue::from_static("websocket"));
        return Ok(Response::from_parts(parts, Body::empty()));
      }
      let (mut parts, body) = response.into_parts();
      strip_hop_headers(&mut parts.headers);
      Ok(Response::from_parts(parts, Body::new(body)))
    }
    Ok(Err(error)) => {
      log(
        "proxy::error",
        &format!(
          "stage=origin.request elapsed_ms={} {error:?}",
          started.elapsed().as_millis()
        ),
      );
      Ok(StatusCode::BAD_GATEWAY.into_response())
    }
    Err(_) => {
      log(
        "proxy::error",
        &format!(
          "stage=origin.request timeout elapsed_ms={}",
          started.elapsed().as_millis()
        ),
      );
      Ok(StatusCode::GATEWAY_TIMEOUT.into_response())
    }
  }
}

fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
  headers
    .get(UPGRADE)
    .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"))
    && headers
      .get_all(CONNECTION)
      .iter()
      .filter_map(|value| value.to_str().ok())
      .flat_map(|value| value.split(','))
      .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
}

fn strip_hop_headers(headers: &mut HeaderMap) {
  let names = headers
    .get_all("connection")
    .iter()
    .filter_map(|v| v.to_str().ok())
    .flat_map(|v| v.split(',').map(|s| s.trim().to_owned()))
    .collect::<Vec<_>>();
  for name in names {
    headers.remove(name);
  }
  for name in [
    "connection",
    "proxy-connection",
    "keep-alive",
    "proxy-authorization",
    "proxy-authenticate",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
  ] {
    headers.remove(name);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn uncertain_tunnel_eof_warns_but_protocol_and_connection_failures_stay_errors() {
    assert_eq!(
      tunnel_error_level(&std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "early eof"
      )),
      "proxy::warn"
    );
    for kind in [
      std::io::ErrorKind::InvalidData,
      std::io::ErrorKind::TimedOut,
      std::io::ErrorKind::PermissionDenied,
      std::io::ErrorKind::BrokenPipe,
    ] {
      assert_eq!(tunnel_error_level(&kind.into()), "proxy::error");
    }
  }
}
