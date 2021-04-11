#!/bin/bash
#
# Script to start tor
#

# Check if tor is running
if ! pgrep -x "tor" > /dev/null
then
  # Check if both ports are being used
  if ! lsof -i :9050 | grep "tor" && ! lsof -i :9051 | grep "tor"
  then
    killall tor
    gnome-terminal --working-directory="$PWD" -- tor --allow-missing-torrc --ignore-missing-torrc \
    --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 \
    --log "notice stdout" --clientuseipv6 1
  else
    gnome-terminal --working-directory="$PWD" -- tor --allow-missing-torrc --ignore-missing-torrc \
    --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 \
    --log "notice stdout" --clientuseipv6 1
  fi
fi
