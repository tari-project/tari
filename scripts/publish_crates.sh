#!/usr/bin/env bash
# NB: The order these are listed in is IMPORTANT! Dependencies must go first

#infrastructure/derive
#infrastructure/shutdown
#infrastructure/storage
#infrastructure/test_utils
#common
#comms
#comms/rpc_macros
#comms/dht
#base_layer/service_framework
#base_layer/mmr
#base_layer/key_manager
#base_layer/p2p
#base_layer/tari_common_types
#base_layer/core
#base_layer/wallet

packages=${@:-'
infrastructure/derive
infrastructure/shutdown
infrastructure/storage
infrastructure/test_utils
common
comms
comms/rpc_macros
comms/dht
base_layer/service_framework
base_layer/mmr
base_layer/key_manager
base_layer/p2p
base_layer/tari_common_types
base_layer/core
base_layer/wallet
'}
p_arr=($packages)

function build_package {
    list=($@)
    for p in "${list[@]}"; do
      echo "************************  Building $path/$p package ************************"
      cargo publish --manifest-path=./${p}/Cargo.toml
      sleep 30 # Wait for crates.io to register any dependent packages
    done
    echo "************************  $path packages built ************************"
}

# You need a token with write access to publish these crates
#cargo login
build_package ${p_arr[@]}
