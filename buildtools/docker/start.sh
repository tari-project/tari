#!/bin/bash
#
# Docker Start Script
#  script to run tor, sleep 15 seconds and start the base node
#

tor &
sleep 15
TARI_CONFIG=~/.tari/config/config.toml
if [[ ! -f $TARI_CONFIG ]]; then
  tari_base_node --init --create-id
  # fix for docker, bind grpc to 0.0.0.0 instead of loopback
  sed -i -e 's/127.0.0.1:18142/0.0.0.0:18142/' $TARI_CONFIG
fi

cd ~/.tari
tari_base_node "$@"
