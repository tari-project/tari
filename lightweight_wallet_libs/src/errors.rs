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

use thiserror::Error;

/// Main error type for the lightweight wallet library
#[derive(Debug, Error)]
pub enum LightweightWalletError {
    #[error("Data structure error: {0}")]
    DataStructureError(#[from] DataStructureError),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] SerializationError),
    
    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationError),
    
    #[error("Key management error: {0}")]
    KeyManagementError(#[from] KeyManagementError),
    
    #[error("Scanning error: {0}")]
    ScanningError(#[from] ScanningError),
    
    #[error("Encryption error: {0}")]
    EncryptionError(#[from] EncryptionError),
    
    #[error("Hex error: {0}")]
    HexError(#[from] crate::hex_utils::HexError),
    
    #[error("Conversion error: {0}")]
    ConversionError(String),
    
    #[error("Invalid argument: {argument} = {value}. {message}")]
    InvalidArgument {
        argument: String,
        value: String,
        message: String,
    },
    
    #[error("Operation not supported: {0}")]
    OperationNotSupported(String),
    
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    
    #[error("Insufficient funds: {0}")]
    InsufficientFunds(String),
    
    #[error("Timeout error: {0}")]
    Timeout(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Errors related to data structure operations
#[derive(Debug, Error)]
pub enum DataStructureError {
    #[error("Invalid output version: {0}")]
    InvalidOutputVersion(String),
    
    #[error("Invalid output value: {0}")]
    InvalidOutputValue(String),
    
    #[error("Invalid key identifier: {0}")]
    InvalidKeyId(String),
    
    #[error("Invalid output features: {0}")]
    InvalidFeatures(String),
    
    #[error("Invalid script: {0}")]
    InvalidScript(String),
    
    #[error("Invalid covenant: {0}")]
    InvalidCovenant(String),
    
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    
    #[error("Invalid range proof: {0}")]
    InvalidRangeProof(String),
    
    #[error("Invalid commitment: {0}")]
    InvalidCommitment(String),
    
    #[error("Invalid payment ID: {0}")]
    InvalidPaymentId(String),
    
    #[error("Invalid transaction output: {0}")]
    InvalidTransactionOutput(String),
    
    #[error("Invalid wallet output: {0}")]
    InvalidWalletOutput(String),
    
    #[error("Invalid encrypted data: {0}")]
    InvalidEncryptedData(String),
    
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),
    
    #[error("Data too large: expected max {max}, got {actual}")]
    DataTooLarge { max: usize, actual: usize },
    
    #[error("Data too small: expected min {min}, got {actual}")]
    DataTooSmall { min: usize, actual: usize },
    
    #[error("Incorrect data length: {0}")]
    IncorrectLength(String),
    
    #[error("Missing required field: {0}")]
    MissingField(String),
    
    #[error("Duplicate data: {0}")]
    DuplicateData(String),
    
    #[error("Invalid data format: {0}")]
    InvalidDataFormat(String),
}

/// Errors related to serialization and deserialization
#[derive(Debug, Error)]
pub enum SerializationError {
    #[error("Serde serialization error: {0}")]
    SerdeSerializationError(String),
    
    #[error("Serde deserialization error: {0}")]
    SerdeDeserializationError(String),
    
    #[error("Borsh serialization error: {0}")]
    BorshSerializationError(String),
    
    #[error("Borsh deserialization error: {0}")]
    BorshDeserializationError(String),
    
    #[error("Hex encoding error: {0}")]
    HexEncodingError(String),
    
    #[error("Hex decoding error: {0}")]
    HexDecodingError(String),
    
    #[error("Base64 encoding error: {0}")]
    Base64EncodingError(String),
    
    #[error("Base64 decoding error: {0}")]
    Base64DecodingError(String),
    
    #[error("JSON serialization error: {0}")]
    JsonSerializationError(String),
    
    #[error("JSON deserialization error: {0}")]
    JsonDeserializationError(String),
    
    #[error("Protobuf serialization error: {0}")]
    ProtobufSerializationError(String),
    
    #[error("Protobuf deserialization error: {0}")]
    ProtobufDeserializationError(String),
    
    #[error("Buffer overflow: {0}")]
    BufferOverflow(String),
    
    #[error("Buffer underflow: {0}")]
    BufferUnderflow(String),
    
    #[error("Invalid encoding: {0}")]
    InvalidEncoding(String),
}

/// Errors related to validation operations
#[derive(Debug, Clone, Error)]
pub enum ValidationError {
    #[error("Range proof validation failed: {0}")]
    RangeProofValidationFailed(String),
    
    #[error("Signature validation failed: {0}")]
    SignatureValidationFailed(String),
    
    #[error("Commitment validation failed: {0}")]
    CommitmentValidationFailed(String),
    
    #[error("Script validation failed: {0}")]
    ScriptValidationFailed(String),
    
    #[error("Covenant validation failed: {0}")]
    CovenantValidationFailed(String),
    
    #[error("Output validation failed: {0}")]
    OutputValidationFailed(String),
    
    #[error("Input validation failed: {0}")]
    InputValidationFailed(String),
    
    #[error("Transaction validation failed: {0}")]
    TransactionValidationFailed(String),
    
    #[error("Block validation failed: {0}")]
    BlockValidationFailed(String),
    
    #[error("Value validation failed: {0}")]
    ValueValidationFailed(String),
    
    #[error("Key validation failed: {0}")]
    KeyValidationFailed(String),
    
    #[error("Address validation failed: {0}")]
    AddressValidationFailed(String),
    
    #[error("Network validation failed: {0}")]
    NetworkValidationFailed(String),
    
    #[error("Version validation failed: {0}")]
    VersionValidationFailed(String),
    
    #[error("Integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    
    #[error("Consensus validation failed: {0}")]
    ConsensusValidationFailed(String),
    
    #[error("Script signature validation failed: {0}")]
    ScriptSignatureValidationFailed(String),
    
    #[error("Metadata signature validation failed: {0}")]
    MetadataSignatureValidationFailed(String),
}

/// Errors related to key management operations
#[derive(Debug, Error)]
pub enum KeyManagementError {
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),
    
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
    
    #[error("Invalid key derivation path: {0}")]
    InvalidKeyDerivationPath(String),
    
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),
    
    #[error("Key import failed: {0}")]
    KeyImportFailed(String),
    
    #[error("Key export failed: {0}")]
    KeyExportFailed(String),
    
    #[error("Key generation failed: {0}")]
    KeyGenerationFailed(String),
    
    #[error("Key recovery failed: {0}")]
    KeyRecoveryFailed(String),
    
    #[error("Stealth address recovery failed: {0}")]
    StealthAddressRecoveryFailed(String),
    
    #[error("Mnemonic error: {0}")]
    MnemonicError(String),
    
    #[error("Seed phrase error: {0}")]
    SeedPhraseError(String),
    
    #[error("Key storage error: {0}")]
    KeyStorageError(String),
    
    #[error("Key encryption error: {0}")]
    KeyEncryptionError(String),
    
    #[error("Key decryption error: {0}")]
    KeyDecryptionError(String),
    
    #[error("Key backup error: {0}")]
    KeyBackupError(String),
    
    #[error("Key restore error: {0}")]
    KeyRestoreError(String),
    
    #[error("Key migration error: {0}")]
    KeyMigrationError(String),
    
    #[error("Key version error: {0}")]
    KeyVersionError(String),
}

/// Errors related to UTXO scanning operations
#[derive(Debug, Error)]
pub enum ScanningError {
    #[error("Blockchain connection failed: {0}")]
    BlockchainConnectionFailed(String),
    
    #[error("Block not found: {0}")]
    BlockNotFound(String),
    
    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),
    
    #[error("Output not found: {0}")]
    OutputNotFound(String),
    
    #[error("Scan interrupted: {0}")]
    ScanInterrupted(String),
    
    #[error("Scan timeout: {0}")]
    ScanTimeout(String),
    
    #[error("Invalid block height: {0}")]
    InvalidBlockHeight(String),
    
    #[error("Invalid block hash: {0}")]
    InvalidBlockHash(String),
    
    #[error("Invalid transaction hash: {0}")]
    InvalidTransactionHash(String),
    
    #[error("Invalid output hash: {0}")]
    InvalidOutputHash(String),
    
    #[error("Scan progress error: {0}")]
    ScanProgressError(String),
    
    #[error("Scan resume failed: {0}")]
    ScanResumeFailed(String),
    
    #[error("Scan state corrupted: {0}")]
    ScanStateCorrupted(String),
    
    #[error("Scan configuration error: {0}")]
    ScanConfigurationError(String),
    
    #[error("Scan memory error: {0}")]
    ScanMemoryError(String),
    
    #[error("Scan performance error: {0}")]
    ScanPerformanceError(String),
    
    #[error("Scan data corruption: {0}")]
    ScanDataCorruption(String),
    
    #[error("Scan network error: {0}")]
    ScanNetworkError(String),
    
    #[error("Scan rate limit exceeded: {0}")]
    ScanRateLimitExceeded(String),
}

/// Errors related to encryption and decryption operations
#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("Invalid encryption key: {0}")]
    InvalidEncryptionKey(String),
    
    #[error("Invalid decryption key: {0}")]
    InvalidDecryptionKey(String),
    
    #[error("Invalid nonce: {0}")]
    InvalidNonce(String),
    
    #[error("Invalid ciphertext: {0}")]
    InvalidCiphertext(String),
    
    #[error("Invalid plaintext: {0}")]
    InvalidPlaintext(String),
    
    #[error("Invalid tag: {0}")]
    InvalidTag(String),
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),
    
    #[error("Invalid encryption parameters: {0}")]
    InvalidEncryptionParameters(String),
    
    #[error("Encryption version error: {0}")]
    EncryptionVersionError(String),
    
    #[error("Encryption algorithm error: {0}")]
    EncryptionAlgorithmError(String),
    
    #[error("Encryption mode error: {0}")]
    EncryptionModeError(String),
    
    #[error("Encryption padding error: {0}")]
    EncryptionPaddingError(String),
    
    #[error("Encryption block size error: {0}")]
    EncryptionBlockSizeError(String),
    
    #[error("Encryption initialization error: {0}")]
    EncryptionInitializationError(String),
    
    #[error("Encryption finalization error: {0}")]
    EncryptionFinalizationError(String),
}

// Conversion implementations for external error types
impl From<hex::FromHexError> for SerializationError {
    fn from(err: hex::FromHexError) -> Self {
        SerializationError::HexDecodingError(err.to_string())
    }
}

impl From<std::io::Error> for SerializationError {
    fn from(err: std::io::Error) -> Self {
        SerializationError::BufferOverflow(err.to_string())
    }
}

impl From<String> for LightweightWalletError {
    fn from(err: String) -> Self {
        LightweightWalletError::InternalError(err)
    }
}

impl From<&str> for LightweightWalletError {
    fn from(err: &str) -> Self {
        LightweightWalletError::InternalError(err.to_string())
    }
}

// Convenience methods for creating common errors
impl LightweightWalletError {
    /// Create an invalid argument error
    pub fn invalid_argument(argument: &str, value: &str, message: &str) -> Self {
        Self::InvalidArgument {
            argument: argument.to_string(),
            value: value.to_string(),
            message: message.to_string(),
        }
    }
    
    /// Create a resource not found error
    pub fn not_found(resource: &str) -> Self {
        Self::ResourceNotFound(resource.to_string())
    }
    
    /// Create an operation not supported error
    pub fn not_supported(operation: &str) -> Self {
        Self::OperationNotSupported(operation.to_string())
    }
    
    /// Create an insufficient funds error
    pub fn insufficient_funds(details: &str) -> Self {
        Self::InsufficientFunds(details.to_string())
    }
    
    /// Create a timeout error
    pub fn timeout(operation: &str) -> Self {
        Self::Timeout(operation.to_string())
    }
    
    /// Create a network error
    pub fn network_error(details: &str) -> Self {
        Self::NetworkError(details.to_string())
    }
    
    /// Create an internal error
    pub fn internal_error(details: &str) -> Self {
        Self::InternalError(details.to_string())
    }
}

impl DataStructureError {
    /// Create an invalid output version error
    pub fn invalid_output_version(version: &str) -> Self {
        Self::InvalidOutputVersion(version.to_string())
    }
    
    /// Create an invalid output value error
    pub fn invalid_output_value(value: &str) -> Self {
        Self::InvalidOutputValue(value.to_string())
    }
    
    /// Create a data too large error
    pub fn data_too_large(max: usize, actual: usize) -> Self {
        Self::DataTooLarge { max, actual }
    }
    
    /// Create a data too small error
    pub fn data_too_small(min: usize, actual: usize) -> Self {
        Self::DataTooSmall { min, actual }
    }
    
    /// Create a missing field error
    pub fn missing_field(field: &str) -> Self {
        Self::MissingField(field.to_string())
    }
}

impl SerializationError {
    /// Create a hex encoding error
    pub fn hex_encoding_error(details: &str) -> Self {
        Self::HexEncodingError(details.to_string())
    }
    
    /// Create a hex decoding error
    pub fn hex_decoding_error(details: &str) -> Self {
        Self::HexDecodingError(details.to_string())
    }
    
    /// Create a serde serialization error
    pub fn serde_serialization_error(details: &str) -> Self {
        Self::SerdeSerializationError(details.to_string())
    }
    
    /// Create a serde deserialization error
    pub fn serde_deserialization_error(details: &str) -> Self {
        Self::SerdeDeserializationError(details.to_string())
    }
}

impl ValidationError {
    /// Create a range proof validation error
    pub fn range_proof_validation_failed(details: &str) -> Self {
        ValidationError::RangeProofValidationFailed(details.to_string())
    }
    
    /// Create a signature validation error
    pub fn signature_validation_failed(details: &str) -> Self {
        ValidationError::SignatureValidationFailed(details.to_string())
    }
    
    /// Create a metadata signature validation error
    pub fn metadata_signature_validation_failed(details: &str) -> Self {
        ValidationError::MetadataSignatureValidationFailed(details.to_string())
    }
    
    /// Create a script signature validation error
    pub fn script_signature_validation_failed(details: &str) -> Self {
        ValidationError::ScriptSignatureValidationFailed(details.to_string())
    }
    
    /// Create a commitment validation error
    pub fn commitment_validation_failed(details: &str) -> Self {
        ValidationError::CommitmentValidationFailed(details.to_string())
    }
}

impl KeyManagementError {
    /// Create a key not found error
    pub fn key_not_found(key_id: &str) -> Self {
        Self::KeyNotFound(key_id.to_string())
    }
    
    /// Create a key derivation failed error
    pub fn key_derivation_failed(details: &str) -> Self {
        Self::KeyDerivationFailed(details.to_string())
    }
    
    /// Create a stealth address recovery error
    pub fn stealth_address_recovery_failed(details: &str) -> Self {
        Self::StealthAddressRecoveryFailed(details.to_string())
    }
}

impl ScanningError {
    /// Create a blockchain connection error
    pub fn blockchain_connection_failed(details: &str) -> Self {
        Self::BlockchainConnectionFailed(details.to_string())
    }
    
    /// Create a block not found error
    pub fn block_not_found(block_id: &str) -> Self {
        Self::BlockNotFound(block_id.to_string())
    }
    
    /// Create a scan timeout error
    pub fn scan_timeout(operation: &str) -> Self {
        Self::ScanTimeout(operation.to_string())
    }
}

impl EncryptionError {
    /// Create an encryption failed error
    pub fn encryption_failed(details: &str) -> Self {
        Self::EncryptionFailed(details.to_string())
    }
    
    /// Create a decryption failed error
    pub fn decryption_failed(details: &str) -> Self {
        Self::DecryptionFailed(details.to_string())
    }
    
    /// Create an authentication failed error
    pub fn authentication_failed(details: &str) -> Self {
        Self::AuthenticationFailed(details.to_string())
    }
} 