import { spawnSync, execFileSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';

const env = { ...process.env };
if (process.platform === 'darwin') {
  env.JAVA_HOME ??= execFileSync('/usr/libexec/java_home', ['-v', '21'], {
    encoding: 'utf8',
  }).trim();
  env.ANDROID_HOME ??= path.join(homedir(), 'Library/Android/sdk');
}
const sdk = env.ANDROID_HOME ?? env.ANDROID_SDK_ROOT;
if (!sdk || !existsSync(sdk)) throw new Error('请设置 ANDROID_HOME，指向已安装的 Android SDK');
const ndkRoot = path.join(sdk, 'ndk');
env.NDK_HOME ??=
  env.ANDROID_NDK_HOME ??
  path.join(
    ndkRoot,
    readdirSync(ndkRoot)
      .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))
      .at(-1)!,
  );
env.ANDROID_NDK_HOME = env.NDK_HOME;
const prebuilt = path.join(env.NDK_HOME, 'toolchains/llvm/prebuilt');
const toolchain = path.join(
  prebuilt,
  readdirSync(prebuilt).find((name) => !name.startsWith('.'))!,
);
for (const [target, clang] of [
  ['aarch64-linux-android', 'aarch64-linux-android31'],
  ['armv7-linux-androideabi', 'armv7a-linux-androideabi31'],
  ['i686-linux-android', 'i686-linux-android31'],
  ['x86_64-linux-android', 'x86_64-linux-android31'],
]) {
  env[`BINDGEN_EXTRA_CLANG_ARGS_${target.replaceAll('-', '_')}`] =
    `--sysroot=${path.join(toolchain, 'sysroot')} --target=${clang}`;
  env[`CARGO_TARGET_${target.replaceAll('-', '_').toUpperCase()}_LINKER`] = path.join(
    toolchain,
    'bin',
    `${clang}-clang`,
  );
  env[`CC_${target.replaceAll('-', '_')}`] = path.join(toolchain, 'bin', `${clang}-clang`);
  env[`AR_${target.replaceAll('-', '_')}`] = path.join(toolchain, 'bin', 'llvm-ar');
}
const result = spawnSync('pnpm', ['exec', 'tauri', 'android', ...process.argv.slice(2)], {
  env,
  stdio: 'inherit',
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
