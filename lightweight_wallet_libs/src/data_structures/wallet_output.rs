// Copyright 2022 The Tari Project
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

use crate::data_structures::{
    encrypted_data::EncryptedData,
    payment_id::PaymentId,
    types::{CompressedPublicKey, MicroMinotari},
};
use crate::hex_utils::{HexEncodable, HexValidatable, HexError};
use borsh::{BorshSerialize, BorshDeserialize};
use hex::ToHex;
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt::{Debug, Formatter},
};

/// Simplified key identifier for lightweight wallet operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum LightweightKeyId {
    /// A simple string identifier for keys
    String(String),
    /// A public key identifier
    PublicKey(CompressedPublicKey),
    /// Zero key (for special cases)
    Zero,
}

impl Default for LightweightKeyId {
    fn default() -> Self {
        LightweightKeyId::Zero
    }
}

impl std::fmt::Display for LightweightKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LightweightKeyId::String(s) => write!(f, "{}", s),
            LightweightKeyId::PublicKey(pk) => write!(f, "{}", pk),
            LightweightKeyId::Zero => write!(f, "zero"),
        }
    }
}

/// Simplified output features for lightweight wallet operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct LightweightOutputFeatures {
    /// Output type (payment, coinbase, burn, etc.)
    pub output_type: LightweightOutputType,
    /// Maturity height (when the output can be spent)
    pub maturity: u64,
    /// Range proof type
    pub range_proof_type: LightweightRangeProofType,
}

impl LightweightOutputFeatures {
    /// Get the serialized bytes of the output features
    pub fn bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).unwrap_or_default()
    }
}

impl Default for LightweightOutputFeatures {
    fn default() -> Self {
        Self {
            output_type: LightweightOutputType::Payment,
            maturity: 0,
            range_proof_type: LightweightRangeProofType::BulletProofPlus,
        }
    }
}

/// Simplified output types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum LightweightOutputType {
    Payment,
    Coinbase,
    Burn,
    ValidatorNodeRegistration,
    CodeTemplateRegistration,
}

impl Default for LightweightOutputType {
    fn default() -> Self {
        LightweightOutputType::Payment
    }
}

/// Simplified range proof types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum LightweightRangeProofType {
    BulletProofPlus,
    RevealedValue,
}

impl Default for LightweightRangeProofType {
    fn default() -> Self {
        LightweightRangeProofType::BulletProofPlus
    }
}

/// Simplified script for lightweight wallet operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct LightweightScript {
    /// Script bytes
    pub bytes: Vec<u8>,
}

impl Default for LightweightScript {
    fn default() -> Self {
        Self { bytes: vec![] }
    }
}

/// Simplified covenant for lightweight wallet operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct LightweightCovenant {
    /// Covenant bytes
    pub bytes: Vec<u8>,
}

impl Default for LightweightCovenant {
    fn default() -> Self {
        Self { bytes: vec![] }
    }
}

/// Simplified execution stack for lightweight wallet operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct LightweightExecutionStack {
    /// Stack items as bytes
    pub items: Vec<Vec<u8>>,
}

impl LightweightExecutionStack {
    /// Get the serialized bytes of the execution stack
    pub fn bytes(&self) -> Vec<u8> {
        borsh::to_vec(&self.items).unwrap_or_default()
    }
}

impl Default for LightweightExecutionStack {
    fn default() -> Self {
        Self { items: vec![] }
    }
}

/// Simplified signature for lightweight wallet operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct LightweightSignature {
    /// Signature bytes
    pub bytes: Vec<u8>,
}

impl Default for LightweightSignature {
    fn default() -> Self {
        Self { bytes: vec![] }
    }
}

/// Simplified range proof for lightweight wallet operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct LightweightRangeProof {
    /// Range proof bytes
    pub bytes: Vec<u8>,
}

impl Default for LightweightRangeProof {
    fn default() -> Self {
        Self { bytes: vec![] }
    }
}

/// A lightweight wallet output where the value and spending key are known
/// This is a simplified version of the full WalletOutput for use in lightweight wallets
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct LightweightWalletOutput {
    /// Output version
    pub version: u8,
    /// Output value in Micro Minotari
    pub value: MicroMinotari,
    /// Spending key identifier
    pub spending_key_id: LightweightKeyId,
    /// Output features
    pub features: LightweightOutputFeatures,
    /// Script
    pub script: LightweightScript,
    /// Covenant
    pub covenant: LightweightCovenant,
    /// Input data (execution stack)
    pub input_data: LightweightExecutionStack,
    /// Script key identifier
    pub script_key_id: LightweightKeyId,
    /// Sender offset public key
    pub sender_offset_public_key: CompressedPublicKey,
    /// Metadata signature
    pub metadata_signature: LightweightSignature,
    /// Script lock height
    pub script_lock_height: u64,
    /// Encrypted data
    pub encrypted_data: EncryptedData,
    /// Minimum value promise
    pub minimum_value_promise: MicroMinotari,
    /// Range proof (optional)
    pub range_proof: Option<LightweightRangeProof>,
    /// Payment ID
    pub payment_id: PaymentId,
}

impl LightweightWalletOutput {
    /// Creates a new lightweight wallet output
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: u8,
        value: MicroMinotari,
        spending_key_id: LightweightKeyId,
        features: LightweightOutputFeatures,
        script: LightweightScript,
        input_data: LightweightExecutionStack,
        script_key_id: LightweightKeyId,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: LightweightSignature,
        script_lock_height: u64,
        covenant: LightweightCovenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<LightweightRangeProof>,
        payment_id: PaymentId,
    ) -> Self {
        Self {
            version,
            value,
            spending_key_id,
            features,
            script,
            input_data,
            script_key_id,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            range_proof,
            payment_id,
        }
    }

    /// Creates a new lightweight wallet output with default values
    pub fn new_default(
        value: MicroMinotari,
        spending_key_id: LightweightKeyId,
        script_key_id: LightweightKeyId,
        sender_offset_public_key: CompressedPublicKey,
        encrypted_data: EncryptedData,
        payment_id: PaymentId,
    ) -> Self {
        Self {
            version: 1, // Current version
            value,
            spending_key_id,
            features: LightweightOutputFeatures::default(),
            script: LightweightScript::default(),
            input_data: LightweightExecutionStack::default(),
            script_key_id,
            sender_offset_public_key,
            metadata_signature: LightweightSignature::default(),
            script_lock_height: 0,
            covenant: LightweightCovenant::default(),
            encrypted_data,
            minimum_value_promise: MicroMinotari::new(0),
            range_proof: None,
            payment_id,
        }
    }

    /// Get the output value
    pub fn value(&self) -> MicroMinotari {
        self.value
    }

    /// Get the spending key ID
    pub fn spending_key_id(&self) -> &LightweightKeyId {
        &self.spending_key_id
    }

    /// Get the script key ID
    pub fn script_key_id(&self) -> &LightweightKeyId {
        &self.script_key_id
    }

    /// Get the encrypted data
    pub fn encrypted_data(&self) -> &EncryptedData {
        &self.encrypted_data
    }

    /// Get the payment ID
    pub fn payment_id(&self) -> &PaymentId {
        &self.payment_id
    }

    /// Check if this is a coinbase output
    pub fn is_coinbase(&self) -> bool {
        matches!(self.features.output_type, LightweightOutputType::Coinbase)
    }

    /// Check if this is a burn output
    pub fn is_burn(&self) -> bool {
        matches!(self.features.output_type, LightweightOutputType::Burn)
    }

    /// Get the maturity height
    pub fn maturity(&self) -> u64 {
        self.features.maturity
    }

    /// Get the script lock height
    pub fn script_lock_height(&self) -> u64 {
        self.script_lock_height
    }

    /// Check if the output is mature at the given block height
    pub fn is_mature_at(&self, block_height: u64) -> bool {
        block_height >= self.features.maturity
    }

    /// Check if the script is unlocked at the given block height
    pub fn is_script_unlocked_at(&self, block_height: u64) -> bool {
        block_height >= self.script_lock_height
    }

    /// Check if the output can be spent at the given block height
    pub fn can_be_spent_at(&self, block_height: u64) -> bool {
        self.is_mature_at(block_height) && self.is_script_unlocked_at(block_height)
    }

    /// Get the range proof type
    pub fn range_proof_type(&self) -> &LightweightRangeProofType {
        &self.features.range_proof_type
    }

    /// Get the output type
    pub fn output_type(&self) -> &LightweightOutputType {
        &self.features.output_type
    }

    /// Get the minimum value promise
    pub fn minimum_value_promise(&self) -> MicroMinotari {
        self.minimum_value_promise
    }

    /// Get the sender offset public key
    pub fn sender_offset_public_key(&self) -> &CompressedPublicKey {
        &self.sender_offset_public_key
    }

    /// Get the metadata signature
    pub fn metadata_signature(&self) -> &LightweightSignature {
        &self.metadata_signature
    }

    /// Get the script
    pub fn script(&self) -> &LightweightScript {
        &self.script
    }

    /// Get the covenant
    pub fn covenant(&self) -> &LightweightCovenant {
        &self.covenant
    }

    /// Get the input data
    pub fn input_data(&self) -> &LightweightExecutionStack {
        &self.input_data
    }

    /// Get the range proof
    pub fn range_proof(&self) -> Option<&LightweightRangeProof> {
        self.range_proof.as_ref()
    }

    /// Set the range proof
    pub fn set_range_proof(&mut self, range_proof: LightweightRangeProof) {
        self.range_proof = Some(range_proof);
    }

    /// Remove the range proof
    pub fn remove_range_proof(&mut self) {
        self.range_proof = None;
    }

    /// Update the encrypted data
    pub fn update_encrypted_data(&mut self, encrypted_data: EncryptedData) {
        self.encrypted_data = encrypted_data;
    }

    /// Update the payment ID
    pub fn update_payment_id(&mut self, payment_id: PaymentId) {
        self.payment_id = payment_id;
    }

    /// Get the output version
    pub fn version(&self) -> u8 {
        self.version
    }
}

impl PartialOrd<LightweightWalletOutput> for LightweightWalletOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LightweightWalletOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        // Primary sort by maturity, then by value
        self.features.maturity
            .cmp(&other.features.maturity)
            .then(self.value.cmp(&other.value))
    }
}

impl Debug for LightweightWalletOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LightweightWalletOutput")
            .field("version", &self.version)
            .field("value", &self.value)
            .field("spending_key_id", &self.spending_key_id)
            .field("features", &self.features)
            .field("script_lock_height", &self.script_lock_height)
            .field("payment_id", &self.payment_id)
            .finish()
    }
}

impl Default for LightweightWalletOutput {
    fn default() -> Self {
        Self {
            version: 1,
            value: MicroMinotari::new(0),
            spending_key_id: LightweightKeyId::Zero,
            features: LightweightOutputFeatures::default(),
            script: LightweightScript::default(),
            input_data: LightweightExecutionStack::default(),
            script_key_id: LightweightKeyId::Zero,
            sender_offset_public_key: CompressedPublicKey::new([0u8; 32]),
            metadata_signature: LightweightSignature::default(),
            script_lock_height: 0,
            covenant: LightweightCovenant::default(),
            encrypted_data: EncryptedData::default(),
            minimum_value_promise: MicroMinotari::new(0),
            range_proof: None,
            payment_id: PaymentId::U256 { value: U256::from(12345) },
        }
    }
}

impl HexEncodable for LightweightWalletOutput {
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

impl HexValidatable for LightweightWalletOutput {}

impl HexEncodable for LightweightScript {
    fn to_hex(&self) -> String {
        self.bytes.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        Ok(Self { bytes })
    }
}

impl HexValidatable for LightweightScript {}

impl HexEncodable for LightweightSignature {
    fn to_hex(&self) -> String {
        self.bytes.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        Ok(Self { bytes })
    }
}

impl HexValidatable for LightweightSignature {}

impl HexEncodable for LightweightRangeProof {
    fn to_hex(&self) -> String {
        self.bytes.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        Ok(Self { bytes })
    }
}

impl HexValidatable for LightweightRangeProof {}

impl HexEncodable for LightweightCovenant {
    fn to_hex(&self) -> String {
        self.bytes.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        Ok(Self { bytes })
    }
}

impl HexValidatable for LightweightCovenant {}

impl HexEncodable for LightweightExecutionStack {
    fn to_hex(&self) -> String {
        // For execution stack, we'll serialize the items to a single hex string
        let bytes = borsh::to_vec(&self.items).unwrap_or_default();
        bytes.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        let items: Vec<Vec<u8>> = borsh::from_slice(&bytes).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        Ok(Self { items })
    }
}

impl HexValidatable for LightweightExecutionStack {}

#[cfg(test)]
mod test {
    use super::*;
    use primitive_types::U256;

    #[test]
    fn test_lightweight_wallet_output_creation() {
        let value = MicroMinotari::new(1000000);
        let spending_key_id = LightweightKeyId::String("spending_key_1".to_string());
        let script_key_id = LightweightKeyId::String("script_key_1".to_string());
        let sender_offset_public_key = CompressedPublicKey::new([0u8; 32]);
        let encrypted_data = EncryptedData::default();
        let payment_id = PaymentId::U256 { value: U256::from(12345) };

        let output = LightweightWalletOutput::new_default(
            value,
            spending_key_id.clone(),
            script_key_id.clone(),
            sender_offset_public_key.clone(),
            encrypted_data.clone(),
            payment_id.clone(),
        );

        assert_eq!(output.value(), value);
        assert_eq!(output.spending_key_id(), &spending_key_id);
        assert_eq!(output.script_key_id(), &script_key_id);
        assert_eq!(output.sender_offset_public_key(), &sender_offset_public_key);
        assert_eq!(output.encrypted_data(), &encrypted_data);
        assert_eq!(output.payment_id(), &payment_id);
        assert_eq!(output.version(), 1);
    }

    #[test]
    fn test_lightweight_wallet_output_maturity() {
        let mut output = LightweightWalletOutput::default();
        output.features.maturity = 100;

        assert!(!output.is_mature_at(50));
        assert!(output.is_mature_at(100));
        assert!(output.is_mature_at(150));
    }

    #[test]
    fn test_lightweight_wallet_output_script_lock() {
        let mut output = LightweightWalletOutput::default();
        output.script_lock_height = 200;

        assert!(!output.is_script_unlocked_at(150));
        assert!(output.is_script_unlocked_at(200));
        assert!(output.is_script_unlocked_at(250));
    }

    #[test]
    fn test_lightweight_wallet_output_can_be_spent() {
        let mut output = LightweightWalletOutput::default();
        output.features.maturity = 100;
        output.script_lock_height = 200;

        assert!(!output.can_be_spent_at(50));  // Neither mature nor unlocked
        assert!(!output.can_be_spent_at(150)); // Mature but not unlocked
        assert!(output.can_be_spent_at(250));  // Both mature and unlocked
    }

    #[test]
    fn test_lightweight_wallet_output_types() {
        let mut output = LightweightWalletOutput::default();
        
        // Test default (payment)
        assert!(!output.is_coinbase());
        assert!(!output.is_burn());

        // Test coinbase
        output.features.output_type = LightweightOutputType::Coinbase;
        assert!(output.is_coinbase());
        assert!(!output.is_burn());

        // Test burn
        output.features.output_type = LightweightOutputType::Burn;
        assert!(!output.is_coinbase());
        assert!(output.is_burn());
    }

    #[test]
    fn test_lightweight_wallet_output_ordering() {
        let mut output1 = LightweightWalletOutput::default();
        output1.features.maturity = 100;
        output1.value = MicroMinotari::new(1000000);

        let mut output2 = LightweightWalletOutput::default();
        output2.features.maturity = 200;
        output2.value = MicroMinotari::new(500000);

        // output1 should come before output2 due to lower maturity
        assert!(output1 < output2);

        let mut output3 = LightweightWalletOutput::default();
        output3.features.maturity = 100;
        output3.value = MicroMinotari::new(2000000);

        // output1 should come before output3 due to lower value (same maturity)
        assert!(output1 < output3);
    }
} 