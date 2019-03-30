#!/usr/bin/env bash
infrastructure_packages=('tari_util' 'derive' 'crypto' 'merklemountainrange')
base_layer_packages=('core')

function build_package {
    path=$1
    shift
    list=($@)
    for p in "${list[@]}"; do
      echo "************************  Building $path/$p package ************************"
      cargo publish --manifest-path=${path}/${p}/Cargo.toml
    done
    echo "************************  $path packages built ************************"

}

# You need a token with write access to publish these crates
cargo login
build_package "infrastructure" ${infrastructure_packages[@]}
build_package "base_layer" ${base_layer_packages[@]}
