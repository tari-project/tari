// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{sync::Arc, time::Duration};

use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::{burn_proof::EncodedMerkleProof, types::FixedHash};
use tari_utilities::ByteArray;
use tokio::sync::broadcast;

use crate::{
    connectivity_service::WalletConnectivityInterface,
    transaction_service::{
        handle::TransactionEvent,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
};

const RETRY_DELAY: Duration = Duration::from_secs(10);
const MAX_ATTEMPTS: usize = 10;
const LOG_TARGET: &str = "wallet::transaction_service::protocols::sync_claim_burn_merkle_proofs";

pub async fn execute<TBackend, TConnectivity>(
    db: TransactionDatabase<TBackend>,
    connectivity: TConnectivity,
    event_publisher: broadcast::Sender<Arc<TransactionEvent>>,
    confirmed_burns: Vec<FixedHash>,
) where
    TBackend: TransactionBackend + 'static,
    TConnectivity: WalletConnectivityInterface,
{
    debug!(
        target: LOG_TARGET,
        "Starting sync_claim_burn_merkle_proofs with {} confirmed burns",
        confirmed_burns.len()
    );
    let mut attempt = 0usize;
    loop {
        if let Err(err) = execute_inner(&db, &connectivity, &confirmed_burns, &event_publisher).await {
            // TODO: not very robust, some burnt outputs may never be updated (save it for the rewrite ;).
            error!(target: LOG_TARGET, "Error in sync_claim_burn_merkle_proofs: {}", err);
            tokio::time::sleep(RETRY_DELAY).await;
            attempt += 1;
            if attempt > MAX_ATTEMPTS {
                error!(
                    target: LOG_TARGET,
                    "sync_claim_burn_merkle_proofs failed after {} attempts, giving up",
                    attempt
                );
                break;
            }
            continue;
        }
        break;
    }
}

async fn execute_inner<TBackend, TConnectivity>(
    db: &TransactionDatabase<TBackend>,
    connectivity: &TConnectivity,
    confirmed_burns: &[FixedHash],
    event_publisher: &broadcast::Sender<Arc<TransactionEvent>>,
) -> anyhow::Result<()>
where
    TBackend: TransactionBackend + 'static,
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

    for output_hash in confirmed_burns {
        let Some(burn) = db.fetch_burn_proof(output_hash)? else {
            warn!(
                target: LOG_TARGET,
                "No burn proof found in database for output {}, skipping",
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

                let mined_in_height = resp.block_height;
                db.update_burn_proof_set_merkle_proof(
                    output_hash,
                    &EncodedMerkleProof {
                        block_hash: resp.block_hash,
                        encoded_merkle_proof: resp.encoded_merkle_proof,
                        leaf_index: resp.leaf_index,
                    },
                    mined_in_height,
                )?;

                info!(
                    target: LOG_TARGET,
                    "Successfully updated burn output {} with merkle proof (mined in height {:?})",
                    output_hash,
                    mined_in_height
                );

                let _ignore = event_publisher.send(Arc::new(TransactionEvent::TransactionBurnConfirmed {
                    output_hash: *output_hash,
                    commitment: Box::new(burn.burn_proof.commitment),
                }));
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
