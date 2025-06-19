// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::data_structures::{
    types::{CompressedCommitment, CompressedPublicKey, MicroMinotari},
    encrypted_data::EncryptedData,
    wallet_output::{LightweightCovenant, LightweightOutputFeatures, LightweightRangeProof, LightweightScript, LightweightSignature},
};
use crate::hex_utils::{HexEncodable, HexValidatable, HexError};
use serde::{Deserialize, Serialize};
use borsh::{BorshSerialize, BorshDeserialize};
use hex::ToHex;

/// Output for a transaction, defining the new ownership of coins that are being transferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LightweightTransactionOutput {
    /// Output version
    pub version: u8,
    /// Options for an output's structure or use
    pub features: LightweightOutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commitment: CompressedCommitment,
    /// A proof that the commitment is in the right range
    pub proof: Option<LightweightRangeProof>,
    /// The script that will be executed when spending this output
    pub script: LightweightScript,
    /// Tari script offset pubkey, K_O
    pub sender_offset_public_key: CompressedPublicKey,
    /// UTXO signature with the script offset private key, k_O
    pub metadata_signature: LightweightSignature,
    /// The covenant that will be executed when spending this output
    pub covenant: LightweightCovenant,
    /// Encrypted value.
    pub encrypted_data: EncryptedData,
    /// The minimum value of the commitment that is proven by the range proof
    pub minimum_value_promise: MicroMinotari,
}

impl LightweightTransactionOutput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: u8,
        features: LightweightOutputFeatures,
        commitment: CompressedCommitment,
        proof: Option<LightweightRangeProof>,
        script: LightweightScript,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: LightweightSignature,
        covenant: LightweightCovenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
    ) -> Self {
        Self {
            version,
            features,
            commitment,
            proof,
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
            encrypted_data,
            minimum_value_promise,
        }
    }

    pub fn version(&self) -> u8 {
        self.version
    }
    pub fn features(&self) -> &LightweightOutputFeatures {
        &self.features
    }
    pub fn commitment(&self) -> &CompressedCommitment {
        &self.commitment
    }
    pub fn proof(&self) -> Option<&LightweightRangeProof> {
        self.proof.as_ref()
    }
    pub fn script(&self) -> &LightweightScript {
        &self.script
    }
    pub fn sender_offset_public_key(&self) -> &CompressedPublicKey {
        &self.sender_offset_public_key
    }
    pub fn metadata_signature(&self) -> &LightweightSignature {
        &self.metadata_signature
    }
    pub fn covenant(&self) -> &LightweightCovenant {
        &self.covenant
    }
    pub fn encrypted_data(&self) -> &EncryptedData {
        &self.encrypted_data
    }
    pub fn minimum_value_promise(&self) -> MicroMinotari {
        self.minimum_value_promise
    }
}

impl Default for LightweightTransactionOutput {
    fn default() -> Self {
        Self {
            version: 1,
            features: LightweightOutputFeatures::default(),
            commitment: CompressedCommitment::new([0u8; 33]),
            proof: None,
            script: LightweightScript::default(),
            sender_offset_public_key: CompressedPublicKey::new([0u8; 32]),
            metadata_signature: LightweightSignature::default(),
            covenant: LightweightCovenant::default(),
            encrypted_data: EncryptedData::default(),
            minimum_value_promise: MicroMinotari::new(0),
        }
    }
}

impl HexEncodable for LightweightTransactionOutput {
    fn to_hex(&self) -> String {
        // For complex structures, we'll serialize to bytes first, then hex
        let bytes = borsh::to_vec(self).unwrap_or_default();
        bytes.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        borsh::from_slice(&bytes).map_err(|e| HexError::InvalidHex(e.to_string()))
    }
}

impl HexValidatable for LightweightTransactionOutput {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_transaction_output_creation() {
        let features = LightweightOutputFeatures::default();
        let commitment = CompressedCommitment::new([1u8; 33]);
        let proof = Some(LightweightRangeProof::default());
        let script = LightweightScript::default();
        let sender_offset_public_key = CompressedPublicKey::new([2u8; 32]);
        let metadata_signature = LightweightSignature::default();
        let covenant = LightweightCovenant::default();
        let encrypted_data = EncryptedData::default();
        let minimum_value_promise = MicroMinotari::new(1000);

        let output = LightweightTransactionOutput::new(
            1,
            features.clone(),
            commitment.clone(),
            proof.clone(),
            script.clone(),
            sender_offset_public_key.clone(),
            metadata_signature.clone(),
            covenant.clone(),
            encrypted_data.clone(),
            minimum_value_promise,
        );

        assert_eq!(output.version(), 1);
        assert_eq!(output.features(), &features);
        assert_eq!(output.commitment(), &commitment);
        assert_eq!(output.proof(), proof.as_ref());
        assert_eq!(output.script(), &script);
        assert_eq!(output.sender_offset_public_key(), &sender_offset_public_key);
        assert_eq!(output.metadata_signature(), &metadata_signature);
        assert_eq!(output.covenant(), &covenant);
        assert_eq!(output.encrypted_data(), &encrypted_data);
        assert_eq!(output.minimum_value_promise(), minimum_value_promise);
    }

    #[test]
    fn test_transaction_output_default() {
        let output = LightweightTransactionOutput::default();
        assert_eq!(output.version(), 1);
        assert_eq!(output.minimum_value_promise(), MicroMinotari::new(0));
    }
} 