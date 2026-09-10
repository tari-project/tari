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
use rand::Rng;
use tari_common::configuration::Network;
use tari_common_types::{
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
    key_manager::{TariKeyAndId, TariKeyId, TransactionKeyManagerInterface},
    multisig::script::derive_multisig_ephemeral_pubkeys,
    offline_signing::models::{
        OneSidedMultisigTransactionInfo,
        OneSidedTransactionInfo,
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
fn multisig_pending_output(
    consensus_constants: &ConsensusConstants,
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
    ))
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

pub fn build_and_sign_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    info: OneSidedTransactionInfo,
) -> Result<SignedTransaction, TransactionBuilderError> {
    let mut tx_builder = TransactionBuilder::new(consensus_constants, key_manager.clone(), network)?;
    if info.fee_per_gram > MicroMinotari::zero() {
        tx_builder.with_fee_per_gram(info.fee_per_gram);
    } else {
        tx_builder.with_fee(info.fee);
    }

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
    tx_builder.with_memo(info.payment_id.clone());
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

pub fn sign_multisig_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    mut info: OneSidedMultisigTransactionInfo,
) -> Result<SignedTransaction, TransactionBuilderError> {
    let constants = consensus_constants.clone();
    let mut tx_builder = TransactionBuilder::new(consensus_constants, key_manager.clone(), network)?;
    if info.base.fee_per_gram > MicroMinotari::zero() {
        tx_builder.with_fee_per_gram(info.base.fee_per_gram);
    } else {
        tx_builder.with_fee(info.base.fee);
    }

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
    let mut pending = vec![multisig_pending_output(
        &constants,
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

    tx_builder.with_memo(info.base.payment_id.clone());
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

pub fn sign_multisig_withdraw_transaction<KM: TransactionKeyManagerInterface>(
    key_manager: &KM,
    consensus_constants: ConsensusConstants,
    network: Network,
    mut info: OneSidedTransactionInfo,
) -> Result<SignedTransaction, TransactionBuilderError> {
    let constants = consensus_constants.clone();
    let mut tx_builder = TransactionBuilder::new(consensus_constants, key_manager.clone(), network)?;
    if info.fee_per_gram > MicroMinotari::zero() {
        tx_builder.with_fee_per_gram(info.fee_per_gram);
    } else {
        tx_builder.with_fee(info.fee);
    }

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

    // As above: one reservation for every output, declared before any key exists.
    let mut pending = vec![PendingOutput::new(
        recipient.amount,
        recipient_output_features_and_scripts_size(
            constants.transaction_weight_params(),
            &recipient.output_features,
            &push_pubkey_script(&Default::default()),
            &Covenant::default(),
            &info.payment_id,
        )?,
    )];
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

    tx_builder.with_memo(info.payment_id.clone());
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
