#!/bin/bash
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
rm -rf temp/data/integration-test-base-node
mkdir -p  temp/data/integration-test-base-node
source $DIR/environment
cd temp/data/integration-test-base-node
cargo run --release --bin tari_base_node -- --base-path . --create-id --init
cargo run --release --bin tari_base_node -- --base-path .
