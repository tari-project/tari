#!/usr/bin/env bash
packages=${@:-'
infrastructure/crypto
infrastructure/derive
infrastructure/storage
infrastructure/tari_util
base_layer/core
base_layer/keymanager
base_layer/mining
base_layer/mmr
base_layer/p2p
base_layer/service_framework
base_layer/wallet
common
comms
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
cargo login
build_package ${p_arr[@]}
