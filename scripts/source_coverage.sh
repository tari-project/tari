#!/bin/bash

# When running in GHA, use lcov format
if [[ "$CI" == "true" ]]; then
  output_opts="--lcov --output-path lcov.info"
else
  export LLVM_COV_FLAGS="-coverage-watermark=90,66"
  output_opts="--html"
fi

ignored_crates=(
deps_only
minotari_app_grpc
minotari_app_utilities
minotari_base_node
minotari_base_node_grpc_client
tari_chat_client
tari_chat_ffi
minotari_console_wallet
tari_contacts
tari_features
tari_integration_tests
tari_libtor
minotari_merge_mining_proxy
tari_metrics
minotari_miner
minotari_mining_helper_ffi
tari_test_utils
minotari_wallet_ffi
minotari_wallet_grpc_client
)

# Included:
# tari_common
# tari_comms
# tari_core
# tari_common_sqlite
# tari_common_types
# tari_comms
# tari_comms_dht
# tari_comms_rpc_macros
# tari_p2p
# tari_service_framework
# tari_storage
# tari_wallet

echo "Check for cargo-llvm-cov"
if [ "$(cargo llvm-cov --version)" ]
then
    echo "    + Already installed"
else
    echo "    + Installing.."
    cargo install cargo-llvm-cov
fi

echo "Source coverage environment parameters:"
echo $(cargo llvm-cov show-env)
echo "Output parameters: $output_opts"

echo "Deleting old coverage files"
cargo llvm-cov clean --workspace

echo "Starting code coverage run"
cargo llvm-cov test \
  --all-features \
  --verbose \
  --workspace \
  --ignore-run-fail \
  --color auto \
  ${output_opts} \
  ${ignored_crates[@]/#/--exclude } \

