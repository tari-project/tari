#!/bin/bash
#
# Docker Start Script for Tari applications
# The docker compose environment should set the following envars
# - APP_NAME - the name of the app to run. This var is used to set the location of log files, and app-specific config
# - APP_EXEC - the name of the application executable. Just the name is enough, since the Dockerfile will put it in /usr/bin
# - CREATE_CONFIG - set to 1 if we should write a default config file if one is missing.
# - CREATE_ID - set to 1 if we should create an id file for this application if one is missing. It will be called
#               {network}_{app_name}_id.json
# - WAIT_FOR_TOR - set to 1 to place a 30 second delay at the beginning of this script.
# - TARI_NETWORK - the Tari network to configure the docker rig for
#

# Export these variables to the environment
START_TOR=${START_TOR:-1}
START_BASE_NODE=${START_BASE_NODE:-1}
START_WALLET=${START_WALLET:-1}
START_MINER=${START_MINER:-1}
USE_OWN_MODEROD=${USE_OWN_MODEROD:-0}
START_MONERO_MM=${START_MONERO_MM:-1}

CREATE_CONFIG=${CREATE_CONFIG:-0}
CREATE_ID=${CREATE_ID:-0}
NETWORK=${TARI_NETWORK:-dibbler}
CONFIG=$DATA_FOLDER/config

check_data_folder() {
  if [[ ! -d "$DATA_FOLDER" ]]; then
    echo "Creating data folder $DATA_FOLDER.."
    mkdir -p "$DATA_FOLDER/config"
    mkdir -p "$DATA_FOLDER/base_node"
    mkdir -p "$DATA_FOLDER/tor"
    mkdir -p "$DATA_FOLDER/xmrig"
    mkdir -p "$DATA_FOLDER/monerod"
    mkdir -p "$DATA_FOLDER/mm_proxy"
    cp log4rs.yml config.toml "$DATA_FOLDER/config/"
    SETUP=1
    echo "Done."
  else
    echo "Using existing data folder $DATA_FOLDER"
  fi
}

check_data_folder

echo "network: $NETWORK"
echo "Setup: $SETUP"

export DATA_FOLDER=$DATA_FOLDER
export WAIT_FOR_TOR=$WAIT_FOR_TOR
export TARI_NETWORK=$NETWORK

if [[ $SETUP == 1 ]]; then
  echo "Creating identity files and default config file"
  docker compose run --rm base_node --init
fi

if [[ $START_TOR == 1 ]]; then
  docker compose up -d tor
  WAIT_FOR_TOR=10
fi

if [[ $START_BASE_NODE == 1 ]]; then
  echo "Starting Base Node"
  export WAIT_FOR_TOR=$WAIT_FOR_TOR
  docker compose up -d base_node
fi

if [[ $START_WALLET == 1 ]]; then
  echo "Starting Wallet"
  export WAIT_FOR_TOR=0
  docker compose up -d wallet
fi

if [[ $START_MINER == 1 ]]; then
  echo "Starting SHA3 Miner"
  export WAIT_FOR_TOR=0
  docker compose up -d sha3_miner
fi

if [[ $USE_OWN_MONEROD == 1 ]]; then
  echo "Local MoneroD is implemented yet!"
  exit 1
fi

if [[ $START_MONERO_MM == 1 ]]; then
  echo "Starting Monero Merge miner"
  export WAIT_FOR_TOR=0
  docker compose up -d mm_proxy xmrig
fi
