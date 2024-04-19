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
use tari_common_types::{transaction::TxId, types::FixedHash};
use tari_core::transactions::{
    key_manager::{TariKeyId, TransactionKeyManagerBranch, TransactionKeyManagerInterface},
    tari_amount::MicroMinotari,
    transaction_components::{OutputType, TransactionError, TransactionOutput, WalletOutput},
};
use tari_script::{inputs, script, ExecutionStack, Opcode, TariScript};
use tari_utilities::hex::Hex;
use tari_key_manager::key_manager_service::KeyId;

use crate::output_manager_service::{
    error::{OutputManagerError, OutputManagerStorageError},
    handle::RecoveredOutput,
    storage::{
        database::{OutputManagerBackend, OutputManagerDatabase},
        models::{DbWalletOutput, KnownOneSidedPaymentScript},
        OutputSource,
    },
};

const LOG_TARGET: &str = "wallet::output_manager_service::recovery";

pub(crate) struct StandardUtxoRecoverer<TBackend: OutputManagerBackend + 'static, TKeyManagerInterface> {
    master_key_manager: TKeyManagerInterface,
    db: OutputManagerDatabase<TBackend>,
}

impl<TBackend, TKeyManagerInterface> StandardUtxoRecoverer<TBackend, TKeyManagerInterface>
where
    TBackend: OutputManagerBackend + 'static,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub fn new(master_key_manager: TKeyManagerInterface, db: OutputManagerDatabase<TBackend>) -> Self {
        Self { master_key_manager, db }
    }

    /// Attempt to rewind all of the given transaction outputs into key_manager outputs. If they can be rewound then add
    /// them to the database and increment the key manager index
    pub async fn scan_and_recover_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let start = Instant::now();
        let outputs_length = outputs.len();

        let known_scripts = self.db.get_all_known_one_sided_payment_scripts()?;

        let mut rewound_outputs: Vec<(WalletOutput, bool, FixedHash)> = Vec::new();
        let push_pub_key_script = script!(PushPubKey(Box::default()));
        for output in outputs {
            let known_script_index = known_scripts.iter().position(|s| s.script == output.script);
            if output.script != script!(Nop) &&
                known_script_index.is_none() &&
                !output.script.pattern_match(&push_pub_key_script)
            {
                continue;
            }

            let (spending_key, committed_value) = match self.attempt_output_recovery(&output).await? {
                Some(recovered) => recovered,
                None => continue,
            };
            let (input_data, script_key) = match self
                .find_script_key(&output.script, &spending_key, known_script_index, &known_scripts)
                .await?
            {
                Some((input_data, script_key)) => (input_data, script_key),
                None => continue,
            };

            let hash = output.hash();
            let uo = WalletOutput::new_with_rangeproof(
                output.version,
                committed_value,
                spending_key,
                output.features,
                output.script,
                input_data,
                script_key,
                output.sender_offset_public_key,
                output.metadata_signature,
                0,
                output.covenant,
                output.encrypted_data,
                output.minimum_value_promise,
                output.proof.clone(),
            );

            rewound_outputs.push((uo, known_script_index.is_some(), hash));
        }

        let rewind_time = start.elapsed();
        trace!(
            target: LOG_TARGET,
            "bulletproof rewind profile - rewound {} outputs in {} ms",
            outputs_length,
            rewind_time.as_millis(),
        );

        let mut rewound_outputs_with_tx_id: Vec<RecoveredOutput> = Vec::new();
        for (output, has_known_script, hash) in &mut rewound_outputs {
            let db_output = DbWalletOutput::from_wallet_output(
                output.clone(),
                &self.master_key_manager,
                None,
                Self::output_source(output, *has_known_script),
                None,
                None,
            )
            .await?;
            let tx_id = TxId::new_random();
            let output_hex = db_output.commitment.to_hex();
            if let Err(e) = self.db.add_unspent_output_with_tx_id(tx_id, db_output) {
                match e {
                    OutputManagerStorageError::DuplicateOutput => {
                        continue;
                    },
                    _ => return Err(OutputManagerError::from(e)),
                }
            }

            rewound_outputs_with_tx_id.push(RecoveredOutput {
                output: output.clone(),
                tx_id,
                hash: *hash,
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

    // Helper function to get the output source for a given output
    fn output_source(output: &WalletOutput, has_known_script: bool) -> OutputSource {
        match output.features.output_type {
            OutputType::Standard => match *output.script.as_slice() {
                [Opcode::Nop] => OutputSource::Standard,
                [Opcode::PushPubKey(_), Opcode::Drop, Opcode::PushPubKey(_)] => OutputSource::StealthOneSided,
                [Opcode::PushPubKey(_)] => {
                    if has_known_script {
                        OutputSource::OneSided
                    } else {
                        OutputSource::Standard
                    }
                },
                _ => OutputSource::NonStandardScript,
            },
            OutputType::Coinbase => OutputSource::Coinbase,
            OutputType::Burn => OutputSource::Burn,
            OutputType::ValidatorNodeRegistration => OutputSource::ValidatorNodeRegistration,
            OutputType::CodeTemplateRegistration => OutputSource::CodeTemplateRegistration,
        }
    }

    async fn find_script_key(
        &self,
        script: &TariScript,
        spending_key: &TariKeyId,
        known_script_index: Option<usize>,
        known_scripts: &[KnownOneSidedPaymentScript],
    ) -> Result<Option<(ExecutionStack, TariKeyId)>, OutputManagerError> {
        let (input_data, script_key) = if script == &script!(Nop) {
            // This is a nop, so we can just create a new key an create the input stack.
            let key = KeyId::Derived {branch: TransactionKeyManagerBranch::CommitmentMask.get_branch_key(), index:spending_key.managed_index().unwrap() };
            let public_key = self.master_key_manager.get_public_key_at_key_id(&key).await?;
            (inputs!(public_key), key)
        } else {
            // This is a known script so lets fill in the details
            if let Some(index) = known_script_index {
                (
                    known_scripts[index].input.clone(),
                    known_scripts[index].script_key_id.clone(),
                )
            } else {
                // this is push public key script, so lets see if we know the public key
                if let Some(Opcode::PushPubKey(public_key)) = script.opcode(0) {
                    let result = self
                        .master_key_manager
                        .find_script_key_id_from_spend_key_id(spending_key, Some(public_key))
                        .await?;
                    if let Some(script_key_id) = result {
                        (ExecutionStack::default(), script_key_id)
                    } else {
                        // The spending key is recoverable but we dont know how to calculate the script key
                        return Ok(None);
                    }
                } else {
                    // this should not happen as the script should have been either nop, known or a pushpubkey
                    // script, but somehow opcode 0 is not pushPubKey
                    return Ok(None);
                }
            }
        };
        Ok(Some((input_data, script_key)))
    }

    async fn attempt_output_recovery(
        &self,
        output: &TransactionOutput,
    ) -> Result<Option<(TariKeyId, MicroMinotari)>, OutputManagerError> {
        // lets first check if the output exists in the db, if it does we dont have to try recovery as we already know
        // about the output.
        match self.db.fetch_by_commitment(output.commitment().clone()) {
            Ok(_) => return Ok(None),
            Err(OutputManagerStorageError::ValueNotFound) => {},
            Err(e) => return Err(e.into()),
        };
        let (key, committed_value) = match self.master_key_manager.try_output_key_recovery(output, None).await {
            Ok(value) => value,
            // Key manager errors here are actual errors and should not be suppressed.
            Err(TransactionError::KeyManagerError(e)) => return Err(TransactionError::KeyManagerError(e).into()),
            Err(_) => return Ok(None),
        };

        Ok(Some((key, committed_value)))
    }

    /// Find the key manager index that corresponds to the spending key in the rewound output, if found then modify
    /// output to contain correct associated script private key and update the key manager to the highest index it has
    /// seen so far.
    async fn update_outputs_script_private_key_and_update_key_manager_index(
        &mut self,
        output: &mut WalletOutput,
    ) -> Result<(), OutputManagerError> {
        let public_key = self
            .master_key_manager
            .get_public_key_at_key_id(&output.spending_key_id)
            .await?;
        let script_key = {
            let found_index = self
                .master_key_manager
                .find_key_index(
                    TransactionKeyManagerBranch::CommitmentMask.get_branch_key(),
                    &public_key,
                )
                .await?;

            self.master_key_manager
                .update_current_key_index_if_higher(
                    TransactionKeyManagerBranch::CommitmentMask.get_branch_key(),
                    found_index,
                )
                .await?;

            TariKeyId::Derived {
                branch: TransactionKeyManagerBranch::CommitmentMask.get_branch_key(),
                index: found_index,
            }
        };
        let public_script_key = self.master_key_manager.get_public_key_at_key_id(&script_key).await?;
        output.input_data = inputs!(public_script_key);
        output.script_key_id = script_key;
        Ok(())
    }
}
