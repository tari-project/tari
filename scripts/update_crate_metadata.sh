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
#   applications/minotari_node
#   applications/minotari_app_grpc
#   applications/minotari_app_utilities
#   applications/minotari_console_wallet
#   applications/minotari_merge_mining_proxy

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
   base_layer/minotari_mining_helper_ffi
   common
   common_sqlite
   common/tari_features
   comms/core
   comms/dht
   comms/rpc_macros
   applications/minotari_node
   applications/minotari_app_grpc
   applications/minotari_app_utilities
   applications/minotari_console_wallet
   applications/minotari_merge_mining_proxy
   applications/minotari_miner
   applications/tari_validator_node
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
