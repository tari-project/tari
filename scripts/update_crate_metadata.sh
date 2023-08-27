#!/usr/bin/env bash

VERSION=$1
if [ "x$VERSION" == "x" ]; then
  echo "USAGE: update_crate_metadata version"
  exit 1
fi

#   infrastructure/derive
#   infrastructure/shutdown
#   infrastructure/storage
#   infrastructure/test_utils
#   base_layer/core
#   base_layer/key_manager
#   base_layer/mmr
#   base_layer/p2p
#   base_layer/service_framework
#   base_layer/wallet
#   base_layer/wallet_ffi
#   common
#   comms
#   comms/dht
#   applications/minotaiji_node
#   applications/minotaiji_app_grpc
#   applications/minotaiji_app_utilities
#   applications/minotaiji_console_wallet
#   applications/minotaiji_merge_mining_proxy
#   applications/test_faucet

function update_versions {
    packages=${@:-'
      infrastructure/derive
   infrastructure/shutdown
   infrastructure/storage
   infrastructure/test_utils
   base_layer/common_types
   base_layer/contacts
   base_layer/core
   base_layer/key_manager
   base_layer/mmr
   base_layer/p2p
   base_layer/service_framework
   base_layer/wallet
   base_layer/wallet_ffi
   base_layer/minotaiji_mining_helper_ffi
   common
   common_sqlite
   common/taiji_features
   comms/core
   comms/dht
   comms/rpc_macros
   applications/minotaiji_node
   applications/minotaiji_app_grpc
   applications/minotaiji_app_utilities
   applications/minotaiji_console_wallet
   applications/minotaiji_merge_mining_proxy
   applications/minotaiji_miner
   applications/taiji_validator_node
   applications/test_faucet
'}

  p_arr=($packages)
    for p in "${p_arr[@]}"; do
      echo "Updating $path/$p version"
      update_version ./${p}/Cargo.toml $VERSION
    done
}

function update_version {
    CARGO=$1
    VERSION=$2
    #SCRIPT='s/version\s*=\s*"\d+\.\d+\.\d+"/version = "'"$VERSION"'"/'
    SCRIPT='s/^version = "0.*$/version = "'"$VERSION"'"/'
    echo "$SCRIPT" "$CARGO"
    sed -i.bak -e "$SCRIPT" "$CARGO"
    rm $CARGO.bak
}



update_versions ${p_arr[@]}
