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
use blake2::{Blake2b, Digest};
use digest::consts::U64;
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
    key_manager::TransactionKeyManagerInterface,
    offline_signing::{
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
        },
        one_sided_signer::{build_and_sign_transaction, sign_multisig_transaction, sign_multisig_withdraw_transaction},
    },
    transaction_components::{MemoField, OutputFeatures, WalletOutput},
};

// ---------------------------------------------------------------------------
// Payload integrity helpers
// ---------------------------------------------------------------------------

/// Computes the 64-byte challenge used to sign or verify an offline-signing
/// payload.  The challenge is a domain-separated Blake2b-512 hash that binds
/// the nonce public key R, the view public key P, and the canonical Borsh bytes
/// of the transaction data: `H_domain(R || P || canonical_borsh_bytes)`.
///
/// Binding R and P into the challenge prevents key-substitution attacks and
/// ensures the signature cannot be replayed under a different nonce or key.
///
/// Both keys are taken as typed `&CompressedPublicKey` so the caller cannot
/// accidentally pass an arbitrary byte slice in place of a public key.
fn payload_challenge(nonce_pub: &CompressedPublicKey, view_pub: &CompressedPublicKey, canonical: &[u8]) -> [u8; 64] {
    let hash = DomainSeparatedHasher::<Blake2b<U64>, OfflineSigningPayloadHashDomain>::new()
        .chain_update(nonce_pub.as_bytes())
        .chain_update(view_pub.as_bytes())
        .chain_update(canonical)
        .finalize();
    let mut challenge = [0u8; 64];
    challenge.copy_from_slice(hash.as_ref());
    challenge
}

/// Signs the Borsh-serialised `Prepare*` payload with the wallet's view private key.
///
/// The challenge binds the nonce public key R and view public key P so that neither
/// can be substituted after signing.
fn sign_payload<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    canonical: &[u8],
) -> Result<PayloadIntegritySignature, TransactionBuilderError> {
    let view_key = key_manager.get_view_key();
    let nonce_key = key_manager.get_random_key(None, None)?;
    let challenge = payload_challenge(&nonce_key.pub_key, &view_key.pub_key, canonical);
    let signature = key_manager.sign_with_nonce_and_challenge(&view_key.key_id, &nonce_key.key_id, &challenge)?;
    Ok(PayloadIntegritySignature { signature })
}

/// Verifies the [`PayloadIntegritySignature`] embedded in a `Prepare*` result.
///
/// The nonce public key R is recovered from the embedded signature; the view
/// public key P is obtained from the offline signer's own key manager (both
/// wallets share the same view key, so no key needs to be transmitted in the
/// payload).  The challenge is reconstructed as `H_domain(R || P || canonical)`
/// and the Schnorr verification is performed.
///
/// # Security
///
/// The view key is a shareable key, so a party that holds it can forge this signature over
/// an arbitrary payload.  Passing this check therefore means "the payload was not mangled by
/// someone without view access", **not** "the payload is what the wallet owner asked for".
/// Callers that use a spend key MUST additionally have the operator confirm a
/// [`crate::offline_signing::PayloadSummary`] of the payload.  See the docs on
/// [`PayloadIntegritySignature`] for the full rationale.
fn verify_payload_signature<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    payload_sig: &PayloadIntegritySignature,
    canonical: &[u8],
) -> Result<(), TransactionBuilderError> {
    let view_key = key_manager.get_view_key();
    // R is the nonce public key embedded in the signature.
    let nonce_pub = payload_sig.signature.get_compressed_public_nonce();
    let challenge = payload_challenge(nonce_pub, &view_key.pub_key, canonical);
    let pub_key = view_key
        .pub_key
        .to_public_key()
        .map_err(|e| TransactionBuilderError::Other(format!("Invalid view public key: {e}")))?;
    let sig = payload_sig
        .signature
        .to_schnorr_signature()
        .map_err(|e| TransactionBuilderError::Other(format!("Invalid signature in payload: {e}")))?;
    if !sig.verify_raw_uniform(&pub_key, &challenge[..]) {
        return Err(TransactionBuilderError::Other(
            "Offline payload integrity check failed: payload signature is invalid. The payload was tampered with in \
             transit or is corrupt."
                .to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Prepare functions (online view-wallet side)
// ---------------------------------------------------------------------------

pub fn prepare_one_sided_transaction_for_signing<TKeyManagerInterface: TransactionKeyManagerInterface>(
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    recipients: &[PaymentRecipient],
    payment_id: MemoField,
    sender_address: TariAddress,
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
    // Borsh-serialise version + tx_id + info to get the canonical bytes to sign.
    // The payload_signature field is excluded by construction (it covers only the data).
    let canonical = borsh_canonical_one_sided(&version, tx_id, &info)
        .map_err(|e| TransactionBuilderError::Other(format!("borsh_canonical_one_sided failed: {e}")))?;
    let payload_signature = sign_payload(tx_builder.key_manager(), &canonical)?;

    Ok(PrepareOneSidedTransactionForSigningResult {
        version,
        tx_id,
        info,
        payload_signature,
    })
}

pub fn prepare_deposit_multisig_transaction<TKeyManagerInterface: TransactionKeyManagerInterface>(
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    amount: MicroMinotari,
    payment_id: MemoField,
    output_features: OutputFeatures,
    party_number: u8,
    public_keys: Vec<CompressedPublicKey>,
    sender: TariAddress,
    recipient: TariAddress,
) -> Result<PrepareDepositMultisigTransactionResult, TransactionBuilderError> {
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
    let canonical = borsh_canonical_multisig(&version, tx_id, &info)
        .map_err(|e| TransactionBuilderError::Other(format!("borsh_canonical_multisig failed: {e}")))?;
    let payload_signature = sign_payload(tx_builder.key_manager(), &canonical)?;

    Ok(PrepareDepositMultisigTransactionResult {
        version,
        tx_id,
        info,
        payload_signature,
    })
}

pub fn prepare_withdraw_multisig_transaction<TKeyManagerInterface: TransactionKeyManagerInterface>(
    tx_id: TxId,
    tx_builder: TransactionBuilder<TKeyManagerInterface>,
    amount: MicroMinotari,
    payment_id: MemoField,
    output_features: OutputFeatures,
    sender: TariAddress,
    recipient: TariAddress,
) -> Result<PrepareWithdrawMultisigTransactionResult, TransactionBuilderError> {
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
    let canonical = borsh_canonical_one_sided(&version, tx_id, &info)
        .map_err(|e| TransactionBuilderError::Other(format!("borsh_canonical_one_sided failed: {e}")))?;
    let payload_signature = sign_payload(tx_builder.key_manager(), &canonical)?;

    Ok(PrepareWithdrawMultisigTransactionResult {
        version,
        tx_id,
        info,
        payload_signature,
    })
}

// ---------------------------------------------------------------------------
// Sign functions (offline signer side) — verify payload integrity first
// ---------------------------------------------------------------------------

pub fn sign_locked_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    request: PrepareOneSidedTransactionForSigningResult,
) -> Result<SignedOneSidedTransactionResult, TransactionBuilderError> {
    // Verify the payload was not tampered with between prepare and sign.
    let canonical = borsh_canonical_one_sided(&request.version, request.tx_id, &request.info)
        .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))?;
    verify_payload_signature(key_manager, &request.payload_signature, &canonical)?;

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
    request: PrepareDepositMultisigTransactionResult,
) -> Result<SignedOneSidedDepositMultisigTransactionResult, TransactionBuilderError> {
    // Verify the payload was not tampered with between prepare and sign.
    let canonical = borsh_canonical_multisig(&request.version, request.tx_id, &request.info)
        .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))?;
    verify_payload_signature(key_manager, &request.payload_signature, &canonical)?;

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
    request: PrepareWithdrawMultisigTransactionResult,
) -> Result<SignedOneSidedWithdrawMultisigTransactionResult, TransactionBuilderError> {
    // Verify the payload was not tampered with between prepare and sign.
    let canonical = borsh_canonical_one_sided(&request.version, request.tx_id, &request.info)
        .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))?;
    verify_payload_signature(key_manager, &request.payload_signature, &canonical)?;

    let signed_transaction =
        sign_multisig_withdraw_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedWithdrawMultisigTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}
