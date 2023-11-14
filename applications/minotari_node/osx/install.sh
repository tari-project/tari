#!/bin/bash

# Installer script for Minotari base node. This script is bundled with OSX 
# versions of the Minotari base node binary distributions.

logo="
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣤⣾⣿⣿⣶⣤⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⣠⣶⣿⣿⣿⣿⠛⠿⣿⣿⣿⣿⣿⣦⣤⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⣤⣾⣿⣿⣿⡿⠋⠀⠀⠀⠀⠀⠀⠉⠛⠿⣿⣿⣿⣿⣷⣦⣄⠀⠀⠀⠀
⣴⣿⣿⣿⣿⣿⣉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠛⢿⣿⣿⣿⣿⣶⣤
⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣦⣤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⣿⣿⣿
⣿⣿⣿⠀⠀⠀⠀⠉⠉⠛⠿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣶⣤⣄⣀⠀⠀⠀⣿⣿⣿
⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⠀⠈⠉⠛⠛⠿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
⢿⣿⣿⣷⣄⠀⠀⠀⠀⠀⠀⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣉⣿⣿⣿⣿⠟
⠀⠈⢿⣿⣿⣷⣄⠀⠀⠀⠀⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⢀⣴⣿⣿⣿⡿⠋⠀⠀
⠀⠀⠀⠈⢿⣿⣿⣷⡄⠀⠀⣿⣿⣿⠀⠀⠀⠀⢀⣴⣿⣿⣿⡿⠋⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠈⢿⣿⣿⣷⡀⣿⣿⣿⠀⠀⣤⣾⣿⣿⣿⠛⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠈⢿⣿⣿⣿⣿⣿⣾⣿⣿⣿⠟⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⢿⣿⣿⣿⣿⠟⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠿⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
"

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

columns="$(tput cols)"
for line in $logo; do
  printf "%*s\n" $(( (31 + columns) / 2)) "$line"
done

if [ ! "$(uname)" == "Darwin" ]; then
  echo "Installer script meant for OSX"
  echo "Please visit https://tari.com/downloads/"
  echo " and download the binary distro for your platform"
  exit 1
fi

DATA_DIR=${1:-"$HOME/.tari"}
NETWORK=stibbons

banner Installing and setting up your Minotari Base Node
if [ ! -d "$DATA_DIR/$NETWORK" ]; then
  echo "Creating Tari data folder in $DATA_DIR"
  mkdir -p $DATA_DIR/$NETWORK
fi

if [ ! -f "$DATA_DIR/config.toml" ]; then
  echo "Copying configuraton files"
  cp tari_config_example.toml $DATA_DIR/config.toml
  cp log4rs_sample_base_node.yml $DATA_DIR/log4rs_base_node.yml
  echo "Configuration complete."
fi

./install_tor.sh no-run
# Start Tor
osascript -e "tell application \"Terminal\" to do script \"sh ${PWD}/start_tor.sh\""
echo "Waiting for Tor to start..."
sleep 20
echo "Ok"

# Make Base Node exec
if [ -f ./minotari_node ]; then
  chmod +x ./minotari_node
fi

# Configure Base Node
./minotari_node --init

banner Running Tari Base Node
# Run Base Node
./minotari_node
