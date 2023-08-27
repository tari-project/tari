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
minotaiji_app_grpc
minotaiji_app_utilities
minotaiji_node
minotaiji_node_grpc_client
taiji_chat_client
taiji_chat_ffi
minotaiji_console_wallet
taiji_contacts
taiji_features
taiji_integration_tests
taiji_libtor
minotaiji_merge_mining_proxy
taiji_metrics
minotaiji_miner
minotaiji_mining_helper_ffi
taiji_test_utils
minotaiji_wallet_ffi
minotaiji_wallet_grpc_client
)

# Included:
# taiji_common
# taiji_comms
# taiji_core
# taiji_common_sqlite
# taiji_common_types
# taiji_comms
# taiji_comms_dht
# taiji_comms_rpc_macros
# taiji_p2p
# taiji_service_framework
# taiji_storage
# taiji_wallet

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

