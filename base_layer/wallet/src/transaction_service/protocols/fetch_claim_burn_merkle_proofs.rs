// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::{burn_proof::EncodedMerkleProof, types::FixedHash};
use tari_utilities::ByteArray;
use tari_transaction_key_manager::legacy_key_manager::LegacyTransactionKeyManagerInterface;
use crate::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::storage::database::{TransactionBackend, TransactionDatabase},
};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::sync_claim_burn_merkle_proofs";

pub async fn execute<TBackend, KM, TConnectivity>(
    db: TransactionDatabase<TBackend>,
    output_manager: OutputManagerHandle<KM>,
    connectivity: TConnectivity,
    confirmed_burns: Vec<FixedHash>,
) where
    TBackend: TransactionBackend + 'static,
    KM: LegacyTransactionKeyManagerInterface,
    TConnectivity: WalletConnectivityInterface,
{
    debug!(
        target: LOG_TARGET,
        "Starting sync_claim_burn_merkle_proofs with {} confirmed burns",
        confirmed_burns.len()
    );
    if let Err(err) = execute_inner(db, output_manager, connectivity, confirmed_burns).await {
        // TODO: not very robust, some burnt outputs may never be updated (save it for the rewrite ;).
        error!(target: LOG_TARGET, "Error in sync_claim_burn_merkle_proofs: {}", err);
    }
}

async fn execute_inner<TBackend, KM, TConnectivity>(
    db: TransactionDatabase<TBackend>,
    mut output_manager: OutputManagerHandle<KM>,
    connectivity: TConnectivity,
    confirmed_burns: Vec<FixedHash>,
) -> anyhow::Result<()>
where
    TBackend: TransactionBackend + 'static,
    KM: LegacyTransactionKeyManagerInterface,
    TConnectivity: WalletConnectivityInterface,
{
    let timer = std::time::Instant::now();
    debug!(
        target: LOG_TARGET,
        "Syncing claim burn merkle proofs for {} confirmed burns. Waiting for client connection...",
        confirmed_burns.len()
    );
    let client = connectivity.obtain_base_node_wallet_rpc_client().await;
    debug!(
        target: LOG_TARGET,
        "Connected to base node wallet client after {:.2?}",
        timer.elapsed()
    );

    let num_expected = confirmed_burns.len();
    let outputs = output_manager.get_many_outputs(confirmed_burns).await?;
    if outputs.len() != num_expected {
        warn!(
            target: LOG_TARGET,
            "Some requested outputs were not found. Requested {}, but only found {} in the database",
            num_expected,
            outputs.len()
        );
    }

    for output in outputs {
        if !output.features().output_type.is_burn() {
            warn!(
                target: LOG_TARGET,
                "NEVER HAPPEN: Output with key id {} is not a burn output, skipping",
                output.commitment_mask_key_id()
            );
            continue;
        }

        let output_hash = output.output_hash();
        let Some(burn) = db.fetch_burn_proof(&output_hash)? else {
            // OK - UTXO not burnt with claim key, so no burn proof
            debug!(
                target: LOG_TARGET,
                "Burn output {} not found in database, skipping",
                output_hash
            );
            continue;
        };
        if burn.kernel_merkle_proof.is_some() {
            // If we're getting the same output again, there could be a reorg so just to be safe we'll refetch.
            warn!(
                target: LOG_TARGET,
                "Burn output {} already has a merkle proof, refetching",
                output_hash
            );
        }

        debug!(
            target: LOG_TARGET,
            "Updating burn output {} with merkle proof",
            output_hash
        );

        let result = client
            .get_kernel_merkle_proof(
                burn.kernel.excess_sig.get_compressed_public_nonce().as_bytes(),
                burn.kernel.excess_sig.get_signature().as_bytes(),
            )
            .await;
        match result {
            Ok(resp) => {
                info!(
                    target: LOG_TARGET,
                    "Received merkle proof for burn output {}: {} bytes",
                    output_hash,
                    resp.encoded_merkle_proof.len()
                );

                // We trust our base node right? ;-) So just store it without validating.

                db.update_burn_proof_set_merkle_proof(&output_hash, &EncodedMerkleProof {
                    block_hash: resp.block_hash,
                    encoded_merkle_proof: resp.encoded_merkle_proof,
                    leaf_index: resp.leaf_index,
                })?;
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "Failed to get merkle proof for burn output {}: {}",
                    output_hash,
                    err
                );
                continue;
            },
        }
    }

    debug!(
        target: LOG_TARGET,
        "Syncing claim burn merkle proofs completed in {:.2?}",
        timer.elapsed()
    );

    Ok(())
}
