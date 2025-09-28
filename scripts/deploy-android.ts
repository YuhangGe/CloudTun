import { execSync } from 'node:child_process';
import path from 'node:path';
import { unzipFile } from './util.ts';
import { $ } from 'bun';

const CWD = process.cwd();

async function run() {
  await $`rm dist/cloudtun.apks dist/cloudtun-release.apk`;
  const cmd = `java -jar ${path.join(CWD, 'bundletool-all-1.18.1.jar')} build-apks --bundle=${path.join(CWD, 'src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab')} --output=${path.join(CWD, 'dist/cloudtun.apks')} --mode=universal --ks=${path.join(CWD, 'scripts/key.jks')} --ks-pass=pass:a123456 --ks-key-alias=key --key-pass=pass:a123456`;
  console.info('\n\nApks generated.\n\nExtracting apk...\n');
  await execSync(cmd);
  await unzipFile(path.join(CWD, 'dist/cloudtun.apks'), path.join(CWD, 'dist'));
  // await $`unzip app-release.apks`;
  await $`mv dist/universal.apk dist/cloudtun-release.apk`;
}

run().catch((ex) => {
  console.error(ex);
  process.exit(-1);
});
