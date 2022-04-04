// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::time::Instant;

use log::*;
use rand::rngs::OsRng;
use tari_common_types::{
    transaction::TxId,
    types::{BulletRangeProof, PrivateKey, PublicKey},
};
use tari_core::transactions::{
    transaction_components::{TransactionOutput, UnblindedOutput},
    transaction_protocol::RewindData,
    CryptoFactories,
};
use tari_crypto::{
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    tari_utilities::hex::Hex,
};
use tari_script::{inputs, script};

use crate::{
    key_manager_service::KeyManagerInterface,
    output_manager_service::{
        error::{OutputManagerError, OutputManagerStorageError},
        handle::RecoveredOutput,
        resources::OutputManagerKeyManagerBranch,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::DbUnblindedOutput,
        },
    },
};

const LOG_TARGET: &str = "wallet::output_manager_service::recovery";

pub(crate) struct StandardUtxoRecoverer<TBackend: OutputManagerBackend + 'static, TKeyManagerInterface> {
    master_key_manager: TKeyManagerInterface,
    rewind_data: RewindData,
    factories: CryptoFactories,
    db: OutputManagerDatabase<TBackend>,
}

impl<TBackend, TKeyManagerInterface> StandardUtxoRecoverer<TBackend, TKeyManagerInterface>
where
    TBackend: OutputManagerBackend + 'static,
    TKeyManagerInterface: KeyManagerInterface,
{
    pub fn new(
        master_key_manager: TKeyManagerInterface,
        rewind_data: RewindData,
        factories: CryptoFactories,
        db: OutputManagerDatabase<TBackend>,
    ) -> Self {
        Self {
            master_key_manager,
            rewind_data,
            factories,
            db,
        }
    }

    /// Attempt to rewind all of the given transaction outputs into unblinded outputs. If they can be rewound then add
    /// them to the database and increment the key manager index
    pub async fn scan_and_recover_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let start = Instant::now();
        let outputs_length = outputs.len();
        let mut rewound_outputs: Vec<(UnblindedOutput, BulletRangeProof)> = outputs
            .into_iter()
            .filter_map(|output| {
                output
                    .full_rewind_range_proof(
                        &self.factories.range_proof,
                        &self.rewind_data.rewind_key,
                        &self.rewind_data.rewind_blinding_key,
                    )
                    .ok()
                    .map(|v| (v, output))
            })
            .filter_map(|(rewind_result, output)| {
                if output.script != script!(Nop) {
                    return None;
                }
                let script_key = PrivateKey::random(&mut OsRng);
                Some((
                    UnblindedOutput::new(
                        output.version,
                        rewind_result.committed_value,
                        rewind_result.blinding_factor,
                        output.features,
                        output.script,
                        inputs!(PublicKey::from_secret_key(&script_key)),
                        script_key,
                        output.sender_offset_public_key,
                        output.metadata_signature,
                        0,
                        output.covenant,
                    ),
                    output.proof,
                ))
            })
            .collect();
        let rewind_time = start.elapsed();
        trace!(
            target: LOG_TARGET,
            "bulletproof rewind profile - rewound {} outputs in {} ms",
            outputs_length,
            rewind_time.as_millis(),
        );

        let mut rewound_outputs_with_tx_id: Vec<RecoveredOutput> = Vec::new();
        for (output, proof) in rewound_outputs.iter_mut() {
            let db_output = DbUnblindedOutput::rewindable_from_unblinded_output(
                output.clone(),
                &self.factories,
                &self.rewind_data,
                None,
                Some(proof),
            )?;
            let tx_id = TxId::new_random();
            let output_hex = db_output.commitment.to_hex();
            if let Err(e) = self.db.add_unspent_output_with_tx_id(tx_id, db_output) {
                match e {
                    OutputManagerStorageError::DuplicateOutput => {
                        info!(
                            target: LOG_TARGET,
                            "Recoverer attempted to import a duplicate output (Commitment: {})", output_hex
                        );
                        continue;
                    },
                    _ => return Err(OutputManagerError::from(e)),
                }
            }

            rewound_outputs_with_tx_id.push(RecoveredOutput {
                output: output.clone(),
                tx_id,
            });
            self.update_outputs_script_private_key_and_update_key_manager_index(output)
                .await?;
            trace!(
                target: LOG_TARGET,
                "Output {} with value {} with {} recovered",
                output_hex,
                output.value,
                output.features,
            );
        }

        Ok(rewound_outputs_with_tx_id)
    }

    /// Find the key manager index that corresponds to the spending key in the rewound output, if found then modify
    /// output to contain correct associated script private key and update the key manager to the highest index it has
    /// seen so far.
    async fn update_outputs_script_private_key_and_update_key_manager_index(
        &mut self,
        output: &mut UnblindedOutput,
    ) -> Result<(), OutputManagerError> {
        let script_key = if output.features.is_coinbase() {
            let found_index = self
                .master_key_manager
                .find_key_index(
                    OutputManagerKeyManagerBranch::Coinbase.get_branch_key(),
                    &output.spending_key,
                )
                .await?;

            self.master_key_manager
                .get_key_at_index(
                    OutputManagerKeyManagerBranch::CoinbaseScript.get_branch_key(),
                    found_index,
                )
                .await?
        } else {
            let found_index = self
                .master_key_manager
                .find_key_index(
                    OutputManagerKeyManagerBranch::Spend.get_branch_key(),
                    &output.spending_key,
                )
                .await?;

            self.master_key_manager
                .update_current_key_index_if_higher(OutputManagerKeyManagerBranch::Spend.get_branch_key(), found_index)
                .await?;
            self.master_key_manager
                .update_current_key_index_if_higher(
                    OutputManagerKeyManagerBranch::SpendScript.get_branch_key(),
                    found_index,
                )
                .await?;

            self.master_key_manager
                .get_key_at_index(OutputManagerKeyManagerBranch::SpendScript.get_branch_key(), found_index)
                .await?
        };

        output.input_data = inputs!(PublicKey::from_secret_key(&script_key));
        output.script_private_key = script_key;
        Ok(())
    }
}
