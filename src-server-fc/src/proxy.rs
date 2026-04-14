use std::sync::Arc;

use axum::{
  body::Bytes,
  extract::{
    State,
    ws::{Message, WebSocket, WebSocketUpgrade},
  },
  http::{HeaderMap, StatusCode},
  response::IntoResponse,
};
use cloudtun_common::{
  constant::{X_CONNECT_HOST_KEY, X_CONNECT_PORT_KEY, X_TOKEN_KEY},
  encode::xor_inplace_simd,
};
use futures_util::{SinkExt, StreamExt};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpStream,
};

use crate::Context;

pub async fn proxy_handler(
  ws: WebSocketUpgrade,
  headers: HeaderMap,
  State(context): State<Arc<Context>>,
) -> impl IntoResponse {
  if !headers
    .get(X_TOKEN_KEY)
    .map(|tk| tk.eq(&context.token))
    .unwrap_or(false)
  {
    return Err(StatusCode::UNAUTHORIZED);
  }

  let Some(remote_host) = headers
    .get(X_CONNECT_HOST_KEY)
    .and_then(|v| v.to_str().map(|s| s.to_string()).ok())
  else {
    return Err(StatusCode::BAD_REQUEST);
  };

  let Some(remote_port) = headers
    .get(X_CONNECT_PORT_KEY)
    .and_then(|v| v.to_str().ok().and_then(|s| s.parse::<u16>().ok()))
  else {
    return Err(StatusCode::BAD_REQUEST);
  };

  let secret = context.password.clone();

  Ok(ws.on_upgrade(move |socket| handle_socket(socket, remote_host, remote_port, secret)))
}

async fn handle_socket(
  socket: WebSocket,
  remote_host: String,
  remote_port: u16,
  secret: Arc<Vec<u8>>,
) {
  let (mut ws_sender, mut ws_receiver) = socket.split();
  let Ok(remote_tcp) = TcpStream::connect((remote_host, remote_port)).await else {
    return;
  };
  let (mut remote_reader, mut remote_writer) = remote_tcp.into_split();
  let mut recv_ws_handle = tokio::spawn(async move {
    while let Some(Ok(msg)) = ws_receiver.next().await {
      match msg {
        Message::Binary(data) => {
          let mut x = data.to_vec();
          // println!("B: {}", hex2str(&x));
          // println!("S: {}", hex2str(&secret));
          xor_inplace_simd(&mut x, &secret);
          //  println!("B2: {}", hex2str(&x));

          if let Err(e) = remote_writer.write_all(&x).await {
            eprintln!("failed send data to remote {e}");
          }
        }
        Message::Close(_) => break,
        _ => (),
      }
    }
  });

  let mut read_remote_handle = tokio::spawn(async move {
    let mut buf = [0u8; 4096];
    loop {
      match remote_reader.read(&mut buf).await {
        Ok(0) => {
          let _ = ws_sender.send(Message::Close(None)).await;
          break;
        }
        Ok(size) => {
          if let Err(e) = ws_sender
            .send(Message::Binary(Bytes::copy_from_slice(&buf[..size])))
            .await
          {
            eprintln!("failed send data to client: {e}");
            break;
          }
        }
        Err(e) => {
          eprintln!("failed read remote data: {e}");
        }
      }
    }
  });

  // // If any one of the tasks exit, abort the other.
  tokio::select! {
    _ = (&mut read_remote_handle) => {
      recv_ws_handle.abort();
    },
    _ = (&mut recv_ws_handle) => {
      read_remote_handle.abort();
    }
  }

  // println!("Websocket context  destroyed");
}
