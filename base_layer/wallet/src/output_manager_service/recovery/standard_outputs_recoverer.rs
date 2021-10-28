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

use std::sync::Arc;

use log::*;
use tari_crypto::{inputs, keys::PublicKey as PublicKeyTrait, tari_utilities::hex::Hex};

use tari_common_types::types::PublicKey;
use tari_core::transactions::{
    transaction::{TransactionOutput, UnblindedOutput},
    CryptoFactories,
};

use crate::output_manager_service::{
    error::{OutputManagerError, OutputManagerStorageError},
    storage::{
        database::{OutputManagerBackend, OutputManagerDatabase},
        models::DbUnblindedOutput,
    },
    MasterKeyManager,
};
use tracing::instrument;

const LOG_TARGET: &str = "wallet::output_manager_service::recovery";

pub(crate) struct StandardUtxoRecoverer<TBackend: OutputManagerBackend + 'static> {
    master_key_manager: Arc<MasterKeyManager<TBackend>>,
    factories: CryptoFactories,
    db: OutputManagerDatabase<TBackend>,
}

impl<TBackend> StandardUtxoRecoverer<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    pub fn new(
        master_key_manager: Arc<MasterKeyManager<TBackend>>,
        factories: CryptoFactories,
        db: OutputManagerDatabase<TBackend>,
    ) -> Self {
        Self {
            master_key_manager,
            factories,
            db,
        }
    }

    /// Attempt to rewind all of the given transaction outputs into unblinded outputs. If they can be rewound then add
    /// them to the database and increment the key manager index
    #[instrument(name = "recovery::scan_and_recover_outputs", skip(self, outputs))]
    pub async fn scan_and_recover_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        let mut rewound_outputs: Vec<UnblindedOutput> = outputs
            .into_iter()
            .filter_map(|output| {
                output
                    .full_rewind_range_proof(
                        &self.factories.range_proof,
                        &self.master_key_manager.rewind_data().rewind_key,
                        &self.master_key_manager.rewind_data().rewind_blinding_key,
                    )
                    .ok()
                    .map(|v| {
                        (
                            v,
                            output.features,
                            output.script,
                            output.sender_offset_public_key,
                            output.metadata_signature,
                        )
                    })
            })
            .map(
                |(output, features, script, sender_offset_public_key, metadata_signature)| {
                    UnblindedOutput::new(
                        output.committed_value,
                        output.blinding_factor.clone(),
                        features,
                        script,
                        inputs!(PublicKey::from_secret_key(&output.blinding_factor)),
                        output.blinding_factor,
                        sender_offset_public_key,
                        metadata_signature,
                    )
                },
            )
            .collect();

        for output in rewound_outputs.iter_mut() {
            self.update_outputs_script_private_key_and_update_key_manager_index(output)
                .await?;

            let db_output = DbUnblindedOutput::from_unblinded_output(output.clone(), &self.factories)?;
            let output_hex = db_output.commitment.to_hex();
            if let Err(e) = self.db.add_unspent_output(db_output).await {
                match e {
                    OutputManagerStorageError::DuplicateOutput => {
                        info!(
                            target: LOG_TARGET,
                            "Recoverer attempted to import a duplicate output (Commitment: {})", output_hex
                        );
                    },
                    _ => return Err(OutputManagerError::from(e)),
                }
            }

            trace!(
                target: LOG_TARGET,
                "Output {} with value {} with {} recovered",
                output
                    .as_transaction_input(&self.factories.commitment)?
                    .commitment
                    .to_hex(),
                output.value,
                output.features,
            );
        }

        Ok(rewound_outputs)
    }

    /// Find the key manager index that corresponds to the spending key in the rewound output, if found then modify
    /// output to contain correct associated script private key and update the key manager to the highest index it has
    /// seen so far.
    #[instrument(
        name = "recovery::update_outputs_script_private_key_and_update_key_manager_index",
        skip(self, output)
    )]
    async fn update_outputs_script_private_key_and_update_key_manager_index(
        &mut self,
        output: &mut UnblindedOutput,
    ) -> Result<(), OutputManagerError> {
        let found_index = self
            .master_key_manager
            .find_utxo_key_index(output.spending_key.clone())
            .await?;

        self.master_key_manager
            .update_current_index_if_higher(found_index)
            .await?;

        let script_private_key = self.master_key_manager.get_script_key_at_index(found_index).await?;
        output.input_data = inputs!(PublicKey::from_secret_key(&script_private_key));
        output.script_private_key = script_private_key;
        Ok(())
    }
}
