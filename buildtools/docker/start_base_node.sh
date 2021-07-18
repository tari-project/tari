#!/bin/bash
#
# Docker Start Script for base nodet docker image in a docker-compose context
#
if [[ x$WAIT_FOR_TOR == x1 ]]; then
  echo "Waiting for tor to start up"
  sleep 30
fi

TARI_BASE=/var/tari/base_node
TARI_CONFIG=$TARI_BASE/config
NETWORK=${TARI_NETWORK:-weatherwax}

if [[ ! -f $TARI_BASE/data/$NETWORK ]]; then
  echo "No node data found at data/$NETWORK"
  tari_base_node --init -b $TARI_BASE
  tari_base_node --create-id -b $TARI_BASE
fi

cd $TARI_BASE
tari_base_node "$@"
