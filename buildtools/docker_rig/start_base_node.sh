#!/bin/bash
#
# Docker Start Script for base nodet docker image in a docker-compose context
#
if [[ x$WAIT_FOR_TOR == x1 ]]; then
  echo "Waiting for tor to start up"
  sleep 30
fi

TARI_BASE=/var/tari/base_node
CONFIG=/var/tari/config
NETWORK=${TARI_NETWORK:-weatherwax}

cd $TARI_BASE

if [[ ! -f $CONFIG/config.toml ]]; then
  echo "I could not find a global Tari configuration file. I can create a default one for you, or you can set this up"
  echo "yourself and place it in the global config path (usually ~/.tari/config/config.toml, but YMMV)"
  # TODO what it says on the box
  exit 1
fi

if [[ ! -f ${NETWORK}_base_node_id.json ]]; then
  echo "I could not find a network identity file for this node (${NETWORK}_base_node_id.json)."
  echo "So I'll create one for you real quick."
  tari_base_node -c $CONFIG/config.toml -b $TARI_BASE --create_id
fi

tari_base_node "$@"
