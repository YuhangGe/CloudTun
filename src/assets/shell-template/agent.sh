#!/bin/bash
set -euo pipefail

cd /home/ubuntu
umask 077
export CLOUDTUN_ACCESS_TOKEN=$ACCESS_TOKEN

if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 ]]; then
  echo 'cloudtun-server only supports Linux x64.' >&2
  exit 1
fi

RELEASE_URL=https://github.com/YuhangGe/CloudTun/releases/latest/download
ASSET=cloudtun-server-linux-x64
DOWNLOAD_DIR=$(mktemp -d /home/ubuntu/.cloudtun-download.XXXXXX)
trap 'rm -rf "$DOWNLOAD_DIR"' EXIT

curl --fail --silent --show-error --location --retry 5 --retry-delay 2 --connect-timeout 10 --max-time 300 \
  "$RELEASE_URL/$ASSET" -o "$DOWNLOAD_DIR/$ASSET"
curl --fail --silent --show-error --location --retry 5 --retry-delay 2 --connect-timeout 10 --max-time 300 \
  "$RELEASE_URL/$ASSET.sha256" -o "$DOWNLOAD_DIR/$ASSET.sha256"
(
  cd "$DOWNLOAD_DIR"
  sha256sum --check "$ASSET.sha256"
)
chmod +x "$DOWNLOAD_DIR/$ASSET"
# Rename on the same filesystem so a failed download cannot replace the executable.
mv -f "$DOWNLOAD_DIR/$ASSET" /home/ubuntu/cloudtun-server

nohup /home/ubuntu/cloudtun-server \
  --secret-id="$SECRET_ID" \
  --secret-key="$SECRET_KEY" \
  --region="$REGION" \
  --cvm-name="$CVM_NAME" \
  --data-dir=/home/ubuntu/.cloudtun-server \
  > /home/ubuntu/cloudtun.log 2>&1 < /dev/null &
