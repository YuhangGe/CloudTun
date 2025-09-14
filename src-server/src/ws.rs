use std::ops::ControlFlow;

use axum::{
  Router,
  body::Bytes,
  extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
  http::{HeaderMap, StatusCode},
  response::IntoResponse,
  routing::{any, get},
};
use axum_extra::{TypedHeader, headers};
use futures_util::StreamExt;

use crate::config::TOKEN;

pub async fn ws_handler(ws: WebSocketUpgrade, headers: HeaderMap) -> impl IntoResponse {
  let Some(v) = headers.get("x-token") else {
    return Err(StatusCode::UNAUTHORIZED);
  };
  if !v.eq(TOKEN) {
    return Err(StatusCode::UNAUTHORIZED);
  }

  Ok(ws.on_upgrade(move |socket| handle_socket(socket)))
}

async fn handle_socket(mut socket: WebSocket) {
  let Some(msg) = socket.recv().await else {
    println!("client disconnected");
    return;
  };
  let Ok(msg) = msg else {
    return;
  };
  if process_message(msg).is_break() {
    return;
  }
  if socket
    .send(Message::Text(format!("Hi !").into()))
    .await
    .is_err()
  {
    println!("client disconnected");
    return;
  }

  // By splitting socket we can send and receive at the same time. In this example we will send
  // unsolicited messages to client based on some sort of server's internal event (i.e .timer).
  let (mut sender, mut receiver) = socket.split();

  // This second task will receive messages from client and print them on server console
  let mut recv_task = tokio::spawn(async move {
    let mut cnt = 0;
    while let Some(Ok(msg)) = receiver.next().await {
      cnt += 1;
      // print message and break if instructed to do so
      if process_message(msg).is_break() {
        break;
      }
    }
    cnt
  });

  // // If any one of the tasks exit, abort the other.
  // tokio::select! {
  //     rv_a = (&mut send_task) => {
  //         match rv_a {
  //             Ok(a) => println!("{a} messages sent to client"),
  //             Err(a) => println!("Error sending messages {a:?}")
  //         }
  //         recv_task.abort();
  //     },
  //     rv_b = (&mut recv_task) => {
  //         match rv_b {

  //         }
  //         send_task.abort();
  //     }
  // }

  match recv_task.await {
    Ok(b) => println!("Received {b} messages"),
    Err(b) => println!("Error receiving messages {b:?}"),
  };

  // returning from the handler closes the websocket connection
  println!("Websocket context  destroyed");
}

fn process_message(msg: Message) -> ControlFlow<(), ()> {
  match msg {
    Message::Binary(d) => {
      println!(">>> got bytes: {}", d.len());
    }
    Message::Close(_) => {
      return ControlFlow::Break(());
    }
    _ => (),
  }
  ControlFlow::Continue(())
}
