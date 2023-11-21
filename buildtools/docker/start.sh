#!/bin/bash
#
# Docker Start Script
#  script to run tor, sleep 15 seconds and start the base node
#

# ToDo:
#  Add tor running check
tor &
sleep 15

TARI_CONFIG=~/.tari/config/config.toml
if [[ ! -f $TARI_CONFIG ]]; then
  minotari_node --init
  # fix for docker, bind grpc to 0.0.0.0 instead of loopback
  sed -i -e 's/127.0.0.1:18142/0.0.0.0:18142/' $TARI_CONFIG
fi

cd ~/.tari
minotari_node "$@"
