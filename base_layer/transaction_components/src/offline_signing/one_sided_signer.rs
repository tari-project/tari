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
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
use std::str::FromStr;

use rand::Rng;
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedPublicKey, CompressedSignature, PrivateKey},
};
use tari_script::{ExecutionStack, Opcode, TariScript, push_pubkey_script};

use crate::{
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
    consensus::ConsensusConstants,
    fee::recipient_output_features_and_scripts_size,
    key_manager::{TariKeyAndId, TariKeyId, TransactionKeyManagerInterface, manager::require_mask_id},
    multisig::script::derive_multisig_ephemeral_pubkeys,
    offline_signing::models::{
        OneSidedMultisigTransactionInfo,
        OneSidedTransactionInfo,
        PaymentRecipient,
        SignedTransaction,
        TransactionMetadata,
    },
    transaction_builder::{FinalizedTransaction, PendingOutput},
    transaction_components::{
        MemoField,
        OutputFeatures,
        TransactionError,
        TransactionOutput,
        WalletOutput,
        WalletOutputBuilder,
        covenants::Covenant,
    },
};

/// The weight a multisig recipient output will contribute, measured before its script exists.
///
/// The script cannot be built until the sender offset key it derives its ephemeral keys from has been reserved, but
/// the reservation needs the weight of every output it is charging for. A placeholder of the same shape - one
/// `CheckMultiSigVerify` over `key_count` keys followed by one `PushPubKey` - serialises to the same size.
///
/// The online wallet uses it too, so that its fee and change estimate for the deposit is the one the signer will
/// commit to.
pub fn multisig_pending_output(
    consensus_constants: &ConsensusConstants,
    recipient: &TariAddress,
    amount: MicroMinotari,
    features: &OutputFeatures,
    memo: &MemoField,
    party_number: u8,
    key_count: usize,
) -> Result<PendingOutput, TransactionBuilderError> {
    let script = TariScript::new(vec![
        Opcode::CheckMultiSigVerify(
            party_number,
            u8::try_from(key_count).map_err(|e| TransactionBuilderError::Other(e.to_string()))?,
            vec![CompressedPublicKey::default(); key_count],
            Box::new([0u8; 32]),
        ),
        Opcode::PushPubKey(Box::default()),
    ])?;
    Ok(PendingOutput::new(
        amount,
        recipient_output_features_and_scripts_size(
            consensus_constants.transaction_weight_params(),
            features,
            &script,
            &Covenant::default(),
            memo,
        )?,
    )
    .for_recipient(recipient.clone()))
}

/// The declaration of a multisig withdrawal's recipient output, the way the withdrawal signer makes it. The online
/// wallet uses it too, so that its fee and change estimate for the withdrawal is the one the signer will commit to.
pub fn withdraw_pending_output(
    consensus_constants: &ConsensusConstants,
    recipient: &PaymentRecipient,
    memo: &MemoField,
) -> Result<PendingOutput, TransactionBuilderError> {
    Ok(PendingOutput::new(
        recipient.amount,
        recipient_output_features_and_scripts_size(
            consensus_constants.transaction_weight_params(),
            &recipient.output_features,
            &push_pubkey_script(&Default::default()),
            &Covenant::default(),
            memo,
        )?,
    )
    .for_recipient(recipient.address.clone()))
}

/// The longest key id string a payload may carry.
const MAX_PAYLOAD_KEY_ID_LEN: usize = 1024;
/// How many key ids a payload key id may wrap in total (`Encrypted.key`, `DH*.private_key` and `Derived.key` each wrap
/// one): the outer id plus a nested id at most two levels deep. The deepest key id the wallet itself produces is a
/// multisig input's script key, `derived.dh_commitment_mask.<pk>.encrypted.<hex>.view_key`: a script key `Derived`
/// from a `DH*` mask over an `Encrypted` sender offset key.
const MAX_PAYLOAD_KEY_ID_NESTING: usize = 3;

/// The key ids a commitment mask is made from, over the keys the wallet makes them from: see
/// [`require_mask_id`]. Anything else as a mask names a wallet root key (or a public tweak of one), or a secret
/// encrypted under the spend key (an imported output's script key), and the signer would encrypt it into the
/// output's recovery data.
fn is_mask_key_id(key_id: &TariKeyId) -> bool {
    require_mask_id(key_id).is_ok()
}

/// Whether `key_id` is within the payload size and nesting limits.
fn is_within_payload_limits(key_id: &TariKeyId) -> bool {
    if key_id.to_string().len() > MAX_PAYLOAD_KEY_ID_LEN {
        return false;
    }
    let mut current = key_id.clone();
    let mut nesting = 0usize;
    loop {
        let inner = match &current {
            TariKeyId::Derived { key } | TariKeyId::Encrypted { key, .. } => key.as_str().to_string(),
            TariKeyId::DHCommitmentMask { private_key, .. } | TariKeyId::DHEncryptedData { private_key, .. } => {
                private_key.as_str().to_string()
            },
            _ => return true,
        };
        nesting = nesting.saturating_add(1);
        if nesting > MAX_PAYLOAD_KEY_ID_NESTING {
            return false;
        }
        current = match TariKeyId::from_str(&inner) {
            Ok(key_id) => key_id,
            Err(_) => return false,
        };
    }
}

fn is_allowed_mask_id(key_id: &TariKeyId) -> bool {
    is_mask_key_id(key_id) && is_within_payload_limits(key_id)
}

/// A script key may be:
/// - any mask key id (see [`require_mask_id`]);
/// - `Derived` over a mask key id (an ordinary wallet script key);
/// - the spend key itself when the output's script is exactly `PushPubKey(<our public spend key>)`. That is how the
///   wallet holds non-stealth outputs (for example non-stealth coinbases), and it is the same rule `WalletOutput` uses
///   to pick the spend key as a script key;
/// - `Encrypted` under the spend key: an imported output's script key (`UnblindedOutput::to_wallet_output`).
///
/// The script key only feeds the script signature (under a random nonce) and the script offset (blinded by fresh
/// sender offset keys), exactly as when the output is spent online, so none of these gives the host anything. An
/// `Encrypted` under the spend key cannot be minted by the host (the cipher is keyed by the spend key), only replayed
/// from an imported output it saw; as a script key that replay does nothing new. As a *mask* it would be decrypted and
/// published in the output's recovery data, which is why [`require_mask_id`] refuses it there, and why `Derived` over
/// it is refused here.
fn is_allowed_script_key_id(key_id: &TariKeyId, script: &TariScript, public_spend_key: &CompressedPublicKey) -> bool {
    if !is_within_payload_limits(key_id) {
        return false;
    }
    match key_id {
        TariKeyId::SpendKey => matches!(script.as_slice(), [Opcode::PushPubKey(pk)] if **pk == *public_spend_key),
        TariKeyId::Derived { key } => TariKeyId::from_str(key.as_str()).is_ok_and(|inner| is_mask_key_id(&inner)),
        TariKeyId::Encrypted { key, .. } if matches!(TariKeyId::from_str(key.as_str()), Ok(TariKeyId::SpendKey)) => {
            true
        },
        _ => is_mask_key_id(key_id),
    }
}

/// Checks the key ids of every wallet output an offline signing payload carries, before any of them reaches the
/// builder or the key manager.
///
/// The payload is only integrity-signed with the view key, so whoever holds the view key (the online host) can
/// forge it, and the operator summary does not show key ids. A payload output whose commitment mask names the spend
/// key would otherwise make the signer encrypt the spend key into the output's recovery data, so masks must be one
/// of `Encrypted`, `DHCommitmentMask`, `DHEncryptedData` or `LedgerKey` over the keys [`require_mask_id`] allows (an
/// `Encrypted` under the spend key, such as an imported output's script key, is refused). Script keys may additionally
/// be `Derived` over one of those, or the spend key for a `PushPubKey(<our public spend key>)` script (see
/// `is_allowed_script_key_id`).
fn check_payload_key_ids(
    inputs: &[WalletOutput],
    outputs: &[WalletOutput],
    public_spend_key: &CompressedPublicKey,
) -> Result<(), TransactionBuilderError> {
    let lists = [("inputs", inputs), ("outputs", outputs)];
    for (list_name, list) in lists {
        for (i, output) in list.iter().enumerate() {
            let mask = output.commitment_mask_key_id();
            if !is_allowed_mask_id(mask) {
                return Err(TransactionBuilderError::OfflinePayloadKeyIdNotAllowed {
                    field: format!("{list_name}[{i}].commitment_mask_key_id"),
                    key_id: mask.to_string(),
                });
            }
            let script_key = output.script_key_id();
            if !is_allowed_script_key_id(script_key, output.script(), public_spend_key) {
                return Err(TransactionBuilderError::OfflinePayloadKeyIdNotAllowed {
                    field: format!("{list_name}[{i}].script_key_id"),
                    key_id: script_key.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientSignedMessage {
    pub tx_id: TxId,
    pub output: TransactionOutput,
    pub public_spend_key: CompressedPublicKey,
    pub partial_signature: CompressedSignature,
    pub tx_metadata: TransactionMetadata,
    pub offset: PrivateKey,
}

/// Fails with [`TransactionBuilderError::OfflineTransactionRequiresChange`] if the transaction would carry no change
/// output; see [`SignedTransaction`] for why.
pub fn build_and_sign_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    info: OneSidedTransactionInfo,
) -> Result<SignedTransaction, TransactionBuilderError> {
    check_payload_key_ids(&info.inputs, &info.outputs, &key_manager.get_spend_key().pub_key)?;
    let mut tx_builder = TransactionBuilder::new(consensus_constants, key_manager.clone(), network)?;
    if info.fee_per_gram > MicroMinotari::zero() {
        tx_builder.with_fee_per_gram(info.fee_per_gram);
    } else {
        tx_builder.with_fee(info.fee);
    }
    // Refuse, before any sender offset key is minted, a transaction without change: see `SignedTransaction`.
    tx_builder.with_change_required();

    for uo in info.inputs {
        tx_builder.with_input(uo)?;
    }

    // The recipients belong to the collecting phase: their outputs are built inside `build`, around keys that only
    // exist once the whole transaction's key count is known.
    for recipient in info.recipients {
        tx_builder.add_stealth_recipient(
            recipient.address.clone(),
            recipient.amount,
            recipient.output_features.clone(),
            recipient.payment_id.clone(),
        )?;
    }
    // The change memo carries the payment id, and the reservation has to measure the memo `build` will write.
    tx_builder.with_memo(info.payment_id.clone());
    // The payload's outputs arrive fully formed, so they cannot be specs; they are declared here instead, because
    // the single reservation has to charge for every output the transaction will carry.
    let pending = info
        .outputs
        .iter()
        .map(PendingOutput::from_output)
        .collect::<Result<Vec<_>, _>>()?;
    let sender_offset_keys = tx_builder.reserve_sender_offset_keys(&pending)?;
    for (mut uo, sender_offset_key) in info.outputs.into_iter().zip(sender_offset_keys) {
        uo.set_sender_offset_public_key(sender_offset_key.pub_key.clone());
        // Whatever signature the payload carried was made against the sender offset key we just replaced, so it can
        // no longer verify. Reduce it to the placeholder the builder recognises, so that it re-signs the output with
        // the key it will actually publish rather than carrying a dead signature into the transaction.
        uo.set_metadata_signature(Default::default());
        tx_builder.with_output(uo, sender_offset_key.key_id, None)?;
    }
    let finalized_tx = tx_builder.build()?;
    let FinalizedTransaction {
        transaction,
        sent_output_hashes,
        change_output_hashes,
        change,
        tx_id,
        sent_outputs,
        ..
    } = finalized_tx;
    let outputs = sent_outputs.iter().map(|o| o.output.clone()).collect();

    Ok(SignedTransaction {
        transaction,
        sent_hashes: sent_output_hashes,
        outputs,
        change_hashes: change_output_hashes,
        change_output: change,
        tx_id,
    })
}

/// Fails with [`TransactionBuilderError::OfflineTransactionRequiresChange`] if the transaction would carry no change
/// output; see [`SignedTransaction`] for why.
pub fn sign_multisig_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    mut info: OneSidedMultisigTransactionInfo,
) -> Result<SignedTransaction, TransactionBuilderError> {
    check_payload_key_ids(
        &info.base.inputs,
        &info.base.outputs,
        &key_manager.get_spend_key().pub_key,
    )?;
    let constants = consensus_constants.clone();
    let mut tx_builder = TransactionBuilder::new(consensus_constants, key_manager.clone(), network)?;
    if info.base.fee_per_gram > MicroMinotari::zero() {
        tx_builder.with_fee_per_gram(info.base.fee_per_gram);
    } else {
        tx_builder.with_fee(info.base.fee);
    }
    // Refuse, before any sender offset key is minted, a transaction without change: see `SignedTransaction`.
    tx_builder.with_change_required();

    for uo in std::mem::take(&mut info.base.inputs) {
        tx_builder.with_input(uo)?;
    }

    if info.base.recipients.len() != 1 {
        return Err(TransactionBuilderError::Other(
            "Only one recipient is supported for multisig transactions".to_string(),
        ));
    }
    let recipient = info
        .base
        .recipients
        .first()
        .ok_or(TransactionBuilderError::NoRecipients)?
        .clone();

    // Every output this transaction will carry is declared up front, because the sender offset keys are reserved in
    // a single call: splitting the reservation would leave the later calls with no input script keys to fold in, and
    // their replies would be bare sender offset private keys.
    // The change memo names the recipient and carries the payment id, so both are declared before the reservation
    // measures it.
    tx_builder.with_memo(info.base.payment_id.clone());
    let mut pending = vec![multisig_pending_output(
        &constants,
        &recipient.address,
        recipient.amount,
        &recipient.output_features,
        &info.payment_id,
        info.party_number,
        info.public_keys.len(),
    )?];
    for uo in &info.base.outputs {
        pending.push(PendingOutput::from_output(uo)?);
    }
    let mut sender_offset_keys = tx_builder.reserve_sender_offset_keys(&pending)?.into_iter();

    let sender_offset = sender_offset_keys
        .next()
        .ok_or(TransactionBuilderError::SenderOffsetKeyPoolExhausted)?;
    let output = build_multisig_output(key_manager, &info, &sender_offset)?;

    for mut uo in info.base.outputs {
        let sender_offset_key = sender_offset_keys
            .next()
            .ok_or(TransactionBuilderError::SenderOffsetKeyPoolExhausted)?;
        uo.set_sender_offset_public_key(sender_offset_key.pub_key.clone());
        // Whatever signature the payload carried was made against the sender offset key we just replaced, so it can
        // no longer verify. Reduce it to the placeholder the builder recognises, so that it re-signs the output with
        // the key it will actually publish rather than carrying a dead signature into the transaction.
        uo.set_metadata_signature(Default::default());
        tx_builder.with_output(uo, sender_offset_key.key_id, None)?;
    }
    tx_builder.add_recipient(recipient.address.clone(), output, sender_offset.key_id, None)?;

    let finalized_tx = tx_builder.build()?;
    let FinalizedTransaction {
        transaction,
        sent_output_hashes,
        change_output_hashes,
        change,
        tx_id,
        sent_outputs,
        ..
    } = finalized_tx;
    let outputs = sent_outputs.iter().map(|o| o.output.clone()).collect();

    Ok(SignedTransaction {
        transaction,
        sent_hashes: sent_output_hashes,
        outputs,
        change_hashes: change_output_hashes,
        change_output: change,
        tx_id,
    })
}

/// Build the multisig recipient output around a sender offset key reserved from the transaction builder.
fn build_multisig_output<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    info: &OneSidedMultisigTransactionInfo,
    sender_offset_key: &TariKeyAndId,
) -> Result<WalletOutput, TransactionBuilderError> {
    if info.base.recipients.len() != 1 {
        return Err(TransactionBuilderError::Other(
            "Only one recipient is supported for multisig transactions".to_string(),
        ));
    }
    let recipient = &info.recipients.first().ok_or(TransactionBuilderError::NoRecipients)?;

    let (_commitment_mask, script_key) = key_manager.get_next_commitment_mask_and_script_key()?;

    let sender_offset_public_key = key_manager.get_public_key_at_key_id(&sender_offset_key.key_id)?;

    let recipient_view_key = recipient.address.public_spend_key();
    let recipient_spend_key = recipient.address.public_spend_key();
    let commitment_mask_key_id = TariKeyId::DHCommitmentMask {
        private_key: sender_offset_key.key_id.clone().into(),
        public_key: recipient_view_key.clone(),
    };

    let encryption_key = TariKeyId::DHEncryptedData {
        private_key: sender_offset_key.key_id.clone().into(),
        public_key: recipient_view_key.clone(),
    };
    let script_pubkey =
        key_manager.stealth_address_script_spending_key(&commitment_mask_key_id, recipient_spend_key)?;

    let mut message = Box::new([0u8; 32]);
    rand::rng().fill_bytes(message.as_mut());

    let ephemeral_pubkeys =
        derive_multisig_ephemeral_pubkeys(key_manager, &info.public_keys, &sender_offset_key.key_id)?;

    let mut script_opcodes = vec![Opcode::CheckMultiSigVerify(
        info.party_number,
        u8::try_from(ephemeral_pubkeys.len()).expect("Is checked"),
        ephemeral_pubkeys.clone(),
        message,
    )];

    script_opcodes.push(Opcode::PushPubKey(script_pubkey.into()));

    let full_script = TariScript::new(script_opcodes)?;

    let output = WalletOutputBuilder::new(recipient.amount, commitment_mask_key_id.clone())
        .with_script(full_script.clone())
        .with_features(recipient.output_features.clone())
        .with_input_data(Default::default())
        .encrypt_data_for_recovery(key_manager, Some(&encryption_key), info.payment_id.clone())?
        .with_script_key(script_key.key_id)
        .with_sender_offset_public_key(sender_offset_public_key.clone())
        .sign_metadata_signature(key_manager, &sender_offset_key.key_id)?
        .try_build(key_manager)?;
    Ok(output)
}

/// Fails with [`TransactionBuilderError::OfflineTransactionRequiresChange`] if the transaction would carry no change
/// output; see [`SignedTransaction`] for why.
pub fn sign_multisig_withdraw_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    mut info: OneSidedTransactionInfo,
) -> Result<SignedTransaction, TransactionBuilderError> {
    check_payload_key_ids(&info.inputs, &info.outputs, &key_manager.get_spend_key().pub_key)?;
    let constants = consensus_constants.clone();
    let mut tx_builder = TransactionBuilder::new(consensus_constants, key_manager.clone(), network)?;
    if info.fee_per_gram > MicroMinotari::zero() {
        tx_builder.with_fee_per_gram(info.fee_per_gram);
    } else {
        tx_builder.with_fee(info.fee);
    }
    // Refuse, before any sender offset key is minted, a transaction without change: see `SignedTransaction`.
    tx_builder.with_change_required();

    for uo in std::mem::take(&mut info.inputs) {
        tx_builder.with_input(uo)?;
    }

    if info.recipients.len() != 1 {
        return Err(TransactionBuilderError::Other(
            "Only one recipient is supported for multisig transactions".to_string(),
        ));
    }
    let recipient = info
        .recipients
        .first()
        .ok_or(TransactionBuilderError::NoRecipients)?
        .clone();

    // As above: one reservation for every output, declared before any key exists, and the memo and the recipient
    // declared before the reservation measures the change memo.
    tx_builder.with_memo(info.payment_id.clone());
    let mut pending = vec![withdraw_pending_output(&constants, &recipient, &info.payment_id)?];
    for uo in &info.outputs {
        pending.push(PendingOutput::from_output(uo)?);
    }
    let mut sender_offset_keys = tx_builder.reserve_sender_offset_keys(&pending)?.into_iter();

    let sender_offset = sender_offset_keys
        .next()
        .ok_or(TransactionBuilderError::SenderOffsetKeyPoolExhausted)?;
    let output = build_multisig_withdraw_output(key_manager, &info, &sender_offset)?;

    for mut uo in info.outputs {
        let sender_offset_key = sender_offset_keys
            .next()
            .ok_or(TransactionBuilderError::SenderOffsetKeyPoolExhausted)?;
        uo.set_sender_offset_public_key(sender_offset_key.pub_key.clone());
        // Whatever signature the payload carried was made against the sender offset key we just replaced, so it can
        // no longer verify. Reduce it to the placeholder the builder recognises, so that it re-signs the output with
        // the key it will actually publish rather than carrying a dead signature into the transaction.
        uo.set_metadata_signature(Default::default());
        tx_builder.with_output(uo, sender_offset_key.key_id, None)?;
    }
    tx_builder.add_recipient(recipient.address.clone(), output, sender_offset.key_id, None)?;

    let finalized_tx = tx_builder.build()?;
    let FinalizedTransaction {
        transaction,
        sent_output_hashes,
        change_output_hashes,
        change,
        tx_id,
        sent_outputs,
        ..
    } = finalized_tx;
    let outputs = sent_outputs.iter().map(|o| o.output.clone()).collect();

    Ok(SignedTransaction {
        transaction,
        sent_hashes: sent_output_hashes,
        outputs,
        change_hashes: change_output_hashes,
        change_output: change,
        tx_id,
    })
}

/// Build the multisig withdrawal recipient output around a sender offset key reserved from the transaction builder.
fn build_multisig_withdraw_output<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    info: &OneSidedTransactionInfo,
    sender_offset_key: &TariKeyAndId,
) -> Result<WalletOutput, TransactionBuilderError> {
    if info.recipients.len() != 1 {
        return Err(TransactionBuilderError::Other(
            "Only one recipient is supported for multisig transactions".to_string(),
        ));
    }
    let recipient = &info.recipients.first().ok_or(TransactionBuilderError::NoRecipients)?;

    let (_commitment_mask_key, script_key) = key_manager.get_next_commitment_mask_and_script_key()?;

    let sender_offset_public_key = key_manager.get_public_key_at_key_id(&sender_offset_key.key_id)?;

    let commitment_mask_key_id = TariKeyId::DHCommitmentMask {
        private_key: sender_offset_key.key_id.clone().into(),
        public_key: recipient
            .address
            .public_view_key()
            .ok_or(TransactionError::BuilderError("Missing public view key".to_string()))?
            .clone(),
    };

    let encryption_key = TariKeyId::DHEncryptedData {
        private_key: sender_offset_key.key_id.clone().into(),
        public_key: recipient
            .address
            .public_view_key()
            .ok_or(TransactionError::BuilderError("Missing public view key".to_string()))?
            .clone(),
    };

    let script_spending_key = key_manager
        .clone()
        .stealth_address_script_spending_key(&commitment_mask_key_id, recipient.address.public_spend_key())?;

    let script = push_pubkey_script(&script_spending_key);

    let output = WalletOutputBuilder::new(recipient.amount, commitment_mask_key_id.clone())
        .with_script(script.clone())
        .with_features(recipient.output_features.clone())
        .with_input_data(ExecutionStack::default())
        .encrypt_data_for_recovery(key_manager, Some(&encryption_key), info.payment_id.clone())?
        .with_script_key(script_key.key_id)
        .with_sender_offset_public_key(sender_offset_public_key.clone())
        .sign_metadata_signature(key_manager, &sender_offset_key.key_id)?
        .try_build(key_manager)?;
    Ok(output)
}
