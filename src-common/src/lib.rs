#![feature(portable_simd)] // nightly required

pub const X_TOKEN_VALUE: &'static str = "O2WC32M7F1";
pub const X_TOKEN_KEY: &'static str = "x-token";
pub const X_CONNECT_HOST_KEY: &'static str = "x-connect-host";
pub const X_CONNECT_PORT_KEY: &'static str = "x-connect-port";
pub const X_SECRET_KEY: &'static str = "x-secret";
pub const REMOTE_PROXY_PORT: u16 = 24816;
pub const LOCAL_HTTP_PROXY_PORT: u16 = 7892;

use core::simd::Simd;

type U8x16 = Simd<u8, 16>;

/// 对 data 原地 XOR，key 长度固定为 16 字节（重复使用）
pub fn xor_inplace_simd(data: &mut [u8], key: &[u8]) {
  //  for i in 0..data.len() {
  //   data[i] = data[i]  ^ key[i % 16]
  //  }
  let lanes = 16usize;
  let key_vec = U8x16::from_slice(key);

  // 处理 16 字节对齐的块
  let mut i = 0usize;
  while i + lanes <= data.len() {
    // 从 slice 装载到向量（要求 slice 长度至少为 lanes）
    let mut v = U8x16::from_slice(&data[i..i + lanes]);
    v ^= key_vec;
    // 将向量写回 slice
    data[i..i + lanes].copy_from_slice(&v.to_array());
    i += lanes;
  }

  // 处理尾部剩余（不足 16 字节）
  let rem = &mut data[i..];
  for (j, byte) in rem.iter_mut().enumerate() {
    *byte ^= key[j];
  }
}

pub fn hex2str(hex: &[u8]) -> String {
  hex
    .iter()
    .map(|n| format!("{:02x}", n))
    .collect::<Vec<String>>()
    .join(",")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_xor_inplace_simd() {
    let key: [u8; 16] = *b"0123456789ABCDEF";
    let mut data = b"Hello, world! This is a test of XOR SIMD.".to_vec();
    // 备份做纯标量对比
    let mut expected = data.clone();
    for (i, b) in expected.iter_mut().enumerate() {
      *b ^= key[i % 16];
    }

    xor_inplace_simd(&mut data, &key);
    assert_eq!(data, expected);

    // 双重 XOR 恢复原文
    xor_inplace_simd(&mut data, &key);
    assert_eq!(data, b"Hello, world! This is a test of XOR SIMD.");
  }
}
