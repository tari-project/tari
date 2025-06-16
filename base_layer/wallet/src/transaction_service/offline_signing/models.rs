use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedPublicKey, FixedHash},
};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{encrypted_data::PaymentId, OutputFeatures, Transaction},
        transaction_protocol::{sender::OutputPair, TransactionMetadata},
    },
};
use tari_script::TariScript;

use crate::transaction_service::error::TransactionServiceError;

const SUPPORTED_VERSION: &str = "1.0.0";

pub fn get_supported_version() -> Version {
    Version::parse(SUPPORTED_VERSION).unwrap()
}

pub trait HasVersion {
    fn get_version(&self) -> &Version;
}

pub trait TransactionResult: HasVersion + Serialize + DeserializeOwned + Sized {
    fn from_json(s: &str) -> Result<Self, TransactionServiceError> {
        let deserialized_obj: Self =
            serde_json::from_str(s).map_err(|e| TransactionServiceError::SerializationError(e.to_string()))?;

        let version = deserialized_obj.get_version();
        let supported_version = get_supported_version();
        if version != &supported_version {
            return Err(TransactionServiceError::SerializationError(format!(
                "Unsupported version. Expected '{}', got '{}'",
                supported_version.to_string(),
                version.to_string(),
            )));
        }

        Ok(deserialized_obj)
    }

    fn to_json(&self) -> Result<String, TransactionServiceError> {
        serde_json::to_string(&self).map_err(|e| TransactionServiceError::SerializationError(e.to_string()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaymentRecipient {
    pub amount: MicroMinotari,
    pub output_features: OutputFeatures,
    pub script: TariScript,
    pub sender_offset_public_key: CompressedPublicKey,
    pub covenant: Covenant,
    pub minimum_value_promise: MicroMinotari,
    pub ephemeral_public_key_nonce: CompressedPublicKey,
    pub address: TariAddress,
    pub use_stealth_address: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChangeOutput {
    pub output: OutputPair,
    pub encrypted_change_sender_offset_key: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OneSidedTransactionInfo {
    // Tip height
    pub last_seen_tip_height: Option<u64>,
    /// Payment ID
    pub payment_id: PaymentId,
    /// Recipient
    pub recipient: PaymentRecipient,
    /// The change output details. This may be None if no change is required.
    pub change_output: Option<ChangeOutput>,
    /// All transaction inputs inputs.
    pub inputs: Vec<OutputPair>,
    /// The recipient's outputs.
    pub outputs: Vec<OutputPair>,
    /// Details used to construct the transaction kernel.
    pub metadata: TransactionMetadata,
    /// Sender address
    pub sender_address: TariAddress,
    /// Encrypted commitment mask keys
    pub encrypted_commitment_mask_keys: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PrepareOneSidedTransactionForSigningResult {
    pub version: Version,
    pub tx_id: TxId,
    pub info: OneSidedTransactionInfo,
}

impl TransactionResult for PrepareOneSidedTransactionForSigningResult {}

impl HasVersion for PrepareOneSidedTransactionForSigningResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub sent_hashes: Vec<FixedHash>,
    pub change_hashes: Vec<FixedHash>,
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
