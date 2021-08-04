#!/bin/bash
#
# Docker Start Script for Tari applications
# The docker compose environment should set the following envars
# - APP_NAME - the name of the app to run. This var is used to set the location of log files, and app-soecific config
# - APP_EXEC - the name of the application executable. Just the name is enough, since the Dockerfile will put it in /usr/bin
# - CREATE_CONFIG - set to 1 if we should write a default config file if one is missing.
# - CREATE_ID - set to 1 if we should create an id file for this application if one is missing. It will be called
#               {network}_{app_name}_id.json
# - WAIT_FOR_TOR - set to 1 to place a 30 second delay at the beginning of this script.
# - TARI_NETWORK - the Tari network to configure the docker rig for
#

APP_NAME=${APP_NAME:-base_node}
APP_EXEC=${APP_EXEC:-tari_base_node}
CREATE_CONFIG=${CREATE_CONFIG:-0}
CREATE_ID=${CREATE_ID:-0}
WAIT_FOR_TOR=${WAIT_FOR_TOR:-0}
NETWORK=${TARI_NETWORK:-weatherwax}
TARI_BASE=/var/tari/$APP_NAME
CONFIG=/var/tari/config

echo "Starting $APP_NAME with following docker environment:"
echo "executable: $APP_EXEC"
echo "network: $NETWORK"
echo "CREATE_CONFIG: $CREATE_CONFIG"
echo "CREATE_ID: $CREATE_ID"
echo "WAIT_FOR_TOR: $WAIT_FOR_TOR"
echo "base folder (in container): $TARI_BASE"
echo "config folder (in container): $CONFIG"

if [[ $WAIT_FOR_TOR == 1 ]]; then
  echo "Waiting for tor to start up"
  sleep 30
fi

cd $TARI_BASE

if [[ $CREATE_CONFIG == 1 && ! -f $CONFIG/config.toml ]]; then
  echo "I could not find a global Tari configuration file. I can create a default one for you, or you can set this up"
  echo "yourself and place it in the global config path (usually ~/.tari/config/config.toml, but YMMV)"
  # TODO what it says on the box
  exit 1
fi

ID_FILENAME=${NETWORK}_${APP_NAME}_id.json

if [[ $CREATE_ID && ! -f $ID_FILENAME ]]; then

  echo "I could not find a network identity file for this node ($ID_FILENAME)."
  echo "So I'll create one for you real quick."
  $APP_EXEC -c $CONFIG/config.toml -b $TARI_BASE --create_id
fi

$APP_EXEC "$@"
