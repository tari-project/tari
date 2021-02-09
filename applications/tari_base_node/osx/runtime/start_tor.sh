#!/bin/bash
#
# Script to start tor
#
TOR=$(which tor)
$TOR --allow-missing-torrc --ignore-missing-torrc \
  --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 \
  --log "notice stdout" --clientuseipv6 1
