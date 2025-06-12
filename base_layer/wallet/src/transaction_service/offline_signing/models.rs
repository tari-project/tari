use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tari_common_types::{tari_address::TariAddress, transaction::TxId, types::FixedHash};
use tari_core::transactions::{
    tari_amount::MicroMinotari,
    transaction_components::{encrypted_data::PaymentId, OutputFeatures},
    transaction_protocol::sender::SingleRoundSenderData,
    SenderTransactionProtocol,
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
pub struct PrepareOneSidedTransactionForSigningResult {
    pub version: Version,
    pub dest_address: TariAddress,
    pub amount: MicroMinotari,
    pub payment_id: PaymentId,
    pub tx_id: TxId,
    pub stp: SenderTransactionProtocol,
    pub encrypted_commitment_mask_keys: Vec<Vec<u8>>,
    pub script: TariScript,
    pub use_stealth_address: bool,
    pub output_features: OutputFeatures,
    pub single_round_sender_data: SingleRoundSenderData,
    pub encrypted_change_sender_offset_key: Option<Vec<u8>>,
    pub last_seen_tip_height: Option<u64>,
}

impl TransactionResult for PrepareOneSidedTransactionForSigningResult {}

impl HasVersion for PrepareOneSidedTransactionForSigningResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedOneSidedTransactionResult {
    pub version: Version,
    pub request: PrepareOneSidedTransactionForSigningResult,
    pub stp: SenderTransactionProtocol,
    pub sent_hashes: Vec<FixedHash>,
    pub change_hashes: Vec<FixedHash>,
}

impl TransactionResult for SignedOneSidedTransactionResult {}

impl HasVersion for SignedOneSidedTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}
