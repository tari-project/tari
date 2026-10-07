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
use borsh::{BorshDeserialize, BorshSerialize};
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

/// A domain-separated Schnorr signature produced by the view wallet over the Borsh-encoded
/// payload data.  The offline signer verifies this before using the spend key, so that any
/// in-transit tampering (recipient swap, amount change, input substitution, …) by a party
/// that does not hold the view key is detected and the signing operation is aborted.
///
/// The challenge binds the nonce public key R, the view public key P, and the canonical
/// Borsh bytes of the transaction payload: `H_domain(R || P || borsh_bytes)`.  Both
/// wallets share the same view key, so the verifier derives P locally rather than
/// trusting a key embedded in the payload.
///
/// # Security — what this does *not* prove
///
/// The signing key here is the wallet's **view key**, which is shareable by design: it is
/// handed out to view-only wallets, auditors, exchanges and block explorers so that they can
/// observe incoming payments.  Anyone holding the view key can therefore *forge* this
/// signature over a payload of their choosing — for example one that redirects the funds to
/// themselves — and the air-gapped signer will verify it as authentic.
///
/// This signature is consequently a **transit-integrity** check, not an authenticity check.
/// The authoritative authorisation step is the human operating the air-gapped signer
/// confirming a [`crate::offline_signing::PayloadSummary`] of what is about to be signed.
/// Closing the gap in the protocol itself requires a dedicated authentication key that is
/// provisioned to the online wallet separately from the view key, which is a breaking change
/// to the key-export format and the payload version.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct PayloadIntegritySignature {
    /// Schnorr signature produced with the view private key.
    /// The signature nonce R is embedded in this value and is used when
    /// recomputing the challenge on the verification side.
    pub signature: CompressedSignature,
}

/// Canonical Borsh bytes for a one-sided transaction payload covering all
/// security-relevant fields (`version`, `tx_id`, and the full `info` struct).
/// Used for both signing (prepare side) and integrity verification (sign side).
///
/// `OneSidedTransactionInfo` is serialised directly via its `BorshSerialize` impl;
/// for `WalletOutput` entries the impl commits to the consensus output hash, which
/// covers ALL output fields, so tampering with any field is detectable.
pub fn borsh_canonical_one_sided(
    version: &Version,
    tx_id: TxId,
    info: &OneSidedTransactionInfo,
) -> Result<Vec<u8>, TransactionError> {
    let mut buf = Vec::new();
    BorshSerialize::serialize(&version.to_string(), &mut buf)
        .map_err(|e| TransactionError::SerializationError(e.to_string()))?;
    BorshSerialize::serialize(&u64::from(tx_id), &mut buf)
        .map_err(|e| TransactionError::SerializationError(e.to_string()))?;
    BorshSerialize::serialize(info, &mut buf).map_err(|e| TransactionError::SerializationError(e.to_string()))?;
    Ok(buf)
}

/// Canonical Borsh bytes for a multisig transaction payload.
pub fn borsh_canonical_multisig(
    version: &Version,
    tx_id: TxId,
    info: &OneSidedMultisigTransactionInfo,
) -> Result<Vec<u8>, TransactionError> {
    let mut buf = Vec::new();
    BorshSerialize::serialize(&version.to_string(), &mut buf)
        .map_err(|e| TransactionError::SerializationError(e.to_string()))?;
    BorshSerialize::serialize(&u64::from(tx_id), &mut buf)
        .map_err(|e| TransactionError::SerializationError(e.to_string()))?;
    BorshSerialize::serialize(info, &mut buf).map_err(|e| TransactionError::SerializationError(e.to_string()))?;
    Ok(buf)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, BorshSerialize, BorshDeserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, BorshSerialize)]
pub struct OneSidedTransactionInfo {
    /// Payment ID
    pub payment_id: MemoField,
    /// Recipient
    pub recipients: Vec<PaymentRecipient>,
    /// All transaction inputs.
    pub inputs: Vec<WalletOutput>,
    /// The recipient's outputs.
    pub outputs: Vec<WalletOutput>,
    pub fee: MicroMinotari,
    pub fee_per_gram: MicroMinotari,
    /// Sender address
    pub sender_address: TariAddress,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, BorshSerialize)]
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

/// What the offline signer hands back to the online wallet.
///
/// # Every signed transaction carries a change output
///
/// The offline signer refuses to sign any payload that would produce no change output, and returns
/// [`crate::TransactionBuilderError::OfflineTransactionRequiresChange`] instead, before any sender offset key is
/// generated. The online wallet refuses to prepare such a payload in the first place; if one arrives anyway it has to
/// be prepared again with a smaller amount.
///
/// The rule closes one way for the online wallet, which holds the view key, to recover the spend key: through the
/// script offset of a transaction without change. The transaction publishes
/// `script offset = sum(input script keys) - sum(sender offset keys)`, and every input script key is
/// `H(mask) + spend key` with a mask the online wallet knows. The recipient outputs in `outputs` carry their sender
/// offset key ids, which are wrapped under the view key, so the online wallet can learn those sender offset keys. The
/// change output's sender offset key is generated and used inside the signer and never leaves it (`change_output`
/// carries only its public key), so with a change output present that sum always has a term the online wallet cannot
/// learn, and it cannot solve for the spend key. Outputs that arrived fully formed in the payload also have their
/// sender offset keys replaced inside the signer, and those are not returned either; the rule does not rely on them.
///
/// This only addresses recovery through the script offset. It does not make it safe to treat the online wallet as
/// untrusted in general: anyone holding the view key can produce a payload that passes the integrity check, so the
/// operator must still check every recipient, amount and "Other outputs" entry in the
/// [`crate::offline_signing::PayloadSummary`] before approving.
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
