#!/bin/bash
#
# Docker Start Script for Tari applications
# The docker compose environment should set the following envars
# - APP_NAME - the name of the app to run. This var is used to set the location of log files, and app-specific config
# - APP_EXEC - the name of the application executable. Just the name is enough, since the Dockerfile will put it in /usr/bin
# - WAIT_FOR_TOR - set to the delay in seconds to pause at the beginning of this script.
#
umask 077

APP_NAME=${APP_NAME:-base_node}
APP_EXEC=${APP_EXEC:-tari_base_node}
WAIT_FOR_TOR=${WAIT_FOR_TOR:-0}
TARI_BASE=${TARI_BASE:-/var/tari/$APP_NAME}
TARI_CONFIG=${TARI_CONFIG:-/var/tari/config/config.toml}
USER_ID=${USER_ID:-1000}
GROUP_ID=${GROUP_ID:-1000}

echo "Starting ${APP_NAME} with following docker environment:"
echo "executable: ${APP_EXEC}"
echo "WAIT_FOR_TOR: $WAIT_FOR_TOR"
echo "base folder (in container): ${TARI_BASE}"
echo "config file (in container): ${TARI_CONFIG}"
echo "arguments (in container): ${@}"
echo "arguments count (in container): ${#}"

if [[ $WAIT_FOR_TOR != 0 ]]; then
  echo "Waiting $WAIT_FOR_TOR seconds for Tor to start up"
  sleep "$WAIT_FOR_TOR"
fi

if [[ ! -d "${TARI_BASE}" ]]; then
  if ! mkdir -p "${TARI_BASE}"; then
    owner=$(stat -c '%U:%G (%u:%g)' "${TARI_BASE}" 2>/dev/null || stat -c '%U:%G (%u:%g)' "$(dirname "${TARI_BASE}")" 2>/dev/null || echo "unknown")
    echo "ERROR: Failed to create base directory '${TARI_BASE}'." >&2
    echo "Directory ownership is ${owner}, but the container runs as user 'tari' (uid 1000:gid 1000)." >&2
    echo "Please ensure the host mount directory is writable by uid 1000 (e.g. 'sudo chown -R 1000:1000 <host_dir>')." >&2
    exit 1
  fi
fi

if [[ ! -w "${TARI_BASE}" ]]; then
  owner=$(stat -c '%U:%G (%u:%g)' "${TARI_BASE}" 2>/dev/null || echo "unknown")
  echo "ERROR: Base directory '${TARI_BASE}' is not writable." >&2
  echo "Directory ownership is ${owner}, but the container runs as user 'tari' (uid 1000:gid 1000)." >&2
  echo "Please ensure the host mount directory is writable by uid 1000 (e.g. 'sudo chown -R 1000:1000 <host_dir>')." >&2
  exit 1
fi

cd "${TARI_BASE}" || {
  echo "ERROR: Failed to change working directory to '${TARI_BASE}'." >&2
  exit 1
}

echo "Starting ${APP_NAME} ..."
echo " > command: ${APP_EXEC} --config ${TARI_CONFIG} --base-path ${TARI_BASE} \"$@\" <"
exec "${APP_EXEC}" --config "${TARI_CONFIG}" --base-path "${TARI_BASE}" "$@" || exit 1
