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
    fmt::{Display, Formatter},
};

use blake2::Digest;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Challenge, ComSignature, Commitment, CommitmentFactory, HashDigest, PublicKey};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    script::{ExecutionStack, ScriptContext, StackItem, TariScript},
    tari_utilities::{hex::Hex, ByteArray, Hashable},
};

use crate::transactions::{
    transaction,
    transaction::{transaction_output::TransactionOutput, OutputFeatures, TransactionError, UnblindedOutput},
};

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionInput {
    /// The features of the output being spent. We will check maturity for all outputs.
    pub features: OutputFeatures,
    /// The commitment referencing the output being spent.
    pub commitment: Commitment,
    /// The serialised script
    pub script: TariScript,
    /// The script input data, if any
    pub input_data: ExecutionStack,
    /// A signature with k_s, signing the script, input data, and mined height
    pub script_signature: ComSignature,
    /// The offset public key, K_O
    pub sender_offset_public_key: PublicKey,
}

/// An input for a transaction that spends an existing output
impl TransactionInput {
    /// Create a new Transaction Input
    pub fn new(
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        input_data: ExecutionStack,
        script_signature: ComSignature,
        sender_offset_public_key: PublicKey,
    ) -> TransactionInput {
        TransactionInput {
            features,
            commitment,
            script,
            input_data,
            script_signature,
            sender_offset_public_key,
        }
    }

    pub fn build_script_challenge(
        nonce_commitment: &Commitment,
        script: &TariScript,
        input_data: &ExecutionStack,
        script_public_key: &PublicKey,
        commitment: &Commitment,
    ) -> Vec<u8> {
        Challenge::new()
            .chain(nonce_commitment.as_bytes())
            .chain(script.as_bytes().as_slice())
            .chain(input_data.as_bytes().as_slice())
            .chain(script_public_key.as_bytes())
            .chain(commitment.as_bytes())
            .finalize()
            .to_vec()
    }

    /// Accessor method for the commitment contained in an input
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Checks if the given un-blinded input instance corresponds to this blinded Transaction Input
    pub fn opened_by(&self, input: &UnblindedOutput, factory: &CommitmentFactory) -> bool {
        factory.open(&input.spending_key, &input.value.into(), &self.commitment)
    }

    /// This will check if the input and the output is the same transactional output by looking at the commitment and
    /// features and script. This will ignore all other output and input fields
    pub fn is_equal_to(&self, output: &TransactionOutput) -> bool {
        self.output_hash() == output.hash()
    }

    /// This will run the script contained in the TransactionInput, returning either a script error or the resulting
    /// public key.
    pub fn run_script(&self, context: Option<ScriptContext>) -> Result<PublicKey, TransactionError> {
        let context = context.unwrap_or_default();
        match self.script.execute_with_context(&self.input_data, &context)? {
            StackItem::PublicKey(pubkey) => Ok(pubkey),
            _ => Err(TransactionError::ScriptExecutionError(
                "The script executed successfully but it did not leave a public key on the stack".to_string(),
            )),
        }
    }

    pub fn validate_script_signature(
        &self,
        public_script_key: &PublicKey,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError> {
        let challenge = TransactionInput::build_script_challenge(
            self.script_signature.public_nonce(),
            &self.script,
            &self.input_data,
            public_script_key,
            &self.commitment,
        );
        if self
            .script_signature
            .verify_challenge(&(&self.commitment + public_script_key), &challenge, factory)
        {
            Ok(())
        } else {
            Err(TransactionError::InvalidSignatureError(
                "Verifying script signature".to_string(),
            ))
        }
    }

    /// This will run the script and verify the script signature. If its valid, it will return the resulting public key
    /// from the script.
    pub fn run_and_verify_script(
        &self,
        factory: &CommitmentFactory,
        context: Option<ScriptContext>,
    ) -> Result<PublicKey, TransactionError> {
        let key = self.run_script(context)?;
        self.validate_script_signature(&key, factory)?;
        Ok(key)
    }

    /// Returns true if this input is mature at the given height, otherwise false
    pub fn is_mature_at(&self, block_height: u64) -> bool {
        self.features.maturity <= block_height
    }

    /// Returns the hash of the output data contained in this input.
    /// This hash matches the hash of a transaction output that this input spends.
    pub fn output_hash(&self) -> Vec<u8> {
        transaction::hash_output(&self.features, &self.commitment, &self.script)
    }
}

/// Implement the canonical hashing function for TransactionInput for use in ordering
impl Hashable for TransactionInput {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.features.to_v1_bytes())
            .chain(self.commitment.as_bytes())
            .chain(self.script.as_bytes())
            .chain(self.sender_offset_public_key.as_bytes())
            .chain(self.script_signature.u().as_bytes())
            .chain(self.script_signature.v().as_bytes())
            .chain(self.script_signature.public_nonce().as_bytes())
            .chain(self.input_data.as_bytes())
            .finalize()
            .to_vec()
    }
}

impl Display for TransactionInput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "{} [{:?}], Script hash: ({}), Offset_Pubkey: ({})",
            self.commitment.to_hex(),
            self.features,
            self.script,
            self.sender_offset_public_key.to_hex()
        )
    }
}

impl PartialOrd for TransactionInput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.commitment.partial_cmp(&other.commitment)
    }
}

impl Ord for TransactionInput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.commitment.cmp(&other.commitment)
    }
}
