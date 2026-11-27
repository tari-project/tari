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
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, CompressedSignature, FixedHash},
};

use crate::{
    MicroMinotari,
    helpers::terminal_safe::escape_str_for_terminal,
    key_manager::TariKeyId,
    transaction_components::{KernelFeatures, MemoField, OutputFeatures, Transaction, TransactionError, WalletOutput},
};

/// Version 4 had no payload integrity signature.
/// Version 5 adds `payload_signature` to all `Prepare*` results so the offline
/// signer can verify the payload was not tampered with in transit.
const VERSION_5: &str = "5.0.0";
/// Version 6 adds `payload_key_id` and `expires_at`, both covered by the payload signature.
const SUPPORTED_VERSION: &str = "6.0.0";

pub fn get_supported_versions() -> Vec<Version> {
    vec![
        Version::parse(VERSION_5).unwrap(),
        Version::parse(SUPPORTED_VERSION).unwrap(),
    ]
}

/// Whether a payload of `version` carries `payload_key_id` and `expires_at`.
pub fn has_payload_auth_fields(version: &Version) -> bool {
    version.major >= 6
}

pub fn get_latest_version() -> Version {
    Version::parse(SUPPORTED_VERSION).unwrap()
}

pub trait HasVersion {
    fn get_version(&self) -> &Version;
}

/// serde errors can quote the input, so they are escaped before they can reach a terminal.
fn parse_error(e: serde_json::Error) -> TransactionError {
    TransactionError::SerializationError(escape_str_for_terminal(&e.to_string()).into_owned())
}

pub trait TransactionResult: HasVersion + Serialize + DeserializeOwned + Sized {
    fn from_json(s: &str) -> Result<Self, TransactionError> {
        let value: serde_json::Value = serde_json::from_str(s).map_err(parse_error)?;
        let version = value
            .get("version")
            .ok_or_else(|| TransactionError::SerializationError("Missing version".into()))?;
        let version: Version = serde_json::from_value(version.clone()).map_err(parse_error)?;
        if !get_supported_versions().contains(&version) {
            let supported = get_supported_versions()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("', '");
            return Err(TransactionError::SerializationError(format!(
                "Unsupported version. Expected one of '{supported}', got '{version}'"
            )));
        }

        let deserialized_obj: Self = serde_json::from_str(s).map_err(parse_error)?;

        Ok(deserialized_obj)
    }

    fn to_json(&self) -> Result<String, TransactionError> {
        serde_json::to_string(&self).map_err(|e| TransactionError::SerializationError(e.to_string()))
    }
}

/// A domain-separated Schnorr signature produced by the online wallet over the Borsh-encoded
/// payload data.  The offline signer verifies this before using the spend key, so that any
/// in-transit tampering (recipient swap, amount change, input substitution, …) by a party
/// that does not hold the signing key is detected and the signing operation is aborted.
///
/// The challenge binds the nonce public key R, the public key P the payload claims to be
/// signed with, and the canonical Borsh bytes of the transaction payload:
/// `H_domain(R || P || borsh_bytes)`.  The verifier derives P locally (from its own view key,
/// or from its spend key for a payload key) rather than trusting a key embedded in the payload.
///
/// # Security — which key signed it matters
///
/// A version 6 payload is normally signed with a **payload key** (`payload_key_id` is
/// `Some(PayloadAuth { index })`): a key derived from the spend key on the signer and imported
/// into the online wallet, so nobody else can produce it. Without one, the payload is signed with
/// the wallet's **view key** (`payload_key_id` is `None`, and always on version 5), which is
/// shareable by design: it is handed out to view-only wallets, auditors, exchanges and block
/// explorers. Anyone holding the view key can *forge* a view-key signature over a payload of their
/// choosing, so it is only a **transit-integrity** check. A signer that requires a payload key
/// (see [`crate::offline_signing::PayloadKeyPolicy`]) refuses view-key payloads.
///
/// Either way the authoritative authorisation step is the human operating the air-gapped signer
/// confirming a [`crate::offline_signing::PayloadSummary`] of what is about to be signed. The
/// type system requires it: the `sign_locked_*` functions only accept a
/// [`crate::offline_signing::ConfirmedPayload`], which is produced by confirming a presented summary.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct PayloadIntegritySignature {
    /// Schnorr signature produced with the view private key.
    /// The signature nonce R is embedded in this value and is used when
    /// recomputing the challenge on the verification side.
    pub signature: CompressedSignature,
}

/// Canonical Borsh bytes for a one-sided transaction payload covering all
/// security-relevant fields (`version`, `tx_id`, the full `info` struct and, from version 6,
/// `payload_key_id` and `expires_at`).
/// Used for both signing (prepare side) and integrity verification (sign side).
///
/// `OneSidedTransactionInfo` is serialised directly via its `BorshSerialize` impl;
/// for `WalletOutput` entries the impl commits to the consensus output hash, which
/// covers ALL output fields, so tampering with any field is detectable.
pub fn borsh_canonical_one_sided(
    version: &Version,
    tx_id: TxId,
    info: &OneSidedTransactionInfo,
    payload_key_id: Option<&TariKeyId>,
    expires_at: u64,
) -> Result<Vec<u8>, TransactionError> {
    borsh_canonical(version, tx_id, info, payload_key_id, expires_at)
}

/// Canonical Borsh bytes for a multisig transaction payload.
pub fn borsh_canonical_multisig(
    version: &Version,
    tx_id: TxId,
    info: &OneSidedMultisigTransactionInfo,
    payload_key_id: Option<&TariKeyId>,
    expires_at: u64,
) -> Result<Vec<u8>, TransactionError> {
    borsh_canonical(version, tx_id, info, payload_key_id, expires_at)
}

fn borsh_canonical<T: BorshSerialize>(
    version: &Version,
    tx_id: TxId,
    info: &T,
    payload_key_id: Option<&TariKeyId>,
    expires_at: u64,
) -> Result<Vec<u8>, TransactionError> {
    let to_err = |e: std::io::Error| TransactionError::SerializationError(e.to_string());
    let mut buf = Vec::new();
    BorshSerialize::serialize(&version.to_string(), &mut buf).map_err(to_err)?;
    BorshSerialize::serialize(&u64::from(tx_id), &mut buf).map_err(to_err)?;
    BorshSerialize::serialize(info, &mut buf).map_err(to_err)?;
    if has_payload_auth_fields(version) {
        BorshSerialize::serialize(&payload_key_id, &mut buf).map_err(to_err)?;
        BorshSerialize::serialize(&expires_at, &mut buf).map_err(to_err)?;
        return Ok(buf);
    }
    if payload_key_id.is_some() || expires_at != 0 {
        // Version 5 bytes do not cover these fields, so they could be added to a signed payload unnoticed
        return Err(TransactionError::SerializationError(format!(
            "A version {version} payload cannot carry a payload key id or an expiry"
        )));
    }
    Ok(buf)
}

/// Accepts only `None` or a `PayloadAuth` key id: nothing else may claim to have signed a payload.
fn deserialize_payload_key_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<TariKeyId>, D::Error> {
    match <Option<TariKeyId> as Deserialize>::deserialize(deserializer)? {
        key_id @ (None | Some(TariKeyId::PayloadAuth { .. })) => Ok(key_id),
        Some(_) => Err(serde::de::Error::custom("payload_key_id must be a payload_auth key id")),
    }
}

/// The payload key file `minotari_offline_signer export-payload-key` writes and the console wallet's
/// `import-payload-key` reads. Holds a private key, so it has no `Debug` and wipes the key when dropped.
#[derive(Serialize, Deserialize)]
pub struct ExportedPayloadKey {
    pub index: u64,
    /// Hex
    pub private_key: String,
    /// Hex
    pub public_key: String,
}

impl Drop for ExportedPayloadKey {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.private_key);
    }
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
    /// The payload key that signed this payload, or `None` when the view key did. Version 6 only.
    #[serde(default, deserialize_with = "deserialize_payload_key_id")]
    pub payload_key_id: Option<TariKeyId>,
    /// Unix time in seconds (UTC) after which the signer refuses this payload. Version 6 only; `0` on version 5.
    #[serde(default)]
    pub expires_at: u64,
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
    /// The payload key that signed this payload, or `None` when the view key did. Version 6 only.
    #[serde(default, deserialize_with = "deserialize_payload_key_id")]
    pub payload_key_id: Option<TariKeyId>,
    /// Unix time in seconds (UTC) after which the signer refuses this payload. Version 6 only; `0` on version 5.
    #[serde(default)]
    pub expires_at: u64,
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
    /// The payload key that signed this payload, or `None` when the view key did. Version 6 only.
    #[serde(default, deserialize_with = "deserialize_payload_key_id")]
    pub payload_key_id: Option<TariKeyId>,
    /// Unix time in seconds (UTC) after which the signer refuses this payload. Version 6 only; `0` on version 5.
    #[serde(default)]
    pub expires_at: u64,
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
/// generated. That is the case whenever what is left after the outputs and the fee is no more than a change output
/// would cost. This wallet's transaction service refuses to prepare such a payload in the first place (the public
/// `prepare_*` functions in [`crate::offline_signing::offline_signer`] do not check); a payload refused by the signer
/// has to be prepared again with a smaller amount.
///
/// The change output's sender offset key never leaves the signer, which is what the builder's sender offset key
/// invariant needs here: the recipient outputs' keys are handed back to the online wallet. See "One sender offset key
/// the host cannot learn" on [`crate::TransactionBuilder`].
///
/// This only addresses recovery through the script offset. It does not make it safe to treat the online wallet as
/// untrusted in general: anyone holding the view key can produce a payload that passes the integrity check, so the
/// operator must still check every recipient, amount and "Other outputs" entry, and the fee, total spend and change,
/// in the [`crate::offline_signing::PayloadSummary`] before approving. The `sign_locked_*` functions only accept a
/// [`crate::offline_signing::ConfirmedPayload`], so a summary is always presented before signing.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub sent_hashes: Vec<FixedHash>,
    pub outputs: Vec<WalletOutput>,
    pub change_hashes: Vec<FixedHash>,
    pub change_output: Option<WalletOutput>,
    pub tx_id: TxId,
}

/// How the signing of a payload was approved. Mirrors [`crate::offline_signing::ConfirmationMode`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfirmationKind {
    /// An operator approved the summary at a prompt.
    Prompt,
    /// The summary was written out and signed without asking anyone.
    NoPrompt,
}

/// A record of what the offline signer approved and signed. It is not signed, so anyone handling the result can
/// rewrite it: it is not proof of approval and nothing should trust it as such.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SigningReceipt {
    pub tx_id: TxId,
    /// Domain-separated Blake2b-256 of the canonical payload bytes, the bytes the payload signature covers.
    pub payload_hash: FixedHash,
    pub confirmation: ConfirmationKind,
    /// Unix time in seconds, by the signer's clock.
    pub signed_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SignedOneSidedTransactionResult {
    pub version: Version,
    pub request: PrepareOneSidedTransactionForSigningResult,
    pub signed_transaction: SignedTransaction,
    /// Absent from results written by signers that predate it.
    #[serde(default)]
    pub receipt: Option<SigningReceipt>,
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
    /// Absent from results written by signers that predate it.
    #[serde(default)]
    pub receipt: Option<SigningReceipt>,
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
    /// Absent from results written by signers that predate it.
    #[serde(default)]
    pub receipt: Option<SigningReceipt>,
}

impl TransactionResult for SignedOneSidedWithdrawMultisigTransactionResult {}

impl HasVersion for SignedOneSidedWithdrawMultisigTransactionResult {
    fn get_version(&self) -> &Version {
        &self.version
    }
}
