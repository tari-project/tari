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

use crate::{
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
    consensus::ConsensusConstants,
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
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
            TransactionResult,
            canonical_payload_bytes,
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
/// payload.  The challenge is a domain-separated Blake2b-512 hash of the
/// canonical payload bytes (the full JSON with the `payload_signature` field
/// stripped out so the signed data is stable).
fn payload_challenge(canonical: &[u8]) -> [u8; 64] {
    let hash = DomainSeparatedHasher::<Blake2b<U64>, OfflineSigningPayloadHashDomain>::new_with_label(
        "offline_signing_payload",
    )
    .chain_update(canonical)
    .finalize();
    let mut challenge = [0u8; 64];
    challenge.copy_from_slice(hash.as_ref());
    challenge
}

/// Signs the serialised `Prepare*` payload with the wallet's view private key.
///
/// The caller is responsible for passing the *serialised-without-signature*
/// bytes as `canonical`.  The resulting [`PayloadIntegritySignature`] must be
/// embedded into the struct before it is handed to the offline signer.
fn sign_payload<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    canonical: &[u8],
) -> Result<PayloadIntegritySignature, TransactionBuilderError> {
    let challenge = payload_challenge(canonical);
    let view_key = key_manager.get_view_key();
    let nonce_key = key_manager.get_random_key(None, None)?;
    let signature = key_manager.sign_with_nonce_and_challenge(&view_key.key_id, &nonce_key.key_id, &challenge)?;
    let view_public_key = key_manager.get_public_key_at_key_id(&view_key.key_id)?;
    Ok(PayloadIntegritySignature {
        view_public_key,
        signature,
    })
}

/// Verifies the [`PayloadIntegritySignature`] embedded in a `Prepare*` result.
///
/// Returns `Ok(())` when:
/// 1. The `view_public_key` in the payload matches the key manager's own view public key.
/// 2. The Schnorr signature over the canonical payload bytes is valid.
///
/// Returns an error if either check fails, and the caller MUST abort signing.
fn verify_payload_signature<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    payload_sig: &PayloadIntegritySignature,
    canonical: &[u8],
) -> Result<(), TransactionBuilderError> {
    // 1. Key identity check — ensure the payload was prepared by *this* wallet's
    //    view key, not by some other key the attacker injected.
    let expected_view_pub = key_manager.get_public_key_at_key_id(&TariKeyId::ViewKey)?;
    if payload_sig.view_public_key != expected_view_pub {
        return Err(TransactionBuilderError::Other(
            "Offline payload integrity check failed: view public key in payload does not match this wallet's view key. \
             The payload may have been tampered with in transit."
                .to_string(),
        ));
    }

    // 2. Signature check — verify the Schnorr signature over the canonical bytes.
    let challenge = payload_challenge(canonical);
    let pub_key = payload_sig
        .view_public_key
        .to_public_key()
        .map_err(|e| TransactionBuilderError::Other(format!("Invalid view public key in payload: {e}")))?;
    let sig = payload_sig
        .signature
        .to_schnorr_signature()
        .map_err(|e| TransactionBuilderError::Other(format!("Invalid signature in payload: {e}")))?;
    if !sig.verify_raw_uniform(&pub_key, &challenge[..]) {
        return Err(TransactionBuilderError::Other(
            "Offline payload integrity check failed: payload signature is invalid. \
             The payload was tampered with in transit or is corrupt."
                .to_string(),
        ));
    }

    Ok(())
}

/// Serialises `result` (which must already contain a valid `payload_signature`)
/// to JSON, strips the signature field, and returns the canonical bytes that
/// were originally signed.  Used by `verify_payload_signature`.
fn canonical_bytes_of<T: TransactionResult>(result: &T) -> Result<Vec<u8>, TransactionBuilderError> {
    let json = result
        .to_json()
        .map_err(|e| TransactionBuilderError::Other(format!("Failed to serialise result for verification: {e}")))?;
    canonical_payload_bytes(&json)
        .map_err(|e| TransactionBuilderError::Other(format!("Failed to compute canonical bytes: {e}")))
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

    // Build an incomplete result (placeholder signature) so we can serialise it
    // to obtain the canonical bytes that will be signed.
    let placeholder_sig = PayloadIntegritySignature {
        view_public_key: CompressedPublicKey::default(),
        signature: Default::default(),
    };
    let partial = PrepareOneSidedTransactionForSigningResult {
        version: get_latest_version(),
        tx_id,
        info,
        payload_signature: placeholder_sig,
    };

    // Serialise → strip signature field → hash → sign.
    let json = partial
        .to_json()
        .map_err(|e| TransactionBuilderError::Other(format!("Serialisation failed: {e}")))?;
    let canonical = canonical_payload_bytes(&json)
        .map_err(|e| TransactionBuilderError::Other(format!("canonical_payload_bytes failed: {e}")))?;
    let payload_signature = sign_payload(tx_builder.key_manager(), &canonical)?;

    Ok(PrepareOneSidedTransactionForSigningResult {
        payload_signature,
        ..partial
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

    let placeholder_sig = PayloadIntegritySignature {
        view_public_key: CompressedPublicKey::default(),
        signature: Default::default(),
    };
    let partial = PrepareDepositMultisigTransactionResult {
        version: get_latest_version(),
        tx_id,
        info,
        payload_signature: placeholder_sig,
    };

    let json = partial
        .to_json()
        .map_err(|e| TransactionBuilderError::Other(format!("Serialisation failed: {e}")))?;
    let canonical = canonical_payload_bytes(&json)
        .map_err(|e| TransactionBuilderError::Other(format!("canonical_payload_bytes failed: {e}")))?;
    let payload_signature = sign_payload(tx_builder.key_manager(), &canonical)?;

    Ok(PrepareDepositMultisigTransactionResult {
        payload_signature,
        ..partial
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

    let placeholder_sig = PayloadIntegritySignature {
        view_public_key: CompressedPublicKey::default(),
        signature: Default::default(),
    };
    let partial = PrepareWithdrawMultisigTransactionResult {
        version: get_latest_version(),
        tx_id,
        info,
        payload_signature: placeholder_sig,
    };

    let json = partial
        .to_json()
        .map_err(|e| TransactionBuilderError::Other(format!("Serialisation failed: {e}")))?;
    let canonical = canonical_payload_bytes(&json)
        .map_err(|e| TransactionBuilderError::Other(format!("canonical_payload_bytes failed: {e}")))?;
    let payload_signature = sign_payload(tx_builder.key_manager(), &canonical)?;

    Ok(PrepareWithdrawMultisigTransactionResult {
        payload_signature,
        ..partial
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
    let canonical = canonical_bytes_of(&request)?;
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
    let canonical = canonical_bytes_of(&request)?;
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
    let canonical = canonical_bytes_of(&request)?;
    verify_payload_signature(key_manager, &request.payload_signature, &canonical)?;

    let signed_transaction =
        sign_multisig_withdraw_transaction(key_manager, consensus_constants, network, request.info.clone())?;

    Ok(SignedOneSidedWithdrawMultisigTransactionResult {
        version: get_latest_version(),
        request,
        signed_transaction,
    })
}
