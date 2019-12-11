#!/usr/bin/env bash

set -e

echo "Synching all submodules"
git submodule update --recursive --remote

SUBMODULES=(
  "comms/yamux"
  "comms/rust-multiaddr"
)
GIT_URLS=(
  "git@github.com:tari-project/yamux.git"
  "git@github.com:tari-project/rust-multiaddr.git"
)

# Change all submodule urls to use ssh
for index in "${!SUBMODULES[@]}"; do
  pushd "${SUBMODULES[$index]}" > /dev/null
  git remote set-url origin "${GIT_URLS[$index]}"
  popd > /dev/null
done
