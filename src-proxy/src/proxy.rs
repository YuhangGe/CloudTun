use crate::{
  outbound::{AnyStream, Outbound},
  route::MatchType,
  routing::Routing,
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
use std::{convert::Infallible, sync::Arc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

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
  let route = routing.decide(&host, port).await;
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
  let mut stream = match result {
    Ok(Ok(stream)) => stream,
    Ok(Err(_)) => {
      log(
        "proxy::error",
        &format!("Unable to connect to {host}:{port}"),
      );
      return Ok(StatusCode::BAD_GATEWAY.into_response());
    }
    Err(_) => return Ok(StatusCode::GATEWAY_TIMEOUT.into_response()),
  };
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
        _ = cancel.cancelled() => {},
        result = async {
          let upgraded = hyper::upgrade::on(req).await.map_err(std::io::Error::other)?;
          tokio::io::copy_bidirectional(&mut TokioIo::new(upgraded), &mut stream).await
        } => { if let Err(e) = result { log("proxy::error", &format!("CONNECT tunnel closed: {e}")); } }
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
    _ => return Ok(StatusCode::BAD_GATEWAY.into_response()),
  };
  let token = cancel.clone();
  tracker.spawn(async move {
    tokio::select! { biased; _ = token.cancelled() => {}, _ = connection.with_upgrades() => {} }
  });
  match tokio::time::timeout(CONNECT_TIMEOUT, sender.send_request(req)).await {
    Ok(Ok(mut response)) => {
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
            } => { if let Err(error) = result { log("proxy::error", &format!("WebSocket tunnel closed: {error}")); } }
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
    Ok(Err(_)) => Ok(StatusCode::BAD_GATEWAY.into_response()),
    Err(_) => Ok(StatusCode::GATEWAY_TIMEOUT.into_response()),
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
