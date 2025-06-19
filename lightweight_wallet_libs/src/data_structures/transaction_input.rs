use crate::data_structures::types::{CompressedPublicKey, MicroMinotari};
use crate::errors::DataStructureError;
use borsh::{BorshDeserialize, BorshSerialize};
use zeroize::Zeroize;

/// Lightweight transaction input structure
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TransactionInput {
    /// Input version
    pub version: u8,
    /// Input features
    pub features: u8,
    /// Commitment to the output being spent
    pub commitment: [u8; 33],
    /// Script signature
    pub script_signature: [u8; 64],
    /// Sender offset public key
    pub sender_offset_public_key: CompressedPublicKey,
    /// Covenant
    pub covenant: Vec<u8>,
    /// Input metadata
    pub input_data: LightweightExecutionStack,
    /// Output hash
    pub output_hash: [u8; 32],
    /// Output features
    pub output_features: u8,
    /// Output metadata signature
    pub output_metadata_signature: [u8; 64],
    /// Maturity
    pub maturity: u64,
    /// Value
    pub value: MicroMinotari,
}

/// Lightweight execution stack for script execution
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LightweightExecutionStack {
    /// Stack items
    pub items: Vec<Vec<u8>>,
}

impl LightweightExecutionStack {
    /// Create a new empty execution stack
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Create an execution stack with items
    pub fn with_items(items: Vec<Vec<u8>>) -> Self {
        Self { items }
    }

    /// Push an item onto the stack
    pub fn push(&mut self, item: Vec<u8>) {
        self.items.push(item);
    }

    /// Pop an item from the stack
    pub fn pop(&mut self) -> Option<Vec<u8>> {
        self.items.pop()
    }

    /// Get the number of items in the stack
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the stack is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get an item by index
    pub fn get(&self, index: usize) -> Option<&Vec<u8>> {
        self.items.get(index)
    }
}

impl Default for LightweightExecutionStack {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionInput {
    /// Create a new transaction input
    pub fn new(
        version: u8,
        features: u8,
        commitment: [u8; 33],
        script_signature: [u8; 64],
        sender_offset_public_key: CompressedPublicKey,
        covenant: Vec<u8>,
        input_data: LightweightExecutionStack,
        output_hash: [u8; 32],
        output_features: u8,
        output_metadata_signature: [u8; 64],
        maturity: u64,
        value: MicroMinotari,
    ) -> Self {
        Self {
            version,
            features,
            commitment,
            script_signature,
            sender_offset_public_key,
            covenant,
            input_data,
            output_hash,
            output_features,
            output_metadata_signature,
            maturity,
            value,
        }
    }

    /// Get the commitment as a hex string
    pub fn commitment_hex(&self) -> String {
        hex::encode(self.commitment)
    }

    /// Get the output hash as a hex string
    pub fn output_hash_hex(&self) -> String {
        hex::encode(self.output_hash)
    }

    /// Get the script signature as a hex string
    pub fn script_signature_hex(&self) -> String {
        hex::encode(self.script_signature)
    }

    /// Get the output metadata signature as a hex string
    pub fn output_metadata_signature_hex(&self) -> String {
        hex::encode(self.output_metadata_signature)
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, DataStructureError> {
        borsh::to_vec(self).map_err(|e| DataStructureError::InvalidDataFormat(e.to_string()))
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DataStructureError> {
        borsh::from_slice(bytes).map_err(|e| DataStructureError::InvalidDataFormat(e.to_string()))
    }

    /// Serialize to hex string
    pub fn to_hex(&self) -> Result<String, DataStructureError> {
        let bytes = self.to_bytes()?;
        Ok(hex::encode(bytes))
    }

    /// Deserialize from hex string
    pub fn from_hex(hex_str: &str) -> Result<Self, DataStructureError> {
        let bytes = hex::decode(hex_str).map_err(|e| DataStructureError::InvalidDataFormat(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

impl Zeroize for TransactionInput {
    fn zeroize(&mut self) {
        self.commitment.zeroize();
        self.script_signature.zeroize();
        self.covenant.zeroize();
        self.input_data.items.iter_mut().for_each(|item| item.zeroize());
        self.output_hash.zeroize();
        self.output_metadata_signature.zeroize();
    }
}

impl Drop for TransactionInput {
    fn drop(&mut self) {
        self.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::types::MicroMinotari;

    #[test]
    fn test_transaction_input_creation() {
        let commitment = [0x01; 33];
        let script_signature = [0x02; 64];
        let sender_offset_public_key = CompressedPublicKey::new([0x03; 32]);
        let covenant = vec![0x03, 0x04, 0x05];
        let input_data = LightweightExecutionStack::with_items(vec![vec![0x06, 0x07]]);
        let output_hash = [0x08; 32];
        let output_metadata_signature = [0x09; 64];
        let value = MicroMinotari::from(1000);

        let input = TransactionInput::new(
            1,
            2,
            commitment,
            script_signature,
            sender_offset_public_key,
            covenant,
            input_data,
            output_hash,
            3,
            output_metadata_signature,
            100,
            value,
        );

        assert_eq!(input.version, 1);
        assert_eq!(input.features, 2);
        assert_eq!(input.commitment, commitment);
        assert_eq!(input.script_signature, script_signature);
        assert_eq!(input.covenant, vec![0x03, 0x04, 0x05]);
        assert_eq!(input.output_hash, output_hash);
        assert_eq!(input.output_features, 3);
        assert_eq!(input.output_metadata_signature, output_metadata_signature);
        assert_eq!(input.maturity, 100);
        assert_eq!(input.value, value);
    }

    #[test]
    fn test_hex_conversion() {
        let input = TransactionInput::new(
            1,
            2,
            [0x01; 33],
            [0x02; 64],
            CompressedPublicKey::new([0x03; 32]),
            vec![0x03],
            LightweightExecutionStack::new(),
            [0x04; 32],
            5,
            [0x06; 64],
            100,
            MicroMinotari::from(1000),
        );

        let hex_str = input.to_hex().unwrap();
        let decoded = TransactionInput::from_hex(&hex_str).unwrap();

        assert_eq!(input, decoded);
    }

    #[test]
    fn test_lightweight_execution_stack() {
        let mut stack = LightweightExecutionStack::new();
        assert!(stack.is_empty());

        stack.push(vec![0x01, 0x02]);
        assert_eq!(stack.len(), 1);
        assert!(!stack.is_empty());

        stack.push(vec![0x03, 0x04]);
        assert_eq!(stack.len(), 2);

        let item = stack.pop().unwrap();
        assert_eq!(item, vec![0x03, 0x04]);
        assert_eq!(stack.len(), 1);

        assert_eq!(stack.get(0), Some(&vec![0x01, 0x02]));
        assert_eq!(stack.get(1), None);
    }
} 