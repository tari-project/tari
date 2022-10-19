// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::{
    cmp::Ordering,
    fmt::{Debug, Formatter},
    ops::Shl,
};

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
    BlindingFactor,
    BulletRangeProof,
    ComSignature,
    CommitmentFactory,
    FixedHash,
    PrivateKey,
    PublicKey,
    RangeProof,
};
use tari_crypto::{
    commitment::{ExtensionDegree, HomomorphicCommitmentFactory},
    errors::RangeProofError,
    extended_range_proof::ExtendedRangeProofService,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::RangeProofService,
    ristretto::bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
    tari_utilities::ByteArray,
};
use tari_script::{ExecutionStack, TariScript};

use super::TransactionOutputVersion;
use crate::{
    consensus::ConsensusEncodingSized,
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components,
        transaction_components::{
            transaction_input::{SpentOutput, TransactionInput},
            transaction_output::TransactionOutput,
            EncryptedValue,
            OutputFeatures,
            TransactionError,
            TransactionInputVersion,
        },
        transaction_protocol::RewindData,
        CryptoFactories,
    },
};

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
// TODO: Try to get rid of 'Serialize' and 'Deserialize' traits here; see related comment at 'struct RawTransactionInfo'
// #LOGGED
#[derive(Clone, Serialize, Deserialize)]
pub struct UnblindedOutput {
    pub version: TransactionOutputVersion,
    pub value: MicroTari,
    pub spending_key: BlindingFactor,
    pub features: OutputFeatures,
    pub script: TariScript,
    pub covenant: Covenant,
    pub input_data: ExecutionStack,
    pub script_private_key: PrivateKey,
    pub sender_offset_public_key: PublicKey,
    pub metadata_signature: ComSignature,
    pub script_lock_height: u64,
    pub encrypted_value: EncryptedValue,
    pub minimum_value_promise: MicroTari,
}

impl UnblindedOutput {
    /// Creates a new un-blinded output

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: TransactionOutputVersion,
        value: MicroTari,
        spending_key: BlindingFactor,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: PrivateKey,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_value: EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> Self {
        Self {
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        }
    }

    pub fn new_current_version(
        value: MicroTari,
        spending_key: BlindingFactor,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: PrivateKey,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_value: EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> Self {
        Self::new(
            TransactionOutputVersion::get_current_version(),
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        )
    }

    /// Commits an UnblindedOutput into a Transaction input
    pub fn as_transaction_input(&self, factory: &CommitmentFactory) -> Result<TransactionInput, TransactionError> {
        let commitment = factory.commit(&self.spending_key, &self.value.into());
        let r_a = PrivateKey::random(&mut OsRng);
        let r_x = PrivateKey::random(&mut OsRng);
        let r_y = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = factory.commit(&r_x, &r_a);
        let ephemeral_pubkey = PublicKey::from_secret_key(&r_y);

        let challenge = TransactionInput::build_script_challenge(
            TransactionInputVersion::get_current_version(),
            &ephemeral_commitment,
            &ephemeral_pubkey,
            &self.script,
            &self.input_data,
            &PublicKey::from_secret_key(&self.script_private_key),
            &commitment,
        );
        let script_signature = ComSignature::sign(
            &self.value.into(),
            &self.spending_key,
            &self.script_private_key,
            &r_a,
            &r_x,
            &r_y,
            &challenge,
            factory,
        )
        .map_err(|_| TransactionError::InvalidSignatureError("Generating script signature".to_string()))?;

        Ok(TransactionInput::new_current_version(
            SpentOutput::OutputData {
                features: self.features.clone(),
                commitment,
                script: self.script.clone(),
                sender_offset_public_key: self.sender_offset_public_key.clone(),
                covenant: self.covenant.clone(),
                version: self.version,
                encrypted_value: self.encrypted_value.clone(),
                minimum_value_promise: self.minimum_value_promise,
            },
            self.input_data.clone(),
            script_signature,
        ))
    }

    /// Commits an UnblindedOutput into a TransactionInput that only contains the hash of the spent output data
    pub fn as_compact_transaction_input(
        &self,
        factory: &CommitmentFactory,
    ) -> Result<TransactionInput, TransactionError> {
        let input = self.as_transaction_input(factory)?;

        Ok(TransactionInput::new(
            input.version,
            SpentOutput::OutputHash(input.output_hash()),
            input.input_data,
            input.script_signature,
        ))
    }

    pub fn as_transaction_output(&self, factories: &CryptoFactories) -> Result<TransactionOutput, TransactionError> {
        if factories.range_proof.range() < 64 && self.value >= MicroTari::from(1u64.shl(&factories.range_proof.range()))
        {
            return Err(TransactionError::ValidationError(
                "Value provided is outside the range allowed by the range proof".into(),
            ));
        }
        let commitment = factories.commitment.commit(&self.spending_key, &self.value.into());

        let range_proof = self.construct_range_proof(factories, None)?;

        let output = TransactionOutput::new(
            self.version,
            self.features.clone(),
            commitment,
            range_proof,
            self.script.clone(),
            self.sender_offset_public_key.clone(),
            self.metadata_signature.clone(),
            self.covenant.clone(),
            self.encrypted_value.clone(),
            self.minimum_value_promise,
        );

        Ok(output)
    }

    fn construct_range_proof(
        &self,
        factories: &CryptoFactories,
        seed_nonce: Option<PrivateKey>,
    ) -> Result<RangeProof, TransactionError> {
        let proof_bytes_result = if self.minimum_value_promise.as_u64() == 0 {
            match seed_nonce {
                Some(nonce) => factories.range_proof.construct_proof_with_recovery_seed_nonce(
                    &self.spending_key,
                    self.value.into(),
                    &nonce,
                ),
                None => factories
                    .range_proof
                    .construct_proof(&self.spending_key, self.value.into()),
            }
        } else {
            let extended_mask =
                RistrettoExtendedMask::assign(ExtensionDegree::DefaultPedersen, vec![self.spending_key.clone()])?;

            let extended_witness = RistrettoExtendedWitness {
                mask: extended_mask,
                value: self.value.into(),
                minimum_value_promise: self.minimum_value_promise.as_u64(),
            };

            factories
                .range_proof
                .construct_extended_proof(vec![extended_witness], seed_nonce)
        };

        let proof_bytes = proof_bytes_result.map_err(|err| {
            TransactionError::RangeProofError(RangeProofError::ProofConstructionError(format!(
                "Failed to construct range proof: {}",
                err
            )))
        })?;

        RangeProof::from_bytes(&proof_bytes).map_err(|_| {
            TransactionError::RangeProofError(RangeProofError::ProofConstructionError(
                "Rangeproof factory returned invalid range proof bytes".to_string(),
            ))
        })
    }

    pub fn as_rewindable_transaction_output(
        &self,
        factories: &CryptoFactories,
        rewind_data: &RewindData,
        range_proof: Option<&BulletRangeProof>,
    ) -> Result<TransactionOutput, TransactionError> {
        if factories.range_proof.range() < 64 && self.value >= MicroTari::from(1u64.shl(&factories.range_proof.range()))
        {
            return Err(TransactionError::ValidationError(
                "Value provided is outside the range allowed by the range proof".into(),
            ));
        }
        let commitment = factories.commitment.commit(&self.spending_key, &self.value.into());

        let proof = if let Some(proof) = range_proof {
            proof.clone()
        } else {
            self.construct_range_proof(factories, Some(rewind_data.rewind_blinding_key.clone()))
                .map_err(|_| {
                    TransactionError::RangeProofError(RangeProofError::ProofConstructionError(
                        "Creating rewindable transaction output".to_string(),
                    ))
                })?
        };

        let output = TransactionOutput::new(
            self.version,
            self.features.clone(),
            commitment,
            proof,
            self.script.clone(),
            self.sender_offset_public_key.clone(),
            self.metadata_signature.clone(),
            self.covenant.clone(),
            self.encrypted_value.clone(),
            self.minimum_value_promise,
        );

        Ok(output)
    }

    pub fn metadata_byte_size(&self) -> usize {
        self.features.consensus_encode_exact_size() +
            self.script.consensus_encode_exact_size() +
            self.covenant.consensus_encode_exact_size()
    }

    // Note: The Hashable trait is not used here due to the dependency on `CryptoFactories`, and `commitment` is not
    // Note: added to the struct to ensure consistency between `commitment`, `spending_key` and `value`.
    pub fn hash(&self, factories: &CryptoFactories) -> FixedHash {
        let commitment = factories.commitment.commit_value(&self.spending_key, self.value.into());
        transaction_components::hash_output(
            self.version,
            &self.features,
            &commitment,
            &self.script,
            &self.covenant,
            &self.encrypted_value,
            &self.sender_offset_public_key,
            self.minimum_value_promise,
        )
    }
}

// These implementations are used for order these outputs for UTXO selection which will be done by comparing the values
impl Eq for UnblindedOutput {}

impl PartialEq for UnblindedOutput {
    fn eq(&self, other: &UnblindedOutput) -> bool {
        self.value == other.value
    }
}

impl PartialOrd<UnblindedOutput> for UnblindedOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl Ord for UnblindedOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Debug for UnblindedOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnblindedOutput")
            .field("version", &self.version)
            .field("value", &self.value)
            .field("spending_key", &"<secret>")
            .field("features", &self.features)
            .field("script", &self.script)
            .field("covenant", &self.covenant)
            .field("input_data", &self.input_data)
            .field("script_private_key", &"<secret>")
            .field("sender_offset_public_key", &self.sender_offset_public_key)
            .field("metadata_signature", &self.metadata_signature)
            .field("script_lock_height", &self.script_lock_height)
            .field("minimum_value_promise", &self.minimum_value_promise)
            .finish()
    }
}
