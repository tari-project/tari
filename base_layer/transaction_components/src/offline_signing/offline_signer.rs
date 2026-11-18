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
use std::time::Duration;

use blake2::{Blake2b, Digest};
use digest::consts::U64;
use log::{info, warn};
use semver::Version;
use tari_common::configuration::Network;
use tari_common_types::{tari_address::TariAddress, transaction::TxId, types::CompressedPublicKey};
use tari_crypto::hashing::DomainSeparatedHasher;
use tari_hashing::OfflineSigningPayloadHashDomain;
use tari_utilities::ByteArray;

use crate::{
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
    consensus::ConsensusConstants,
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    offline_signing::{
        ConfirmedPayload,
        models::{
            OneSidedMultisigTransactionInfo,
            OneSidedTransactionInfo,
            PayloadIntegritySignature,
            PaymentRecipient,
            PrepareDepositMultisigTransactionResult,
            PrepareOneSidedTransactionForSigningResult,
            PrepareWithdrawMultisigTransactionResult,
            SignedOneSidedDepositMultisigTransactionResult,
            SignedOneSidedTransactionResult,
            SignedOneSidedWithdrawMultisigTransactionResult,
            borsh_canonical_multisig,
            borsh_canonical_one_sided,
            get_latest_version,
            has_payload_auth_fields,
        },
        one_sided_signer::{build_and_sign_transaction, sign_multisig_transaction, sign_multisig_withdraw_transaction},
        payload_summary::{LOG_TARGET, format_unix_time},
    },
    transaction_components::{MemoField, OutputFeatures, TransactionError, WalletOutput},
};

// ---------------------------------------------------------------------------
// Payload integrity helpers
// ---------------------------------------------------------------------------

/// How long a prepared payload can be signed for when the caller does not say.
pub const DEFAULT_PAYLOAD_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

/// Which payload signatures the offline signer accepts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PayloadKeyPolicy {
    /// A payload signed with the view key, or with a payload key of any index.
    #[default]
    AcceptViewKey,
    /// Only a version 6 payload signed with a payload key whose index is listed. The view key is shared with
    /// view-only wallets and auditors, so a view-key signature does not show who prepared the payload.
    RequirePayloadKey { accepted_indices: Vec<u64> },
}

/// The signer's clock, in unix seconds.
pub(crate) fn unix_now() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp()).unwrap_or(0)
}

/// Computes the 64-byte challenge used to sign or verify an offline-signing
/// payload.  The challenge is a domain-separated Blake2b-512 hash that binds
/// the nonce public key R, the signing public key P, and the canonical Borsh bytes
/// of the transaction data: `H_domain(R || P || canonical_borsh_bytes)`.
///
/// Binding R and P into the challenge prevents key-substitution attacks and
/// ensures the signature cannot be replayed under a different nonce or key.
///
/// Both keys are taken as typed `&CompressedPublicKey` so the caller cannot
/// accidentally pass an arbitrary byte slice in place of a public key.
fn payload_challenge(nonce_pub: &CompressedPublicKey, signing_pub: &CompressedPublicKey, canonical: &[u8]) -> [u8; 64] {
    let hash = DomainSeparatedHasher::<Blake2b<U64>, OfflineSigningPayloadHashDomain>::new()
        .chain_update(nonce_pub.as_bytes())
        .chain_update(signing_pub.as_bytes())
        .chain_update(canonical)
        .finalize();
    let mut challenge = [0u8; 64];
    challenge.copy_from_slice(hash.as_ref());
    challenge
}

/// Signs the Borsh-serialised `Prepare*` payload with `signing_key_id`: the view key or the imported payload key.
///
/// The challenge binds the nonce public key R and the signing public key P so that neither
/// can be substituted after signing.
pub(crate) fn sign_payload<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    signing_key_id: &TariKeyId,
    canonical: &[u8],
) -> Result<PayloadIntegritySignature, TransactionBuilderError> {
    let signing_pub = key_manager.get_public_key_at_key_id(signing_key_id)?;
    let nonce_key = key_manager.get_random_key(None, None)?;
    let challenge = payload_challenge(&nonce_key.pub_key, &signing_pub, canonical);
    let signature = key_manager.sign_with_nonce_and_challenge(signing_key_id, &nonce_key.key_id, &challenge)?;
    Ok(PayloadIntegritySignature { signature })
}

/// Signs a new payload with the wallet's imported payload key, or with the view key when it has none. `canonical`
/// builds the payload's canonical bytes from its `payload_key_id` and `expires_at`, which this returns with the
/// signature.
fn authenticate_payload<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    lifetime: Option<Duration>,
    canonical: impl FnOnce(Option<&TariKeyId>, u64) -> Result<Vec<u8>, TransactionError>,
) -> Result<(Option<TariKeyId>, u64, PayloadIntegritySignature), TransactionBuilderError> {
    let payload_key = key_manager.get_payload_auth_key();
    let payload_key_id = payload_key
        .as_ref()
        .map(|(index, _)| TariKeyId::PayloadAuth { index: *index });
    let expires_at = unix_now().saturating_add(lifetime.unwrap_or(DEFAULT_PAYLOAD_LIFETIME).as_secs());
    let canonical = canonical(payload_key_id.as_ref(), expires_at)
        .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))?;
    let signing_key_id = match payload_key {
        Some((_, key_id)) => key_id,
        None => key_manager.get_view_key().key_id,
    };
    let payload_signature = sign_payload(key_manager, &signing_key_id, &canonical)?;
    Ok((payload_key_id, expires_at, payload_signature))
}

/// Refuses a payload once `now` is past its `expires_at`, which is `None` for a version 5 payload (no expiry).
pub(crate) fn check_payload_expiry(expires_at: Option<u64>, now: u64) -> Result<(), TransactionBuilderError> {
    if let Some(expires_at) = expires_at.filter(|expires_at| now > *expires_at) {
        return Err(TransactionBuilderError::Other(format!(
            "Payload refused: it expired at {}. Prepare it again.",
            format_unix_time(expires_at)
        )));
    }
    Ok(())
}

/// Refuses a payload whose signing key `policy` does not accept, before any key is derived. Error messages name the
/// reason and never key material.
fn check_payload_key_policy(
    policy: &PayloadKeyPolicy,
    version: &Version,
    payload_key_id: Option<&TariKeyId>,
) -> Result<(), TransactionBuilderError> {
    let PayloadKeyPolicy::RequirePayloadKey { accepted_indices } = policy else {
        return Ok(());
    };
    let refuse = |reason: String| {
        Err(TransactionBuilderError::Other(format!(
            "Payload refused: {reason}. This signer requires a payload key."
        )))
    };
    if !has_payload_auth_fields(version) {
        return refuse(format!(
            "it is a version {version} payload, which can only be signed with the view key"
        ));
    }
    let Some(TariKeyId::PayloadAuth { index }) = payload_key_id else {
        return refuse("it was signed with the view key (shared key)".to_string());
    };
    if accepted_indices.is_empty() {
        return refuse("no payload key index is accepted (the accept-list is empty), so every payload is".to_string());
    }
    if !accepted_indices.contains(index) {
        return refuse(format!("payload key #{index} is not an accepted index"));
    }
    Ok(())
}

/// Verifies a `Prepare*` result's expiry, signing key and [`PayloadIntegritySignature`], at time `now`.
///
/// The nonce public key R is recovered from the embedded signature. The signing public key P is derived by the
/// offline signer itself: its view key when `payload_key_id` is `None`, otherwise the payload key it derives from its
/// spend key at the claimed index, so no key is taken from the payload. The challenge is reconstructed as
/// `H_domain(R || P || canonical)` and the Schnorr verification is performed.
///
/// # Security
///
/// A view-key signature can be forged by anyone holding the view key, so passing this check under
/// [`PayloadKeyPolicy::AcceptViewKey`] means "the payload was not mangled by someone without view access", **not**
/// "the payload is what the wallet owner asked for". The `sign_locked_*` functions therefore only accept a
/// [`crate::offline_signing::ConfirmedPayload`], which can only be obtained by presenting a
/// [`crate::offline_signing::PayloadSummary`] of the payload and confirming it.  See the docs on
/// [`PayloadIntegritySignature`] for the full rationale.
pub(crate) fn verify_payload<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    policy: &PayloadKeyPolicy,
    version: &Version,
    payload_key_id: Option<&TariKeyId>,
    expires_at: u64,
    payload_sig: &PayloadIntegritySignature,
    canonical: &[u8],
    now: u64,
) -> Result<(), TransactionBuilderError> {
    check_payload_expiry(has_payload_auth_fields(version).then_some(expires_at), now)?;
    check_payload_key_policy(policy, version, payload_key_id)?;
    let signing_pub = match payload_key_id {
        None => key_manager.get_view_key().pub_key,
        Some(key_id @ TariKeyId::PayloadAuth { .. }) => key_manager.get_public_key_at_key_id(key_id)?,
        Some(_) => {
            return Err(TransactionBuilderError::Other(
                "Payload refused: payload_key_id is not a payload key".to_string(),
            ));
        },
    };
    // R is the nonce public key embedded in the signature.
    let nonce_pub = payload_sig.signature.get_compressed_public_nonce();
    let challenge = payload_challenge(nonce_pub, &signing_pub, canonical);
    let pub_key = signing_pub
        .to_public_key()
        .map_err(|e| TransactionBuilderError::Other(format!("Invalid signing public key: {e}")))?;
    let sig = payload_sig
        .signature
        .to_schnorr_signature()
        .map_err(|e| TransactionBuilderError::Other(format!("Invalid signature in payload: {e}")))?;
    if !sig.verify_raw_uniform(&pub_key, &challenge[..]) {
        return Err(TransactionBuilderError::Other(
            "Offline payload integrity check failed: payload signature is invalid. The payload was tampered with in \
             transit, is corrupt, or was signed with a different key than it claims."
                .to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Prepare functions (online view-wallet side)
// ---------------------------------------------------------------------------

pub fn prepare_one_sided_transaction_for_signing<TKeyManagerInterface: TransactionKeyManagerInterface>(
    key_manager: &TKeyManagerInterface,
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    recipients: &[PaymentRecipient],
    payment_id: MemoField,
    sender_address: TariAddress,
    payload_lifetime: Option<Duration>,
) -> Result<PrepareOneSidedTransactionForSigningResult, TransactionBuilderError> {
    let fee = tx_builder.fee();
    let fee_per_gram = tx_builder.fee_per_gram().unwrap_or_default();
    let outputs = tx_builder
        .custom_outputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let inputs = tx_builder
        .inputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let info = OneSidedTransactionInfo {
        payment_id,
        recipients: recipients.to_vec(),
        inputs,
        outputs,
        fee,
        fee_per_gram,
        sender_address,
    };

    let version = get_latest_version();
    // The payload_signature field is excluded by construction (it covers only the data).
    let (payload_key_id, expires_at, payload_signature) =
        authenticate_payload(key_manager, payload_lifetime, |key_id, expires_at| {
            borsh_canonical_one_sided(&version, tx_id, &info, key_id, expires_at)
        })?;

    Ok(PrepareOneSidedTransactionForSigningResult {
        version,
        tx_id,
        info,
        payload_signature,
        payload_key_id,
        expires_at,
    })
}

pub fn prepare_deposit_multisig_transaction<TKeyManagerInterface: TransactionKeyManagerInterface>(
    key_manager: &TKeyManagerInterface,
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    amount: MicroMinotari,
    payment_id: MemoField,
    output_features: OutputFeatures,
    party_number: u8,
    public_keys: Vec<CompressedPublicKey>,
    sender: TariAddress,
    recipient: TariAddress,
    payload_lifetime: Option<Duration>,
) -> Result<PrepareDepositMultisigTransactionResult, TransactionBuilderError> {
    // The offline signer derives the recipient output from the view key; refuse here, before the operator approves.
    if recipient.public_view_key().is_none() {
        return Err(TransactionError::BuilderError("Missing public view key".to_string()).into());
    }
    let fee = tx_builder.fee();
    let fee_per_gram = tx_builder.fee_per_gram().unwrap_or_default();
    let outputs = tx_builder
        .custom_outputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let inputs = tx_builder
        .inputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let base = OneSidedTransactionInfo {
        payment_id: payment_id.clone(),
        recipients: vec![PaymentRecipient {
            amount,
            output_features,
            address: recipient,
            payment_id,
        }],
        inputs,
        outputs,
        fee,
        fee_per_gram,
        sender_address: sender,
    };

    let info = OneSidedMultisigTransactionInfo {
        base,
        party_number,
        public_keys,
    };

    let version = get_latest_version();
    let (payload_key_id, expires_at, payload_signature) =
        authenticate_payload(key_manager, payload_lifetime, |key_id, expires_at| {
            borsh_canonical_multisig(&version, tx_id, &info, key_id, expires_at)
        })?;

    Ok(PrepareDepositMultisigTransactionResult {
        version,
        tx_id,
        info,
        payload_signature,
        payload_key_id,
        expires_at,
    })
}

pub fn prepare_withdraw_multisig_transaction<TKeyManagerInterface: TransactionKeyManagerInterface>(
    key_manager: &TKeyManagerInterface,
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    amount: MicroMinotari,
    payment_id: MemoField,
    output_features: OutputFeatures,
    sender: TariAddress,
    recipient: TariAddress,
    payload_lifetime: Option<Duration>,
) -> Result<PrepareWithdrawMultisigTransactionResult, TransactionBuilderError> {
    // The offline signer derives the recipient output from the view key; refuse here, before the operator approves.
    if recipient.public_view_key().is_none() {
        return Err(TransactionError::BuilderError("Missing public view key".to_string()).into());
    }
    let fee = tx_builder.fee();
    let fee_per_gram = tx_builder.fee_per_gram().unwrap_or_default();
    let outputs = tx_builder
        .custom_outputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();
    let inputs = tx_builder
        .inputs()
        .iter()
        .map(|output_pair| output_pair.output.clone())
        .collect::<Vec<WalletOutput>>();

    let info = OneSidedTransactionInfo {
        payment_id: payment_id.clone(),
        recipients: vec![PaymentRecipient {
            amount,
            output_features,
            address: recipient,
            payment_id,
        }],
        fee,
        fee_per_gram,
        inputs,
        outputs,
        sender_address: sender,
    };

    let version = get_latest_version();
    let (payload_key_id, expires_at, payload_signature) =
        authenticate_payload(key_manager, payload_lifetime, |key_id, expires_at| {
            borsh_canonical_one_sided(&version, tx_id, &info, key_id, expires_at)
        })?;

    Ok(PrepareWithdrawMultisigTransactionResult {
        version,
        tx_id,
        info,
        payload_signature,
        payload_key_id,
        expires_at,
    })
}

// ---------------------------------------------------------------------------
// Sign functions (offline signer side) — verify payload integrity first
// ---------------------------------------------------------------------------

/// Records whether an offline payload was signed, after the integrity and key-id checks have run.
fn log_signing_outcome<T>(
    tx_id: TxId,
    result: Result<T, TransactionBuilderError>,
) -> Result<T, TransactionBuilderError> {
    match &result {
        Ok(_) => info!(target: LOG_TARGET, "Offline payload {tx_id} signed"),
        Err(e) => warn!(target: LOG_TARGET, "Offline payload {tx_id} not signed: {e}"),
    }
    result
}

pub fn sign_locked_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    confirmed: ConfirmedPayload<PrepareOneSidedTransactionForSigningResult>,
    policy: &PayloadKeyPolicy,
) -> Result<SignedOneSidedTransactionResult, TransactionBuilderError> {
    let request = confirmed.into_payload();
    let tx_id = request.tx_id;
    log_signing_outcome(
        tx_id,
        verify_and_sign_one_sided(key_manager, consensus_constants, network, request, policy),
    )
}

fn verify_and_sign_one_sided<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    request: PrepareOneSidedTransactionForSigningResult,
    policy: &PayloadKeyPolicy,
) -> Result<SignedOneSidedTransactionResult, TransactionBuilderError> {
    // Verify the payload was not tampered with between prepare and sign.
    let key_id = request.payload_key_id.as_ref();
    let canonical = borsh_canonical_one_sided(
        &request.version,
        request.tx_id,
        &request.info,
        key_id,
        request.expires_at,
    )
    .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))?;
    verify_payload(
        key_manager,
        policy,
        &request.version,
        key_id,
        request.expires_at,
        &request.payload_signature,
        &canonical,
        unix_now(),
    )?;

    let signed_transaction =
        build_and_sign_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}

pub fn sign_locked_deposit_multisig_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    confirmed: ConfirmedPayload<PrepareDepositMultisigTransactionResult>,
    policy: &PayloadKeyPolicy,
) -> Result<SignedOneSidedDepositMultisigTransactionResult, TransactionBuilderError> {
    let request = confirmed.into_payload();
    let tx_id = request.tx_id;
    log_signing_outcome(
        tx_id,
        verify_and_sign_deposit_multisig(key_manager, consensus_constants, network, request, policy),
    )
}

fn verify_and_sign_deposit_multisig<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    request: PrepareDepositMultisigTransactionResult,
    policy: &PayloadKeyPolicy,
) -> Result<SignedOneSidedDepositMultisigTransactionResult, TransactionBuilderError> {
    // Verify the payload was not tampered with between prepare and sign.
    let key_id = request.payload_key_id.as_ref();
    let canonical = borsh_canonical_multisig(
        &request.version,
        request.tx_id,
        &request.info,
        key_id,
        request.expires_at,
    )
    .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))?;
    verify_payload(
        key_manager,
        policy,
        &request.version,
        key_id,
        request.expires_at,
        &request.payload_signature,
        &canonical,
        unix_now(),
    )?;

    let signed_transaction =
        sign_multisig_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedDepositMultisigTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}

pub fn sign_locked_withdraw_multisig_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    confirmed: ConfirmedPayload<PrepareWithdrawMultisigTransactionResult>,
    policy: &PayloadKeyPolicy,
) -> Result<SignedOneSidedWithdrawMultisigTransactionResult, TransactionBuilderError> {
    let request = confirmed.into_payload();
    let tx_id = request.tx_id;
    log_signing_outcome(
        tx_id,
        verify_and_sign_withdraw_multisig(key_manager, consensus_constants, network, request, policy),
    )
}

fn verify_and_sign_withdraw_multisig<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    request: PrepareWithdrawMultisigTransactionResult,
    policy: &PayloadKeyPolicy,
) -> Result<SignedOneSidedWithdrawMultisigTransactionResult, TransactionBuilderError> {
    // Verify the payload was not tampered with between prepare and sign.
    let key_id = request.payload_key_id.as_ref();
    let canonical = borsh_canonical_one_sided(
        &request.version,
        request.tx_id,
        &request.info,
        key_id,
        request.expires_at,
    )
    .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))?;
    verify_payload(
        key_manager,
        policy,
        &request.version,
        key_id,
        request.expires_at,
        &request.payload_signature,
        &canonical,
        unix_now(),
    )?;

    let signed_transaction =
        sign_multisig_withdraw_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedWithdrawMultisigTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}
