#!/usr/bin/env bash
set -e

# The order is important. Dependencies must be published before the crates that depend on them.
PACKAGES=(
    "tari_storage"
    "tari_shutdown"
    "tari_metrics"
    "tari_max_size"
    "tari_script"
    "tari_hashing"
    "tari_jellyfish"
    "tari_comms_rpc_macros"
    "tari_common_sqlite"
    "tari_features"
    "tari_test_utils"
    "tari_common"
    "tari_comms"
    "tari_comms_dht"
    "minotari_ledger_wallet_common"
    "tari_common_types"
    "tari_sidechain"
    "minotari_ledger_wallet_comms"
    "tari_service_framework"
    "tari_transaction_components"
    "tari_transaction_key_manager"
    "tari_node_components"
    "tari_p2p"
    "tari_libtor"
    "tari_mmr"
    "tari_core"
    "minotari_node_wallet_client"
    "minotari_wallet"
    "minotari_app_grpc"
    "minotari_app_utilities"
    "minotari_wallet_grpc_client"
    "minotari_node_grpc_client"
)

for package in "${PACKAGES[@]}"; do
    echo "Dry-run Publishing ${package}..."
    cargo publish --package "${package}" --dry-run
    echo "Publishing ${package}..."
    cargo publish --package "${package}"
done

echo "All packages published successfully."