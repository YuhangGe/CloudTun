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
  // let rem = &mut data[i..];
  // for (j, byte) in rem.iter_mut().enumerate() {
  //   *byte ^= key[j];
  // }
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
