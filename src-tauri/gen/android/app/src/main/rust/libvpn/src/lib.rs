extern crate jni;

use jni::objects::{JClass, JString};
use jni::JNIEnv;
use tun2proxy::
 
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn Java_com_cloudv2ray_app_CloudV2RayVpnService_start(
  mut env: JNIEnv,
  _class: JClass,
  config_str: JString,
  tun_fd: i32,
) -> i32 {
  let rust_string = env
    .get_string(&config_str)
    .expect("Couldn't get Java string!")
    .to_string_lossy()
    .into_owned();

  

  println!("Got from kotlin: {}, {}", rust_string, tun_fd);
  999
}
