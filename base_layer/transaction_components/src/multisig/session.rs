// Copyright 2025 The Tari Project
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
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    tari_address::TariAddress,
    types::{CompressedPublicKey, FixedHash},
};
use tari_script::{
    push_pubkey_script,
    CompressedCheckSigSchnorrSignature,
    ExecutionStack,
    Opcode,
    StackItem,
    TariScript,
};
use tari_utilities::ByteArray;
use uuid::Uuid;

use crate::{
    consensus::ConsensusConstants,
    fee::Fee,
    helpers::borsh::SerializedSize,
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    multisig::script::{derive_multisig_ephemeral_pubkeys, get_multi_sig_script_components},
    transaction_builder::FinalizedTransaction,
    transaction_components::{
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        OutputFeatures,
        Transaction,
        TransactionError,
        WalletOutput,
        WalletOutputBuilder,
    },
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
};

pub struct MultisigSession<TKeyManagerInterface> {
    key_manager: TKeyManagerInterface,
}

impl<TKeyManagerInterface> MultisigSession<TKeyManagerInterface>
where TKeyManagerInterface: TransactionKeyManagerInterface
{
    pub fn new(key_manager: TKeyManagerInterface) -> Self {
        MultisigSession { key_manager }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_deposit_multisig_transaction(
        &mut self,
        amount: MicroMinotari,
        party_number: u8,
        public_keys: Vec<CompressedPublicKey>,
        recipient: TariAddress,
        mut tx_builder: TransactionBuilder<TKeyManagerInterface>,
        uuid: Uuid,
    ) -> Result<
        (
            Transaction,
            MemoField,
            Vec<FixedHash>,
            Vec<FixedHash>,
            Option<Vec<WalletOutput>>,
        ),
        TransactionBuilderError,
    > {
        if party_number == 0 || (party_number as usize) > public_keys.len() {
            return Err(TransactionError::BuilderError(format!(
                "Invalid multisig threshold party_number={}, participants={}",
                party_number,
                public_keys.len()
            ))
            .into());
        }

        let mut message = Box::new([0u8; 32]);
        OsRng.fill_bytes(message.as_mut());

        let user_data = uuid.as_bytes().to_vec();
        let fee_estimate = tx_builder.get_fee_estimate_without_change()?;
        let payment_id =
            MemoField::new_address_and_data(recipient.clone(), fee_estimate, true, TxType::PaymentToOther, user_data)
                .map_err(|e| TransactionError::BuilderError(format!("Failed to create MemoField: {}", e)))?;

        let sender_offset_key = self
            .key_manager.get_random_key(None, true)?;

        let recipient_spend_key = recipient.public_spend_key();

        let sender_offset_public_key = sender_offset_key.pub_key;

        let commitment_mask_key_id = TariKeyId::DHCommitmentMask {
            private_key: sender_offset_key.key_id.clone().into(),
            public_key: recipient
                .public_view_key()
                .ok_or(TransactionBuilderError::InvalidAddressNoViewKey)?
                .clone(),
        };

        let encryption_key_id = TariKeyId::DHEncryptedData {
            private_key: sender_offset_key.key_id.clone().into(),
            public_key: recipient
                .public_view_key()
                .ok_or(TransactionBuilderError::InvalidAddressNoViewKey)?
                .clone(),
        };

        let ephemeral_pubkeys =
            derive_multisig_ephemeral_pubkeys(&self.key_manager, &public_keys, &sender_offset_key.key_id)?;

        let mut script_opcodes = vec![Opcode::CheckMultiSigVerify(
            party_number,
            u8::try_from(ephemeral_pubkeys.len()).expect("Is checked"),
            ephemeral_pubkeys.clone(),
            message,
        )];

        let script_pubkey = self
            .key_manager
            .stealth_address_script_spending_key(&commitment_mask_key_id, recipient_spend_key)?;

        script_opcodes.push(Opcode::PushPubKey(script_pubkey.clone().into()));

        let final_script = TariScript::new(script_opcodes)?;

        let output = WalletOutputBuilder::new(amount, commitment_mask_key_id.clone())
            .with_script(final_script.clone())
            .with_features(OutputFeatures::default())
            .with_input_data(ExecutionStack::default())
            .encrypt_data_for_recovery(&self.key_manager, Some(&encryption_key_id), payment_id.clone())?
            .with_script_key(TariKeyId::Zero)
            .with_sender_offset_public_key(sender_offset_public_key.clone())
            .sign_as_sender_and_receiver_verified(&mut self.key_manager, &sender_offset_key.key_id, &recipient)?
            .try_build(&self.key_manager)?;

        tx_builder
            .add_recipient(
                recipient,
                output.clone(),
                Some(sender_offset_key.key_id),
                Some(encryption_key_id),
            )?;

        let finalized_builder = tx_builder.build()?;

        let (change_hashes, change) = match finalized_builder.change {
            Some(change_output) => {
                let hash = change_output.output_hash();
                (vec![hash], Some(vec![change_output]))
            },
            None => (vec![], None),
        };

        let sent_hashes = vec![output.output_hash()];

        Ok((
            finalized_builder.transaction,
            payment_id,
            sent_hashes,
            change_hashes,
            change,
        ))
    }

    #[allow(clippy::too_many_lines)]
    pub fn spend_multisig_utxo(
        &self,
        signatures: Vec<CompressedCheckSigSchnorrSignature>,
        recipient: TariAddress,
        output: WalletOutput,
        consensus_constants: &ConsensusConstants,
    ) -> Result<(FinalizedTransaction, MemoField, MicroMinotari), TransactionBuilderError> {
        // Enforce correct signature count and ordering for the multisig script
        let (_ephemeral_pubkeys, threshold) = get_multi_sig_script_components(output.script())
            .ok_or(TransactionError::BuilderError("no keys found".to_string()))?;
        if signatures.len() < threshold as usize {
            return Err(TransactionError::BuilderError(format!(
                "Insufficient signatures: need at least {}, got {}",
                threshold,
                signatures.len()
            ))
            .into());
        }

        let mut input_stack = ExecutionStack::default();
        for sig in signatures {
            input_stack
                .push(StackItem::Signature(sig))
                .map_err(|e| TransactionError::BuilderError(format!("Failed to push signature: {}", e)))?;
        }

        let mut input_wallet_output = output.clone();
        input_wallet_output.set_input_data(input_stack);

        let key_manager = self.key_manager.clone();
        let amount = output.value();

        let mut tx_builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), recipient.network())?;
        let fee_calculator = Fee::new(*consensus_constants.transaction_weight_params());
        let fee_per_gram = MicroMinotari::from(1);
        let script = push_pubkey_script(&Default::default());

        let features_and_scripts_byte_size = consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                OutputFeatures::default()
                    .get_serialized_size()
                    .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))? +
                    script
                        .get_serialized_size()
                        .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))? +
                    Covenant::default()
                        .get_serialized_size()
                        .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
            );

        let fee: MicroMinotari = fee_calculator.calculate(fee_per_gram, 1, 1, 1, features_and_scripts_byte_size);
        let payment_id =
            MemoField::new_address_and_data(recipient.clone(), fee_per_gram, true, TxType::PaymentToOther, vec![])
                .map_err(|e| TransactionError::BuilderError(format!("Failed to create MemoField: {}", e)))?;

        if fee > amount {
            return Err(TransactionError::BuilderError(format!(
                "insufficient funds: fee: {}, amount: {}",
                fee, amount
            ))
            .into());
        }

        let total_amount = amount
            .checked_sub(fee)
            .ok_or(TransactionError::BuilderError("Amount too small to cover fee".into()))?;

        tx_builder.with_input(input_wallet_output)?;
        tx_builder.with_fee_per_gram(fee_per_gram);
        tx_builder.with_lock_height(0);

        tx_builder
            .add_stealth_recipient(recipient, total_amount, OutputFeatures::default(), payment_id.clone())?;

        let tx = match tx_builder.build() {
            Ok(tx) => tx,
            Err(e) => {
                return Err(TransactionError::BuilderError(format!("Failed to build transaction: {:?}", e)).into());
            },
        };

        Ok((tx, payment_id, total_amount))
    }
}
