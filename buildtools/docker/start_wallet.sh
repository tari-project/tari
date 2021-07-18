#!/bin/bash
#
# Docker Start Script for console_wallet docker image in a docker-compose context
#
if [[ x$WAIT_FOR_TOR == x1 ]]; then
  echo "Waiting for tor to start up"
  sleep 30
fi

TARI_BASE=/var/tari
TARI_CONFIG=$TARI_BASE/config

if [[ ! -f $TARI_CONFIG/config.toml ]]; then
  echo "No configuration file found at $TARI_CONFIG. Copying the default instance."
  tari_console_wallet --init --create-id -b $TARI_BASE/wallet
fi

cd $TARI_BASE
tari_console_wallet "$@"
