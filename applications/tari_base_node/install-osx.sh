#!/bin/bash

# Installer script for Tari base node. This script is bundled with OSX and Linux versions of the Tari base node
# binary distributions.

function display_center() {
    columns="$(tput cols)"
    echo "$1" | while IFS= read -r line; do
        printf "%*s\n" $(( (${#line} + columns) / 2)) "$line"
    done
}

function banner() {
  columns="$(tput cols)"
  for (( c=1; c<=$columns; c++ )); do
      echo -n "—"
  done

  display_center " ✨  $1 ✨ "
  for (( c=1; c<=$columns; c++ )); do
      echo -n "—"
  done

  echo
}

DATA_DIR=${1:-"$HOME/.tari"}
NETWORK=rincewind

banner Installing and setting up your Tari Base Node
echo "Creating Tari data folder in $DATA_DIR"
mkdir -p $DATA_DIR/$NETWORK
echo "Copying configuraton files"
cp rincewind-simple.toml $DATA_DIR/config.toml
cp log4rs-sample.yml $DATA_DIR/log4rs.yml
echo "Configuration complete."

./install_tor.sh no-run
# Start Tor
osascript -e "tell application \"Terminal\" to do script \"sh ${PWD}/start_tor.sh\""
echo "Waiting for Tor to start..."
sleep 20
echo "Ok"

# Configure Base Node
./tari_base_node --create_id

banner Running Tari Base Node
# Run Base Node
./tari_base_node