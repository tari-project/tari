#!/bin/bash

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
source $DIR/environment
PORT=$1
GRPCPORT=$2
cd temp/base_nodes/Basenode$PORT
#cargo run --release --bin tari_base_node -- --base-path . --create-id --init
cargo run --release --bin tari_base_node -- --base-path .
