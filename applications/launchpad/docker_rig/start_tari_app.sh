#!/bin/bash
#
# Docker Start Script for Tari applications
# The docker compose environment should set the following envars
# - APP_NAME - the name of the app to run. This var is used to set the location of log files, and app-specific config
# - APP_EXEC - the name of the application executable. Just the name is enough, since the Dockerfile will put it in /usr/bin
# - WAIT_FOR_TOR - set to the delay in seconds to pause at the beginning of this script.
#

APP_NAME=${APP_NAME:-base_node}
APP_EXEC=${APP_EXEC:-tari_base_node}
WAIT_FOR_TOR=${WAIT_FOR_TOR:-0}
TARI_BASE=/var/tari/$APP_NAME
CONFIG=/var/tari/config
USER_ID=${USER_ID:-1000}
GROUP_ID=${GROUP_ID:-1000}

echo "Starting $APP_NAME with following docker environment:"
echo "executable: $APP_EXEC"
echo "WAIT_FOR_TOR: $WAIT_FOR_TOR"
echo "base folder (in container): $TARI_BASE"
echo "config folder (in container): $CONFIG"

if [[ $WAIT_FOR_TOR != 0 ]]; then
  echo "Waiting $WAIT_FOR_TOR seconds for Tor to start up"
  sleep "$WAIT_FOR_TOR"
fi

if [[ ! -d  "$TARI_BASE" ]]; then
  mkdir -p "$TARI_BASE"
fi

cd "$TARI_BASE" || exit 1

echo "Starting ${APP_NAME}..."
echo Command: $APP_EXEC "$@"
$APP_EXEC "$@" || exit 1

