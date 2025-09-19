#!/bin/bash
cd /home/ubuntu
curl -O https://raw.githubusercontent.com/YuhangGe/cloudtun-server-release/refs/heads/release/cloudtun-server
chmod +x cloudtun-server
nohup ./cloudtun-server -t=$TOKEN --secret-id=$SECRET_ID --secret-key=$SECRET_KEY --region=$REGION --cvm-name=$CVM_NAME > /home/ubuntu/log.log 2>&1 &
