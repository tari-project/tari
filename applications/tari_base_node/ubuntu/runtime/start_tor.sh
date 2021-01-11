#!/bin/bash
#
# Script to start tor
#
if [ -z "${use_parent_paths}" ]
then
    no_output=""
else
    no_output=">/dev/null"
fi
gnome-terminal --working-directory="$PWD" -- tor --allow-missing-torrc --ignore-missing-torrc \
  --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 \
  --log "notice stdout" --clientuseipv6 1 "${no_output}"
