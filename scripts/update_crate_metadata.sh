#!/usr/bin/env bash

VERSION=$1
if [ "x$VERSION" == "x" ]; then
  echo "USAGE: update_crate_metadata version"
  exit 1
fi

function update_versions {
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
    for p in "${p_arr[@]}"; do
      echo "Updating $path/$p version"
      update_version ./${p}/Cargo.toml $VERSION
    done
}

function update_version {
    CARGO=$1
    VERSION=$2
    SCRIPT='s/^version = "[0-9]\.[0-9]\.[0-9]"$/version = "'"$VERSION"'"/g'
    sed -i.bak -e "$SCRIPT" "$CARGO"
    rm $CARGO.bak
}



update_versions ${p_arr[@]}
