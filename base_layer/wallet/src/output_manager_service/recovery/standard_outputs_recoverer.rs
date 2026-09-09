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
use tari_common_types::types::FixedHash;
use tari_script::{ExecutionStack, Opcode, TariScript, inputs, script};
use tari_transaction_components::{
    MicroMinotari,
    key_manager::TariKeyId,
    transaction_components::{MemoField, OutputType, TransactionOutput, WalletOutput},
};
use tari_transaction_key_manager::legacy_key_manager::LegacyTransactionKeyManagerInterface;
use tari_utilities::{ByteArray, hex::Hex};

use crate::output_manager_service::{
    error::{OutputManagerError, OutputManagerStorageError},
    handle::RecoveredOutput,
    storage::{
        OutputSource,
        database::{OutputManagerBackend, OutputManagerDatabase},
        models::{DbWalletOutput, KnownOneSidedPaymentScript},
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
    TKeyManagerInterface: LegacyTransactionKeyManagerInterface,
{
    pub fn new(master_key_manager: TKeyManagerInterface, db: OutputManagerDatabase<TBackend>) -> Self {
        Self { master_key_manager, db }
    }

    /// Attempt to rewind all of the given transaction outputs into key_manager outputs. If they can be rewound then add
    /// them to the database and increment the key manager index
    #[allow(clippy::too_many_lines)]
    pub async fn scan_and_recover_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let start = Instant::now();
        let outputs_length = outputs.len();

        let known_scripts = self
            .db
            .get_all_known_one_sided_payment_scripts(&self.master_key_manager)?;

        let mut rewound_outputs: Vec<(WalletOutput, bool, FixedHash)> = Vec::new();
        let push_pub_key_script = script!(PushPubKey(Box::default()))?;
        for output in outputs {
            let known_script_index = known_scripts.iter().position(|s| s.script == output.script);
            if output.script != script!(Nop)? &&
                known_script_index.is_none() &&
                !output.script.pattern_match(&push_pub_key_script)
            {
                continue;
            }

            let (commitment_mask, committed_value, payment_id) = match self.attempt_output_recovery(&output)? {
                Some(recovered) => recovered,
                None => continue,
            };
            let (input_data, script_key) =
                match self.find_script_key(&output.script, &commitment_mask, known_script_index, &known_scripts)? {
                    Some((input_data, script_key)) => (input_data, script_key),
                    None => continue,
                };

            let hash = output.hash();
            let uo = WalletOutput::new_from_transaction_output(
                committed_value,
                commitment_mask,
                payment_id,
                output,
                input_data,
                script_key,
            );

            rewound_outputs.push((uo, known_script_index.is_some(), hash));
        }

        let rewind_time = start.elapsed();
        trace!(
            target: LOG_TARGET,
            "UTXO recovery - checked {} outputs in {} ms",
            outputs_length,
            rewind_time.as_millis(),
        );

        let mut recovered_outputs: Vec<RecoveredOutput> = Vec::new();
        for (output, has_known_script, hash) in &mut rewound_outputs {
            let db_output = DbWalletOutput::from_wallet_output(
                output.clone(),
                None,
                Self::output_source(output, *has_known_script),
                None,
                None,
            );
            let output_hex = db_output.commitment.to_hex();
            let view_key = self.master_key_manager.get_view_key().pub_key;
            if let Err(e) = self.db.add_unspent_output_with_tx_id(
                output.calculate_tx_id(view_key.as_bytes()),
                db_output,
                &self.master_key_manager,
            ) {
                match e {
                    OutputManagerStorageError::DuplicateOutput => {
                        continue;
                    },
                    _ => return Err(OutputManagerError::from(e)),
                }
            }

            recovered_outputs.push(RecoveredOutput {
                output: output.clone(),
                hash: *hash,
            });
            trace!(
                target: LOG_TARGET,
                "Output {} with value {} with {} recovered",
                output_hex,
                output.value(),
                output.features(),
            );
        }

        Ok(recovered_outputs)
    }

    // Helper function to get the output source for a given output
    fn output_source(output: &WalletOutput, has_known_script: bool) -> OutputSource {
        match output.features().output_type {
            OutputType::Standard => match *output.script().as_slice() {
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
            OutputType::SidechainCheckpoint => OutputSource::SidechainCheckpoint,
            OutputType::SidechainProof => OutputSource::SidechainProof,
            OutputType::ValidatorNodeExit => OutputSource::ValidatorNodeExit,
        }
    }

    fn find_script_key(
        &self,
        script: &TariScript,
        spending_key: &TariKeyId,
        known_script_index: Option<usize>,
        known_scripts: &[KnownOneSidedPaymentScript],
    ) -> Result<Option<(ExecutionStack, TariKeyId)>, OutputManagerError> {
        let (input_data, script_key) = if script == &script!(Nop)? {
            let key = nop_script_key(&self.master_key_manager, spending_key)?;
            let public_key = self.master_key_manager.get_public_key_at_key_id(&key)?;
            (inputs!(public_key), key)
        } else {
            // This is a known script so lets fill in the details
            if let Some(index) = known_script_index {
                (
                    known_scripts.get(index).expect("Already checked").input.clone(),
                    known_scripts.get(index).expect("Already checked").script_key_id.clone(),
                )
            } else {
                // this is push public key script, so lets see if we know the public key
                if let Some(Opcode::PushPubKey(public_key)) = script.opcode(0) {
                    let result = self
                        .master_key_manager
                        .find_script_key_id_from_commitment_mask_key_id(spending_key, Some(public_key))?;
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

    fn attempt_output_recovery(
        &self,
        output: &TransactionOutput,
    ) -> Result<Option<(TariKeyId, MicroMinotari, MemoField)>, OutputManagerError> {
        // lets first check if the output exists in the db, if it does we dont have to try recovery as we already know
        // about the output.
        match self
            .db
            .fetch_by_commitment(output.commitment().clone(), &self.master_key_manager)
        {
            Ok(_) => return Ok(None),
            Err(OutputManagerStorageError::ValueNotFound) => {},
            Err(e) => return Err(e.into()),
        };
        let (key, committed_value, payment_id) = match self.master_key_manager.try_output_key_recovery(
            output.commitment(),
            output.encrypted_data(),
            &output.sender_offset_public_key,
        )? {
            Some(value) => value,
            _ => return Ok(None),
        };

        Ok(Some((key, committed_value, payment_id)))
    }
}

/// The script key a recovered `Nop` script output carries.
///
/// A `Nop` script places no constraint on the script key, so it is derived from the recovered commitment mask rather
/// than invented. A random key would be host known: on a ledger wallet it lands in the opaque partial sum the device
/// cannot verify, so spending the output would ask the device for a script offset that no key *it* derived blinds -
/// which it refuses, because the reply would be a sender offset private key it had just generated.
/// `Derived { key: commitment_mask }` is the same shape an ordinary wallet script key has, so the device can derive
/// it from `alpha`.
fn nop_script_key<KM: LegacyTransactionKeyManagerInterface>(
    key_manager: &KM,
    commitment_mask_key_id: &TariKeyId,
) -> Result<TariKeyId, OutputManagerError> {
    key_manager
        .find_script_key_id_from_commitment_mask_key_id(commitment_mask_key_id, None)?
        .ok_or_else(|| {
            OutputManagerError::BuildError(
                "Could not derive a script key from the recovered commitment mask".to_string(),
            )
        })
}

#[cfg(test)]
mod test {
    use tari_common_types::types::PrivateKey;
    use tari_crypto::keys::SecretKey;
    use tari_transaction_components::key_manager::{
        ScriptKeyBucket,
        TransactionKeyManagerInterface,
        script_key_bucket,
    };
    use tari_transaction_key_manager::legacy_key_manager::create_new_random_key_manager;

    use super::*;

    /// `attempt_output_recovery` hands back an `Encrypted` or `DHCommitmentMask` commitment mask, which is the
    /// common case, and the script key has to be derived from it rather than invented - otherwise the wallet cannot
    /// spend the output on a ledger at all.
    #[tokio::test]
    async fn a_recovered_nop_output_carries_a_derived_script_key() {
        let key_manager = create_new_random_key_manager().await.unwrap();
        let commitment_mask = key_manager
            .create_encrypted_key(PrivateKey::random(&mut rand::rng()), None)
            .unwrap();

        let script_key = nop_script_key(&key_manager, &commitment_mask).unwrap();
        assert!(
            matches!(script_key, TariKeyId::Derived { .. }),
            "unexpected script key: {script_key:?}"
        );

        // ...and the derived key really is the one whose public key the input stack pushes.
        assert_eq!(
            key_manager.get_public_key_at_key_id(&script_key).unwrap(),
            key_manager
                .get_public_key_at_key_id(&TariKeyId::Derived {
                    key: (&commitment_mask).into()
                })
                .unwrap()
        );
    }

    /// The point of deriving it: a ledger device folds a `Derived` script key into `alpha` itself, so a script
    /// offset over nothing but recovered outputs still carries a script side term the device generated. That is
    /// what the device requires before it will hand an offset back at all - see
    /// `key_manager::manager::tests::a_script_offset_whose_only_script_key_is_host_known_is_refused`.
    #[tokio::test]
    async fn a_recovered_script_key_is_one_the_device_derives() {
        let key_manager = create_new_random_key_manager().await.unwrap();
        let commitment_mask = key_manager
            .create_encrypted_key(PrivateKey::random(&mut rand::rng()), None)
            .unwrap();

        let script_key = nop_script_key(&key_manager, &commitment_mask).unwrap();
        assert_eq!(script_key_bucket(&script_key), ScriptKeyBucket::AlphaDerived);

        // A script offset over nothing but recovered outputs is accepted.
        assert!(key_manager.get_script_offset(&[script_key], 1).is_ok());

        // An output recovered by an older build carries a random `Encrypted` script key instead. There is no
        // migration for those: the device cannot derive them, so they blind nothing against the host and the
        // device refuses the offset. The user has to re-run recovery.
        let legacy_script_key = key_manager
            .create_encrypted_key(PrivateKey::random(&mut rand::rng()), None)
            .unwrap();
        assert_eq!(script_key_bucket(&legacy_script_key), ScriptKeyBucket::HostKnown);
    }
}
