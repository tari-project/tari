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
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tari_common_types::{tari_address::TariAddress, transaction::TxId, types::FixedHash};
use tari_core::transactions::{
    tari_amount::MicroMinotari,
    transaction_components::{payment_id::PaymentId, OutputFeatures, Transaction, WalletOutput},
    transaction_protocol::TransactionMetadata,
};

use crate::transaction_service::{
    error::TransactionServiceError,
    offline_signing::marshal_output_pair::MarshalOutputPair,
};

const SUPPORTED_VERSION: &str = "1.0.0";

pub fn get_supported_version() -> Version {
    Version::parse(SUPPORTED_VERSION).unwrap()
}

pub trait HasVersion {
    fn get_version(&self) -> &Version;
}

pub trait TransactionResult: HasVersion + Serialize + DeserializeOwned + Sized {
    fn from_json(s: &str) -> Result<Self, TransactionServiceError> {
        let value: serde_json::Value =
            serde_json::from_str(s).map_err(|e| TransactionServiceError::SerializationError(e.to_string()))?;
        let version = value
            .get("version")
            .ok_or_else(|| TransactionServiceError::SerializationError("Missing version".into()))?;
        let version: Version = serde_json::from_value(version.clone())
            .map_err(|e| TransactionServiceError::SerializationError(e.to_string()))?;
        if version != get_supported_version() {
            return Err(TransactionServiceError::SerializationError(format!(
                "Unsupported version. Expected '{}', got '{}'",
                get_supported_version(),
                version
            )));
        }

        let deserialized_obj: Self =
            serde_json::from_str(s).map_err(|e| TransactionServiceError::SerializationError(e.to_string()))?;

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
    pub address: TariAddress,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OneSidedTransactionInfo {
    /// Payment ID
    pub payment_id: PaymentId,
    /// Recipient
    pub recipient: PaymentRecipient,
    /// The change output details. This may be None if no change is required.
    pub change_output: Option<MarshalOutputPair>,
    /// All transaction inputs inputs.
    pub inputs: Vec<MarshalOutputPair>,
    /// The recipient's outputs.
    pub outputs: Vec<MarshalOutputPair>,
    /// Details used to construct the transaction kernel.
    pub metadata: TransactionMetadata,
    /// Sender address
    pub sender_address: TariAddress,
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
    pub change_output: Option<WalletOutput>,
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
