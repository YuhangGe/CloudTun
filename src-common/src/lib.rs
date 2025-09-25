#![feature(portable_simd)] // nightly required

pub mod constant;
pub mod encode;

pub mod stream;
pub mod util;

#[cfg(feature = "ping")]
pub mod ping;
#[cfg(feature = "proxy")]
pub mod proxy;
#[cfg(feature = "tencent")]
pub mod tencent;
