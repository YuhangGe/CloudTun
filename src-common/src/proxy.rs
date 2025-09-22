use std::{
  io::{Error, ErrorKind},
  sync::Arc,
};

use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use rand::Rng;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::{
  connect_async,
  tungstenite::{client::IntoClientRequest, protocol::Message},
};
use tokio_util::bytes::Bytes;

use crate::{
  constant::{X_CONNECT_HOST_KEY, X_CONNECT_PORT_KEY, X_SECRET_KEY, X_TOKEN_KEY},
  encode::xor_inplace_simd,
  util::hex2str,
};

pub async fn proxy_to_cloudtun_server<
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
  F: Fn(&str, &str) + Send + Sync + 'static,
>(
  local_stream: S,
  server: Arc<(String, u16, String)>,
  target_host: String,
  target_port: u16,
  secret: Arc<(Vec<u8>, String)>,
  log_fn: Arc<F>,
) -> std::io::Result<()> {
  let url = format!("ws://{}:{}/ws", server.0, server.1);
  let mut request = url
    .into_client_request()
    .map_err(|e| Error::new(ErrorKind::Other, e))?;

  let headers = request.headers_mut();
  headers.append(X_TOKEN_KEY, HeaderValue::from_str(&server.2).unwrap());
  headers.append(
    X_CONNECT_HOST_KEY,
    HeaderValue::from_str(&target_host).unwrap(),
  );
  headers.append(
    X_CONNECT_PORT_KEY,
    HeaderValue::from_str(&target_port.to_string()).unwrap(),
  );
  headers.append(X_SECRET_KEY, HeaderValue::from_str(&secret.1).unwrap());

  let (ws_stream, _) = connect_async(request)
    .await
    .map_err(|e| Error::new(ErrorKind::Other, e))?;
  let (mut ws_sink, mut ws_stream) = ws_stream.split();
  let (mut local_stream_reader, mut local_stream_writer) = tokio::io::split(local_stream);

  log_fn(
    "proxy::info",
    &format!("Proxy ==> {}:{}", target_host, target_port),
  );

  let read_log_fn = log_fn.clone();
  // 任务1: 从 Upgraded -> WebSocket
  let read_handle = tokio::spawn(async move {
    let mut buf = [0u8; 8192];
    loop {
      match local_stream_reader.read(&mut buf).await {
        Ok(0) => {
          // let _ = ws_sink.send(Message::Close(None)).await;
          // eprintln!("read upgraded zero.");
          break;
        }
        Ok(n) => {
          let data = &mut buf[..n];
          // println!("xxxx {n}, {}", hex2str(data));

          // println!("A: {}", hex2str(data));
          // println!("S: {}", hex2str(&SECRET));
          xor_inplace_simd(data, &secret.0);
          // println!("A2: {}", hex2str(data));

          if let Err(e) = ws_sink
            .send(Message::Binary(Bytes::copy_from_slice(data)))
            .await
          {
            read_log_fn("proxy::error", &format!("ws send error: {e}"));
            break;
          }
        }
        Err(e) => {
          read_log_fn("proxy::error", &format!("read upgraded error: {e}"));
          break;
        }
      }
    }
  });

  // 任务2: 从 WebSocket -> Upgraded
  let write_handle = tokio::spawn(async move {
    while let Some(msg) = ws_stream.next().await {
      match msg {
        Ok(Message::Binary(data)) => {
          let l = data.len();
          if let Err(e) = local_stream_writer.write_all(&data).await {
            log_fn(
              "proxy::error",
              &format!("write {l} bytes to upgraded error: {e}"),
            );
            break;
          }
        }
        Ok(Message::Close(_)) => {
          // eprintln!("got close");
          let _ = local_stream_writer.flush().await;
          break;
        }
        _ => {}
      }
    }
  });

  // 等待两个任务结束
  let _ = tokio::try_join!(read_handle, write_handle);

  Ok(())
}

pub fn generate_proxy_secret() -> (Vec<u8>, String) {
  let rng = rand::rng();
  let secret: Vec<_> = rng.random_iter::<u8>().take(16).collect();
  let secret: Vec<_> = (0..16).map(|_| 0).collect();
  let secret_hex = secret
    .iter()
    .map(|n| format!("{:02x}", n))
    .collect::<Vec<_>>()
    .join("");
  println!("xxx len {}, {}", secret.len(), secret_hex);
  (secret, secret_hex)
}
