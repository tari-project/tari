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
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use rand::Rng;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedPublicKey, FixedHash},
};
use tari_script::{
    CompressedCheckSigSchnorrSignature,
    ExecutionStack,
    Opcode,
    StackItem,
    TariScript,
    push_pubkey_script,
};
use tari_utilities::ByteArray;
use uuid::Uuid;

use crate::{
    MicroMinotari,
    TransactionBuilder,
    TransactionBuilderError,
    consensus::ConsensusConstants,
    fee::{Fee, addressed_output_memo, recipient_output_features_and_scripts_size},
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    multisig::script::{derive_multisig_ephemeral_pubkeys, get_multi_sig_script_components},
    transaction_builder::FinalizedTransaction,
    transaction_components::{
        OutputFeatures,
        Transaction,
        TransactionError,
        WalletOutput,
        WalletOutputBuilder,
        covenants::Covenant,
        memo_field::{MemoField, TxType},
    },
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
            TxId,
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
        rand::rng().fill_bytes(message.as_mut());

        let user_data = uuid.as_bytes().to_vec();
        let fee_estimate = tx_builder.get_fee_estimate_without_change()?;
        let payment_id =
            MemoField::new_address_and_data(recipient.clone(), fee_estimate, true, TxType::PaymentToOther, user_data)
                .map_err(|e| TransactionError::BuilderError(format!("Failed to create MemoField: {}", e)))?;

        let sender_offset_key = self
            .key_manager
            .get_random_key(None, Some(LedgerKeyBranch::OneSidedSenderOffset))?;

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
            .sign_metadata_signature_user_verified(&self.key_manager, &sender_offset_key.key_id, &recipient)?
            .try_build(&self.key_manager)?;

        tx_builder.add_recipient(
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
            finalized_builder.tx_id,
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

        // The whole input goes to a single recipient output with no change output, and that output carries the memo
        // built below in its encrypted data, which the builder charges for. The real memo cannot be built yet
        // because it records the fee being calculated here, so measure a copy built with a zero fee - see
        // `addressed_output_memo`. Leaving the memo out is not a rounding nit: the build cannot balance at all.
        let measured_memo = addressed_output_memo(
            MemoField::default(),
            recipient.clone(),
            MicroMinotari::zero(),
            TxType::PaymentToOther,
        )?;
        let features_and_scripts_byte_size = recipient_output_features_and_scripts_size(
            consensus_constants.transaction_weight_params(),
            &OutputFeatures::default(),
            &script,
            &Covenant::default(),
            &measured_memo,
        )?;

        let fee: MicroMinotari = fee_calculator.calculate(fee_per_gram, 1, 1, 1, features_and_scripts_byte_size);
        // Record the actual fee, not the fee-per-gram: this memo is handed to the recipient and shown to the user as
        // what the transaction cost, and `TransactionBuilder::build` rewrites any memo whose recorded fee does not
        // match the fee it settled on.
        let payment_id = addressed_output_memo(MemoField::default(), recipient.clone(), fee, TxType::PaymentToOther)?;

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

        tx_builder.add_stealth_recipient(recipient, total_amount, OutputFeatures::default(), payment_id.clone())?;

        let tx = match tx_builder.build() {
            Ok(tx) => tx,
            Err(e) => {
                return Err(TransactionError::BuilderError(format!("Failed to build transaction: {:?}", e)).into());
            },
        };

        Ok((tx, payment_id, total_amount))
    }
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::tari_address::{TariAddress, TariAddressFeatures};
    use tari_script::{CompressedCheckSigSchnorrSignature, ExecutionStack, Opcode, TariScript};

    use crate::{
        MicroMinotari,
        key_manager::{KeyManager, TransactionKeyManagerInterface},
        multisig::session::MultisigSession,
        test_helpers::create_consensus_manager,
        transaction_components::{OutputFeatures, WalletOutputBuilder, memo_field::MemoField},
    };

    /// `spend_multisig_utxo` hands the whole input to a single recipient with no change output, and that output
    /// carries an `AddressAndData` memo (padded to a 130 byte minimum) in its encrypted data, which the transaction
    /// builder charges for. An estimate that leaves the memo out cannot be balanced at all, so the call fails
    /// outright; this drives the real call and checks the fee it settled on.
    #[test]
    fn spend_multisig_utxo_fee_estimate_counts_the_output_memo() {
        let rules = create_consensus_manager();
        let consensus_constants = rules.consensus_constants(0);
        let key_manager = KeyManager::new_random().unwrap();
        let recipient_key_manager = KeyManager::new_random().unwrap();
        let recipient = TariAddress::new_dual_address(
            recipient_key_manager.get_view_key().pub_key,
            recipient_key_manager.get_spend_key().pub_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        // A 1-of-1 multisig output, the script shape `get_multi_sig_script_components` looks for.
        let amount = MicroMinotari(100_000);
        let (commitment_mask, script_key) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
        let party_key = key_manager.get_random_key(None, None).unwrap();
        let sender_offset = key_manager.get_random_key(None, None).unwrap();
        let script = TariScript::new(vec![
            Opcode::CheckMultiSigVerify(1, 1, vec![party_key.pub_key], Box::new([0u8; 32])),
            Opcode::PushPubKey(Box::new(script_key.pub_key.clone())),
        ])
        .unwrap();
        let input = WalletOutputBuilder::new(amount, commitment_mask.key_id)
            .with_script(script)
            .with_features(OutputFeatures::default())
            .with_input_data(ExecutionStack::default())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::default())
            .unwrap()
            .with_script_key(script_key.key_id)
            .with_sender_offset_public_key(sender_offset.pub_key.clone())
            .sign_metadata_signature(&key_manager, &sender_offset.key_id)
            .unwrap()
            .try_build(&key_manager)
            .unwrap();

        let session = MultisigSession::new(key_manager);
        let (finalized, memo, total_amount) = session
            .spend_multisig_utxo(
                vec![CompressedCheckSigSchnorrSignature::default()],
                recipient,
                input,
                consensus_constants,
            )
            .unwrap();

        // The whole input is spent, so what the recipient does not get is exactly the fee the builder charged.
        assert_eq!(
            amount - total_amount,
            finalized.fee,
            "the up-front fee estimate must be exactly what the transaction builder charges"
        );
        // The memo really is the padded `AddressAndData` shape that has to be paid for.
        assert!(
            memo.get_size() >= 130,
            "expected a padded AddressAndData memo, got {} bytes",
            memo.get_size()
        );
        // And it records the fee actually charged, not the fee-per-gram, so the recipient is told what the
        // transaction really cost.
        assert_eq!(memo.get_fee(), Some(finalized.fee));
        assert_eq!(
            finalized.transaction.body.outputs().len(),
            1,
            "there must be no change output"
        );
    }
}
