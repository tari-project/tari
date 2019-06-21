#!/usr/bin/env bash
packages=${@:-'infrastructure/tari_util infrastructure/derive infrastructure/crypto infrastructure/merklemountainrange base_layer/core'}
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
