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

use std::{cmp::Ordering, ops::Shl};

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{BlindingFactor, ComSignature, CommitmentFactory, PrivateKey, PublicKey, RangeProof};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::{RangeProofError, RangeProofService},
    script::{ExecutionStack, TariScript},
    tari_utilities::ByteArray,
};

use crate::{
    consensus::{ConsensusEncodingSized, ConsensusEncodingWrapper},
    transactions::{
        tari_amount::MicroTari,
        transaction,
        transaction::{
            transaction_input::{SpentOutput, TransactionInput},
            transaction_output::TransactionOutput,
            OutputFeatures,
            TransactionError,
        },
        transaction_protocol::RewindData,
        CryptoFactories,
    },
};

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
// TODO: Try to get rid of 'Serialize' and 'Deserialize' traits here; see related comment at 'struct RawTransactionInfo'
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnblindedOutput {
    pub value: MicroTari,
    pub spending_key: BlindingFactor,
    pub features: OutputFeatures,
    pub script: TariScript,
    pub input_data: ExecutionStack,
    pub script_private_key: PrivateKey,
    pub sender_offset_public_key: PublicKey,
    pub metadata_signature: ComSignature,
    pub script_lock_height: u64,
}

impl UnblindedOutput {
    /// Creates a new un-blinded output
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value: MicroTari,
        spending_key: BlindingFactor,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: PrivateKey,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
        script_lock_height: u64,
    ) -> UnblindedOutput {
        UnblindedOutput {
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
        }
    }

    /// Commits an UnblindedOutput into a Transaction input
    pub fn as_transaction_input(&self, factory: &CommitmentFactory) -> Result<TransactionInput, TransactionError> {
        let commitment = factory.commit(&self.spending_key, &self.value.into());
        let script_nonce_a = PrivateKey::random(&mut OsRng);
        let script_nonce_b = PrivateKey::random(&mut OsRng);
        let nonce_commitment = factory.commit(&script_nonce_b, &script_nonce_a);

        let challenge = TransactionInput::build_script_challenge(
            &nonce_commitment,
            &self.script,
            &self.input_data,
            &PublicKey::from_secret_key(&self.script_private_key),
            &commitment,
        );
        let script_signature = ComSignature::sign(
            self.value.into(),
            &self.script_private_key + &self.spending_key,
            script_nonce_a,
            script_nonce_b,
            &challenge,
            factory,
        )
        .map_err(|_| TransactionError::InvalidSignatureError("Generating script signature".to_string()))?;

        Ok(TransactionInput {
            spent_output: SpentOutput::OutputData {
                features: self.features.clone(),
                commitment,
                script: self.script.clone(),
                sender_offset_public_key: self.sender_offset_public_key.clone(),
            },
            input_data: self.input_data.clone(),
            script_signature,
        })
    }

    /// Commits an UnblindedOutput into a TransactionInput that only contains the hash of the spent output data
    pub fn as_compact_transaction_input(
        &self,
        factory: &CommitmentFactory,
    ) -> Result<TransactionInput, TransactionError> {
        let input = self.as_transaction_input(factory)?;

        Ok(TransactionInput {
            spent_output: SpentOutput::OutputHash(input.output_hash()),
            input_data: input.input_data,
            script_signature: input.script_signature,
        })
    }

    pub fn as_transaction_output(&self, factories: &CryptoFactories) -> Result<TransactionOutput, TransactionError> {
        if factories.range_proof.range() < 64 && self.value >= MicroTari::from(1u64.shl(&factories.range_proof.range()))
        {
            return Err(TransactionError::ValidationError(
                "Value provided is outside the range allowed by the range proof".into(),
            ));
        }
        let commitment = factories.commitment.commit(&self.spending_key, &self.value.into());
        let output = TransactionOutput {
            features: self.features.clone(),
            commitment,
            proof: RangeProof::from_bytes(
                &factories
                    .range_proof
                    .construct_proof(&self.spending_key, self.value.into())?,
            )
            .map_err(|_| TransactionError::RangeProofError(RangeProofError::ProofConstructionError))?,
            script: self.script.clone(),
            sender_offset_public_key: self.sender_offset_public_key.clone(),
            metadata_signature: self.metadata_signature.clone(),
        };

        Ok(output)
    }

    pub fn as_rewindable_transaction_output(
        &self,
        factories: &CryptoFactories,
        rewind_data: &RewindData,
    ) -> Result<TransactionOutput, TransactionError> {
        if factories.range_proof.range() < 64 && self.value >= MicroTari::from(1u64.shl(&factories.range_proof.range()))
        {
            return Err(TransactionError::ValidationError(
                "Value provided is outside the range allowed by the range proof".into(),
            ));
        }
        let commitment = factories.commitment.commit(&self.spending_key, &self.value.into());

        let proof_bytes = factories.range_proof.construct_proof_with_rewind_key(
            &self.spending_key,
            self.value.into(),
            &rewind_data.rewind_key,
            &rewind_data.rewind_blinding_key,
            &rewind_data.proof_message,
        )?;

        let proof = RangeProof::from_bytes(&proof_bytes)
            .map_err(|_| TransactionError::RangeProofError(RangeProofError::ProofConstructionError))?;

        let output = TransactionOutput {
            features: self.features.clone(),
            commitment,
            proof,
            script: self.script.clone(),
            sender_offset_public_key: self.sender_offset_public_key.clone(),
            metadata_signature: self.metadata_signature.clone(),
        };

        Ok(output)
    }

    pub fn metadata_byte_size(&self) -> usize {
        self.features.consensus_encode_exact_size() +
            ConsensusEncodingWrapper::wrap(&self.script).consensus_encode_exact_size()
    }

    // Note: The Hashable trait is not used here due to the dependency on `CryptoFactories`, and `commitment` us not
    // Note: added to the struct to ensure the atomic nature between `commitment`, `spending_key` and `value`.
    pub fn hash(&self, factories: &CryptoFactories) -> Vec<u8> {
        let commitment = factories.commitment.commit_value(&self.spending_key, self.value.into());
        transaction::hash_output(&self.features, &commitment, &self.script)
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
