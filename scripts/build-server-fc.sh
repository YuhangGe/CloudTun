#!/bin/bash

CWD=$(dirname $0)
if [[ `basename $(pwd)` = 'scripts' ]]; then
    cd ../
else
    cd `dirname $CWD`
fi

docker build -t fc-rust-demo .
docker build -f Dockerfile.base -t cloudtun-base .
docker build -f Dockerfile.server-fc -t cloudtun-server-fc .
docker run --name cloudtun-server-fc cloudtun-server-fc bash
docker cp cloudtun-server-fc:/app/cloudtun-server-fc ./bin/cloudtun-server-fc/server
docker rm -f cloudtun-server-fc