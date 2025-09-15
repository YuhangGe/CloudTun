use std::thread;

use cloudtun_proxy::{start_proxy, stop_proxy};

fn main() {
  let proxy_handle = thread::spawn(move || {
    start_proxy(Some(
      "ad.baidu.com:deny
gstatic.com:proxy
youtube.com:proxy
google.com:proxy"
        .into(),
    ));
  });

  ctrlc::set_handler(move || {
    stop_proxy();
  })
  .unwrap();

  proxy_handle.join().unwrap();

  println!("Bye!");
}
