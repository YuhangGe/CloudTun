use axum::{body::Bytes, http::HeaderValue};
use cloudtun_common::{
  REMOTE_PROXY_PORT, X_CONNECT_HOST_KEY, X_CONNECT_PORT_KEY, X_SECRET_KEY, X_TOKEN_KEY,
  X_TOKEN_VALUE, xor_inplace_simd,
};
use futures_util::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use std::io::{Error, ErrorKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use tokio_tungstenite::{
  connect_async,
  tungstenite::{client::IntoClientRequest, protocol::Message},
};

use lazy_static::lazy_static;
use rand::Rng;

lazy_static! {
  static ref SECRET: Vec<u8> = {
    let rng = rand::rng();
    rng.random_iter::<u8>().take(16).collect()
    // vec![1;16]
  };
  static ref SECRET_HEX: String = SECRET
    .iter()
    .map(|n| format!("{:02x}", n))
    .collect::<Vec<_>>()
    .join("");
}

pub async fn proxy_tunnel(
  upgraded: Upgraded,
  proxy_host: &str,
  target_host: String,
  target_port: u16,
) -> std::io::Result<()> {
  // 建立 websocket 连接
  let url = format!("ws://{proxy_host}:{REMOTE_PROXY_PORT}/ws");
  let mut request = url
    .into_client_request()
    .map_err(|e| Error::new(ErrorKind::Other, e))?;

  let headers = request.headers_mut();
  headers.append(X_TOKEN_KEY, HeaderValue::from_static(X_TOKEN_VALUE));
  headers.append(
    X_CONNECT_HOST_KEY,
    HeaderValue::from_str(&target_host).unwrap(),
  );
  headers.append(
    X_CONNECT_PORT_KEY,
    HeaderValue::from_str(&target_port.to_string()).unwrap(),
  );
  headers.append(X_SECRET_KEY, HeaderValue::from_str(&SECRET_HEX).unwrap());

  let (ws_stream, _) = connect_async(request)
    .await
    .map_err(|e| Error::new(ErrorKind::Other, e))?;
  let (mut ws_sink, mut ws_stream) = ws_stream.split();
  let upgraded = TokioIo::new(upgraded);
  let (mut upgraded_reader, mut upgraded_writer) = tokio::io::split(upgraded);

  // 任务1: 从 Upgraded -> WebSocket
  let read_handle = tokio::spawn(async move {
    let mut buf = [0u8; 1024];
    loop {
      match upgraded_reader.read(&mut buf).await {
        Ok(0) => {
          let _ = ws_sink.send(Message::Close(None)).await;
          break;
        }
        Ok(n) => {
          let data = &mut buf[..n];
          // println!("A: {}", hex2str(data));
          // println!("S: {}", hex2str(&SECRET));
          xor_inplace_simd(data, &SECRET);
          // println!("A2: {}", hex2str(data));

          if let Err(e) = ws_sink
            .send(Message::Binary(Bytes::copy_from_slice(data)))
            .await
          {
            eprintln!("ws send error: {e}");
            break;
          }
        }
        Err(e) => {
          eprintln!("read upgraded error: {e}");
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
          if let Err(e) = upgraded_writer.write_all(&data).await {
            eprintln!("write to upgraded error: {e}");
            break;
          }
        }
        Ok(Message::Close(_)) => {
          // println!("got close");
          let _ = upgraded_writer.shutdown().await;
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
