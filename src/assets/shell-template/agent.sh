#!/bin/bash
cd /home/ubuntu
curl -O https://raw.githubusercontent.com/YuhangGe/cloudtun-server-release/refs/heads/release/cloudtun-server
chmod +x cloudtun-server
nohup ./cloudtun-server --token=$TOKEN --secret-id=$SECRET_ID --secret-key=$SECRET_KEY --region=$REGION --cvm-name=$CVM_NAME > /home/ubuntu/cloudtun.log 2>&1 &
# wget https://github.com/v2fly/v2ray-core/releases/download/v5.38.0/v2ray-linux-64.zip
# unzip v2ray-linux-64.zip -d v2ray
# cd v2ray
# echo '{
#   "inbounds": [
#     {
#       "port": 24817,
#       "protocol": "vmess",
#       "settings": {
#         "clients": [
#           {
#             "id": "$TOKEN"
#           }
#         ]
#       }
#     }
#   ],
#   "outbounds": [
#     {
#       "protocol": "freedom"
#     }
#   ]
# }' > config.json
# nohup ./v2ray run > /home/ubuntu/v2ray.log 2>&1 &