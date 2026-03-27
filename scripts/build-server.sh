#!/bin/bash

CWD=$(dirname $0)
if [[ `basename $(pwd)` = 'scripts' ]]; then
    cd ../
else
    cd `dirname $CWD`
fi

docker build -t fc-rust-demo .
docker build -f Dockerfile.base -t cloudtun-base .
docker build -f Dockerfile.server -t cloudtun-server .
docker run --name cloudtun-server cloudtun-server bash
docker cp cloudtun-server:/app/cloudtun-server ./bin/cloudtun-server
docker rm -f cloudtun-server