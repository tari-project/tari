// Copyright 2025. The Tari Project
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
use semver::Version;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, CompressedSignature, FixedHash},
};

use crate::{
    MicroMinotari,
    transaction_components::{KernelFeatures, MemoField, OutputFeatures, Transaction, TransactionError, WalletOutput},
};

/// Version 4 had no payload integrity signature.
/// Version 5 adds `payload_signature` to all `Prepare*` results so the offline
/// signer can verify the payload was not tampered with in transit.
const SUPPORTED_VERSION: &str = "5.0.0";

pub fn get_supported_versions() -> Vec<Version> {
    vec![Version::parse(SUPPORTED_VERSION).unwrap()]
}

pub fn get_latest_version() -> Version {
    Version::parse(SUPPORTED_VERSION).unwrap()
}

pub trait HasVersion {
    fn get_version(&self) -> &Version;
}

pub trait TransactionResult: HasVersion + Serialize + DeserializeOwned + Sized {
    fn from_json(s: &str) -> Result<Self, TransactionError> {
        let value: serde_json::Value =
            serde_json::from_str(s).map_err(|e| TransactionError::SerializationError(e.to_string()))?;
        let version = value
            .get("version")
            .ok_or_else(|| TransactionError::SerializationError("Missing version".into()))?;
        let version: Version =
            serde_json::from_value(version.clone()).map_err(|e| TransactionError::SerializationError(e.to_string()))?;
        if !get_supported_versions().contains(&version) {
            return Err(TransactionError::SerializationError(format!(
                "Unsupported version. Expected '{}', got '{}'",
                get_supported_versions().first().expect("at least one version"),
                version
            )));
        }

        let deserialized_obj: Self =
            serde_json::from_str(s).map_err(|e| TransactionError::SerializationError(e.to_string()))?;

        Ok(deserialized_obj)
    }

    fn to_json(&self) -> Result<String, TransactionError> {
        serde_json::to_string(&self).map_err(|e| TransactionError::SerializationError(e.to_string()))
    }
}

/// A domain-separated Schnorr signature produced by the view wallet over the canonical
/// JSON bytes of the `Prepare*` payload.  The offline signer verifies this before using
/// the spend key, ensuring that any in-transit tampering (recipient swap, amount change,
/// input substitution, …) is detected and the signing operation is aborted.
///
/// The `view_public_key` field lets the offline signer cross-check that the payload
/// was produced by the expected wallet instance (the one whose view key matches the
/// key stored in the offline signer's keystore).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PayloadIntegritySignature {
    /// The view public key of the wallet that prepared this payload.
    pub view_public_key: CompressedPublicKey,
    /// Schnorr signature over the canonical payload bytes (all JSON fields except
    /// `payload_signature` itself), produced with the view private key.
    pub signature: CompressedSignature,
}

/// Returns the canonical bytes of a serialised `Prepare*` JSON payload that are
/// covered by the [`PayloadIntegritySignature`].
///
/// Concretely: deserialise `json_str` as a JSON object, remove the
/// `payload_signature` key (so the signed data is stable regardless of whether
/// the field is present), and re-serialise to bytes.  Using `serde_json::Value`
/// as an intermediary guarantees key-ordering is preserved exactly as the
/// serialiser produced it for all other fields.
pub fn canonical_payload_bytes(json_str: &str) -> Result<Vec<u8>, TransactionError> {
    // serde_json::Value uses BTreeMap (sorted keys) by default, giving stable
    // byte output regardless of the order in which fields appear in `json_str`.
    // NOTE: if the `preserve_order` feature of serde_json is ever enabled this
    // invariant still holds because both the prepare side and the sign side call
    // this function on JSON produced by the same `to_json()` serialiser, so the
    // field ordering is identical on both sides.
    let mut value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| TransactionError::SerializationError(e.to_string()))?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("payload_signature");
    }
    serde_json::to_vec(&value).map_err(|e| TransactionError::SerializationError(e.to_string()))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaymentRecipient {
    pub amount: MicroMinotari,
    pub output_features: OutputFeatures,
    pub address: TariAddress,
    pub payment_id: MemoField,
}

/// Transaction metadata, this includes all the fields that needs to be signed on the kernel
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    pub fee: MicroMinotari,
    /// The earliest block this transaction can be mined
    pub lock_height: u64,
    /// The kernel features
    pub kernel_features: KernelFeatures,
    /// optional burn commitment if present
    pub burn_commitment: Option<CompressedCommitment>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OneSidedTransactionInfo {
    /// Payment ID
    pub payment_id: MemoField,
    /// Recipient
    pub recipients: Vec<PaymentRecipient>,
    /// All transaction inputs inputs.
    pub inputs: Vec<WalletOutput>,
    /// The recipient's outputs.
    pub outputs: Vec<WalletOutput>,
    pub fee: MicroMinotari,
    pub fee_per_gram: MicroMinotari,
    /// Sender address
    pub sender_address: TariAddress,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OneSidedMultisigTransactionInfo {
    #[serde(flatten)]
    pub base: OneSidedTransactionInfo,
    pub public_keys: Vec<CompressedPublicKey>,
    pub party_number: u8,
}

impl core::ops::Deref for OneSidedMultisigTransactionInfo {
    type Target = OneSidedTransactionInfo;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl core::ops::DerefMut for OneSidedMultisigTransactionInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl OneSidedMultisigTransactionInfo {
    pub fn new(base: OneSidedTransactionInfo, public_keys: Vec<CompressedPublicKey>, party_number: u8) -> Self {
        Self {
            base,
            public_keys,
            party_number,
        }
    }

    pub fn into_base(self) -> OneSidedTransactionInfo {
        self.base
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrepareOneSidedTransactionForSigningResult {
    pub version: Version,
    pub tx_id: TxId,
    pub info: OneSidedTransactionInfo,
    /// Integrity signature produced by the online view wallet over the canonical
    /// payload bytes.  The offline signer MUST verify this before signing.
    pub payload_signature: PayloadIntegritySignature,
}

impl TransactionResult for PrepareOneSidedTransactionForSigningResult {}

impl HasVersion for PrepareOneSidedTransactionForSigningResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrepareDepositMultisigTransactionResult {
    pub version: Version,
    pub tx_id: TxId,
    pub info: OneSidedMultisigTransactionInfo,
    /// Integrity signature produced by the online view wallet over the canonical
    /// payload bytes.  The offline signer MUST verify this before signing.
    pub payload_signature: PayloadIntegritySignature,
}

impl TransactionResult for PrepareDepositMultisigTransactionResult {}

impl HasVersion for PrepareDepositMultisigTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrepareWithdrawMultisigTransactionResult {
    pub version: Version,
    pub tx_id: TxId,
    pub info: OneSidedTransactionInfo,
    /// Integrity signature produced by the online view wallet over the canonical
    /// payload bytes.  The offline signer MUST verify this before signing.
    pub payload_signature: PayloadIntegritySignature,
}

impl TransactionResult for PrepareWithdrawMultisigTransactionResult {}

impl HasVersion for PrepareWithdrawMultisigTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub sent_hashes: Vec<FixedHash>,
    pub outputs: Vec<WalletOutput>,
    pub change_hashes: Vec<FixedHash>,
    pub change_output: Option<WalletOutput>,
    pub tx_id: TxId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedOneSidedTransactionResult {
    pub version: Version,
    pub request: PrepareOneSidedTransactionForSigningResult,
    pub signed_transaction: SignedTransaction,
}

impl TransactionResult for SignedOneSidedTransactionResult {}

impl HasVersion for SignedOneSidedTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedOneSidedDepositMultisigTransactionResult {
    pub version: Version,
    pub request: PrepareDepositMultisigTransactionResult,
    pub signed_transaction: SignedTransaction,
}

impl TransactionResult for SignedOneSidedDepositMultisigTransactionResult {}

impl HasVersion for SignedOneSidedDepositMultisigTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedOneSidedWithdrawMultisigTransactionResult {
    pub version: Version,
    pub request: PrepareWithdrawMultisigTransactionResult,
    pub signed_transaction: SignedTransaction,
}

impl TransactionResult for SignedOneSidedWithdrawMultisigTransactionResult {}

impl HasVersion for SignedOneSidedWithdrawMultisigTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}
