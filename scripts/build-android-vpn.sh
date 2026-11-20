#!/usr/bin/env bash
set -euo pipefail
# Called by Gradle for the exact ABI/profile being packaged.
project_root="$(cd "$(dirname "$0")/.." && pwd)"
vpn_target="$1"
vpn_profile="$2"
vpn_output="$3"
vpn_ndk="$4"
vpn_toolchain="$(find "$vpn_ndk/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d | head -1)"
case "$vpn_target" in
  aarch64-linux-android) vpn_clang=aarch64-linux-android31 ;;
  armv7-linux-androideabi) vpn_clang=armv7a-linux-androideabi31 ;;
  i686-linux-android) vpn_clang=i686-linux-android31 ;;
  x86_64-linux-android) vpn_clang=x86_64-linux-android31 ;;
  *) exit 1 ;;
esac
vpn_key="${vpn_target//-/_}"
export "BINDGEN_EXTRA_CLANG_ARGS_${vpn_key}=--sysroot=$vpn_toolchain/sysroot --target=$vpn_clang"
export "CC_${vpn_key}=$vpn_toolchain/bin/$vpn_clang-clang"
export "AR_${vpn_key}=$vpn_toolchain/bin/llvm-ar"
export "CARGO_TARGET_$(echo "$vpn_key" | tr '[:lower:]' '[:upper:]')_LINKER=$vpn_toolchain/bin/$vpn_clang-clang"
vpn_args=(build --manifest-path "$project_root/src-vpn/Cargo.toml" --target "$vpn_target" --lib --locked)
if [[ "$vpn_profile" == release ]]; then vpn_args+=(--release); fi
# Keep a stable cache for this independent crate and its lockfile.
export CARGO_TARGET_DIR="$project_root/src-vpn/target"
cargo "${vpn_args[@]}"
mkdir -p "$vpn_output"
cp "$CARGO_TARGET_DIR/$vpn_target/$vpn_profile/libcloudtunvpn.so" "$vpn_output/libcloudtunvpn.so"
