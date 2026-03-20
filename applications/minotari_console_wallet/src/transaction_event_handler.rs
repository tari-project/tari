//  Copyright 2026. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::path::Path;

use anyhow::anyhow;
use log::*;
use minotari_wallet::{
    WalletSqlite,
    storage::sqlite_db::models::DbBurnProof,
    transaction_service::{
        config::TransactionServiceConfig,
        handle::{TransactionEvent, TransactionServiceHandle},
    },
};
use tari_common::configuration::Network;
use tari_common_types::types::HashOutput;
use tari_sidechain::{AbridgedTransactionKernel, BurnClaimProof, CompleteClaimBurnProof};
use tari_utilities::hex::Hex;
use tokio::fs;

const LOG_TARGET: &str = "wallet::app::transaction_event_handler";

pub fn start(wallet: &WalletSqlite) -> impl Future<Output = ()> + 'static {
    let network = wallet.network.as_network();
    let transaction_service = wallet.transaction_service.clone();
    let config = wallet.config.transaction_service_config.clone();
    TransactionEventHandler {
        transaction_service,
        config,
        network,
    }
    .run()
}

struct TransactionEventHandler {
    config: TransactionServiceConfig,
    network: Network,
    transaction_service: TransactionServiceHandle,
}

impl TransactionEventHandler {
    pub(self) async fn run(mut self) {
        let mut events = self.transaction_service.get_event_stream();
        while let Ok(event) = events.recv().await {
            #[allow(clippy::single_match)]
            match &*event {
                TransactionEvent::TransactionBurnConfirmed { output_hash, .. } => {
                    if let Err(err) = self.handle_burn_confirmed(*output_hash).await {
                        error!(target: LOG_TARGET, "Error handling TransactionBurnConfirmed event: {}", err);
                    }
                },
                _ => {},
            }
        }
    }

    async fn handle_burn_confirmed(&mut self, output_hash: HashOutput) -> anyhow::Result<()> {
        let proof = self
            .transaction_service
            .get_burn_proof(output_hash)
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "Burn proof not found in database for output {} after TransactionBurnConfirmed event",
                    output_hash
                )
            })?;

        let out_dir = self.config.get_burn_proof_output_dir(self.network);

        write_burn_proof_to_file(out_dir, proof).await?;

        Ok(())
    }
}

async fn write_burn_proof_to_file<P: AsRef<Path>>(burn_proofs_dir: P, proof: DbBurnProof) -> anyhow::Result<()> {
    fs::create_dir_all(&burn_proofs_dir).await?;
    let kernel_merkle_proof = proof
        .kernel_merkle_proof
        .ok_or_else(|| anyhow!("No kernel_merkle_proof"))?;
    let encrypted_data = proof.encrypted_data.ok_or_else(|| anyhow!("No encrypted_data"))?;
    let value = proof.value.ok_or_else(|| anyhow!("No value"))?;

    let filename = format!(
        "{}-{}.json",
        proof.burn_proof.claim_public_key,
        proof.burn_proof.commitment.to_hex()
    );
    let final_path = burn_proofs_dir.as_ref().join(filename);

    let complete_proof = CompleteClaimBurnProof {
        claim_proof: BurnClaimProof {
            burn_public_key: proof.burn_proof.claim_public_key,
            commitment: proof.burn_proof.commitment,
            ownership_proof: proof.burn_proof.ownership_proof,
            encoded_merkle_proof: kernel_merkle_proof,
            kernel: AbridgedTransactionKernel {
                version: proof.kernel.version.as_u8(),
                fee: proof.kernel.fee.as_u64(),
                lock_height: proof.kernel.lock_height,
                excess: proof.kernel.excess,
                excess_sig: proof.kernel.excess_sig,
            },
            value: value.as_u64(),
            sender_offset_public_key: proof.burn_proof.sender_offset_public_key,
        },
        encrypted_data: encrypted_data.into_vec(),
    };

    fs::write(&final_path, serde_json::to_vec_pretty(&complete_proof)?).await?;
    info!(target: LOG_TARGET, "Wrote burn proof to {}", final_path.display());
    Ok(())
}
