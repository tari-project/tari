#!/usr/bin/env bash
# NB: The order these are listed in is IMPORTANT! Dependencies must go first
packages=${@:-'
   infrastructure/broadcast_channel
   infrastructure/derive
   infrastructure/protobuf_build
   infrastructure/pubsub
   infrastructure/shutdown
   infrastructure/storage
common
comms
base_layer/transactions
base_layer/p2p
base_layer/core
base_layer/keymanager
base_layer/mining
base_layer/mmr
base_layer/service_framework
base_layer/wallet
base_layer/wallet_ffi
'}
p_arr=($packages)

function build_package {
    list=($@)
    for p in "${list[@]}"; do
      echo "************************  Building $path/$p package ************************"
      cargo publish --manifest-path=./${p}/Cargo.toml
    done
    echo "************************  $path packages built ************************"

}

# You need a token with write access to publish these crates
#cargo login
build_package ${p_arr[@]}
