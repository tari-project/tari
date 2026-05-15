#!/usr/bin/env bash
set -e

# The order is important. Dependencies must be published before the crates that depend on them.
PACKAGES=(

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