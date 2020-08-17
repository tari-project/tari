#!/bin/bash
#
# Docker Start Script
#  script to run tor, sleep 15 seconds and start the base node
#

tor
sleep 15
if [[ ! -f ~/.tari/config.toml ]]; then
  tari_base_node --init --create-id
fi

cd ~/.tari
tari_base_node "$@"
