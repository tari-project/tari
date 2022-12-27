// Copyright 2022. The Tari Project
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

use std::convert::TryInto;

use log::{trace, warn};
use tari_common_types::types::{CommitmentFactory, HashOutput, PublicKey};
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_script::{ScriptContext, TariScript};
use tari_utilities::hex::Hex;

use crate::{
    borsh::SerializedSize,
    chain_storage::{BlockchainBackend, BlockchainDatabase, MmrTree},
    consensus::ConsensusConstants,
    transactions::{
        transaction_components::{Transaction, TransactionError, TransactionInput, TransactionOutput},
        CryptoFactories,
    },
    validation::ValidationError,
};

pub const LOG_TARGET: &str = "c::val::chain_transaction_validator";

pub struct ChainLinkedTransactionValidator<B> {
    db: BlockchainDatabase<B>,
    factories: CryptoFactories,
}

impl<B: BlockchainBackend + 'static> ChainLinkedTransactionValidator<B> {
    pub fn new(db: BlockchainDatabase<B>) -> Self {
        Self {
            db,
            factories: CryptoFactories::default(),
        }
    }

    pub async fn validate(
        &self,
        tx: &Transaction,
        prev_hash: Option<HashOutput>,
        height: u64,
    ) -> Result<(), ValidationError> {
        validate_script_offset(tx, &self.factories.commitment, prev_hash, height)?;
        validate_covenants(tx, height)?;

        let constants = self.db.consensus_constants()?;
        {
            let db = self.db.db_read_access()?;
            validate_excess_sig_not_in_db(&*db, tx)?;
            check_inputs_are_utxos(&*db, tx)?;
            check_outputs(&*db, constants, tx)?;
        };

        verify_timelocks(tx, height)?;

        Ok(())
    }
}

fn validate_excess_sig_not_in_db<B: BlockchainBackend>(db: &B, tx: &Transaction) -> Result<(), ValidationError> {
    for kernel in tx.body.kernels() {
        if let Some((db_kernel, header_hash)) = db.fetch_kernel_by_excess_sig(&kernel.excess_sig.to_owned())? {
            let msg = format!(
                "Aggregate body contains kernel excess: {} which matches already existing excess signature in chain \
                 database block hash: {}. Existing kernel excess: {}, excess sig nonce: {}, excess signature: {}",
                kernel.excess.to_hex(),
                header_hash.to_hex(),
                db_kernel.excess.to_hex(),
                db_kernel.excess_sig.get_public_nonce().to_hex(),
                db_kernel.excess_sig.get_signature().to_hex(),
            );
            return Err(ValidationError::DuplicateKernelError(msg));
        };
    }
    Ok(())
}

/// this will validate the script offset of the aggregate body.
fn validate_script_offset(
    tx: &Transaction,
    factory: &CommitmentFactory,
    prev_header: Option<HashOutput>,
    height: u64,
) -> Result<(), TransactionError> {
    trace!(target: LOG_TARGET, "Checking script offset");
    let script_offset = PublicKey::from_secret_key(&tx.script_offset);

    // lets count up the input script public keys
    let mut input_keys = PublicKey::default();
    let prev_hash: [u8; 32] = prev_header.unwrap_or_default().as_slice().try_into().unwrap_or([0; 32]);
    for input in tx.body.inputs() {
        let context = ScriptContext::new(height, &prev_hash, input.commitment()?);
        input_keys = input_keys + input.run_and_verify_script(factory, Some(context))?;
    }

    // Now lets gather the output public keys and hashes.
    let mut output_keys = PublicKey::default();
    for output in tx.body.outputs() {
        // We should not count the coinbase tx here
        if !output.is_coinbase() {
            output_keys = output_keys + output.sender_offset_public_key.clone();
        }
    }
    let lhs = input_keys - output_keys;
    if lhs != script_offset {
        return Err(TransactionError::ScriptOffset);
    }
    Ok(())
}

fn validate_covenants(tx: &Transaction, height: u64) -> Result<(), TransactionError> {
    for input in tx.body.inputs() {
        input.covenant()?.execute(height, input, tx.body.outputs())?;
    }
    Ok(())
}

/// This function checks that all inputs in the blocks are valid UTXO's to be spent
fn check_inputs_are_utxos<B: BlockchainBackend>(db: &B, tx: &Transaction) -> Result<(), ValidationError> {
    let mut not_found_inputs = Vec::new();
    let mut output_hashes = None;

    for input in tx.body.inputs() {
        // If spending a unique_id, a new output must contain the unique id
        match check_input_is_utxo(db, input) {
            Ok(_) => continue,
            Err(ValidationError::UnknownInput) => {
                // Lazily allocate and hash outputs as needed
                if output_hashes.is_none() {
                    output_hashes = Some(tx.body.outputs().iter().map(|output| output.hash()).collect::<Vec<_>>());
                }

                let output_hashes = output_hashes.as_ref().unwrap();
                let output_hash = input.output_hash();
                if output_hashes.iter().any(|output| output == &output_hash) {
                    continue;
                }
                not_found_inputs.push(output_hash);
            },
            Err(err) => {
                return Err(err);
            },
        }
    }

    if !not_found_inputs.is_empty() {
        return Err(ValidationError::UnknownInputs(not_found_inputs));
    }

    Ok(())
}

/// This function checks that an input is a valid spendable UTXO
fn check_input_is_utxo<B: BlockchainBackend>(db: &B, input: &TransactionInput) -> Result<(), ValidationError> {
    let output_hash = input.output_hash();
    if let Some(utxo_hash) = db.fetch_unspent_output_hash_by_commitment(input.commitment()?)? {
        // We know that the commitment exists in the UTXO set. Check that the output hash matches (i.e. all fields
        // like output features match)
        if utxo_hash == output_hash {
            // Because the retrieved hash matches the new input.output_hash() we know all the fields match and are all
            // still the same
            return Ok(());
        }

        let output = db.fetch_output(&utxo_hash)?;
        warn!(
            target: LOG_TARGET,
            "Input spends a UTXO but does not produce the same hash as the output it spends: Expected hash: {}, \
             provided hash:{}
            input: {:?}. output in db: {:?}",
            utxo_hash.to_hex(),
            output_hash.to_hex(),
            input,
            output
        );

        return Err(ValidationError::UnknownInput);
    }

    // Wallet needs to know if a transaction has already been mined and uses this error variant to do so.
    if db.fetch_output(&output_hash)?.is_some() {
        warn!(
            target: LOG_TARGET,
            "Validation failed due to already spent input: {}", input
        );
        // We know that the output here must be spent because `fetch_unspent_output_hash_by_commitment` would have
        // been Some
        return Err(ValidationError::ContainsSTxO);
    }

    warn!(
        target: LOG_TARGET,
        "Validation failed due to input: {} which does not exist yet", input
    );
    Err(ValidationError::UnknownInput)
}

/// This function checks:
/// 1. that the output type is permitted
/// 2. the byte size of TariScript does not exceed the maximum
/// 3. that the outputs do not already exist in the UTxO set.
fn check_outputs<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    tx: &Transaction,
) -> Result<(), ValidationError> {
    let max_script_size = constants.get_max_script_byte_size();
    for output in tx.body.outputs() {
        check_permitted_output_types(constants, output)?;
        check_tari_script_byte_size(&output.script, max_script_size)?;
        check_not_duplicate_txo(db, output)?;
        check_validator_node_registration_utxo(constants, output)?;
    }
    Ok(())
}

fn check_permitted_output_types(
    constants: &ConsensusConstants,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    if !constants
        .permitted_output_types()
        .contains(&output.features.output_type)
    {
        return Err(ValidationError::OutputTypeNotPermitted {
            output_type: output.features.output_type,
        });
    }

    Ok(())
}

/// Checks the byte size of TariScript is less than or equal to the given size, otherwise returns an error.
fn check_tari_script_byte_size(script: &TariScript, max_script_size: usize) -> Result<(), ValidationError> {
    let script_size = script.get_serialized_size();
    if script_size > max_script_size {
        return Err(ValidationError::TariScriptExceedsMaxSize {
            max_script_size,
            actual_script_size: script_size,
        });
    }
    Ok(())
}

/// This function checks that the outputs do not already exist in the TxO set.
fn check_not_duplicate_txo<B: BlockchainBackend>(db: &B, output: &TransactionOutput) -> Result<(), ValidationError> {
    if let Some(index) = db.fetch_mmr_leaf_index(MmrTree::Utxo, &output.hash())? {
        warn!(
            target: LOG_TARGET,
            "Validation failed due to previously spent output: {} (MMR index = {})", output, index
        );
        return Err(ValidationError::ContainsTxO);
    }
    if db
        .fetch_unspent_output_hash_by_commitment(&output.commitment)?
        .is_some()
    {
        warn!(
            target: LOG_TARGET,
            "Duplicate UTXO set commitment found for output: {}", output
        );
        return Err(ValidationError::ContainsDuplicateUtxoCommitment);
    }

    Ok(())
}

fn check_validator_node_registration_utxo(
    constants: &ConsensusConstants,
    utxo: &TransactionOutput,
) -> Result<(), ValidationError> {
    if let Some(reg) = utxo.features.validator_node_registration() {
        if utxo.minimum_value_promise < constants.validator_node_registration_min_deposit_amount() {
            return Err(ValidationError::ValidatorNodeRegistrationMinDepositAmount {
                min: constants.validator_node_registration_min_deposit_amount(),
                actual: utxo.minimum_value_promise,
            });
        }
        if utxo.features.maturity < constants.validator_node_registration_min_lock_height() {
            return Err(ValidationError::ValidatorNodeRegistrationMinLockHeight {
                min: constants.validator_node_registration_min_lock_height(),
                actual: utxo.features.maturity,
            });
        }

        // TODO(SECURITY): Signing this with a blank msg allows the signature to be replayed. Using the commitment
        //                 is ideal as uniqueness is enforced. However, because the VN and wallet have different
        //                 keys this becomes difficult. Fix this once we have decided on a solution.
        if !reg.is_valid_signature_for(&[]) {
            return Err(ValidationError::InvalidValidatorNodeSignature);
        }
    }
    Ok(())
}

// This function checks that all the timelocks in the provided transaction pass. It checks kernel lock heights and
// input maturities
fn verify_timelocks(tx: &Transaction, current_height: u64) -> Result<(), ValidationError> {
    if tx.min_spendable_height() > current_height + 1 {
        warn!(
            target: LOG_TARGET,
            "Transaction has a min spend height higher than the current tip"
        );
        return Err(ValidationError::MaturityError);
    }
    Ok(())
}
