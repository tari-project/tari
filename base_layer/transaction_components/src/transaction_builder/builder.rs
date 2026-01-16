// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, fmt::Debug};

use log::*;
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::TxId,
    types::{
        ComAndPubSignature,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        PrivateKey,
        UncompressedPublicKey,
        UncompressedSignature,
    },
};
use tari_script::{push_pubkey_script, script, ExecutionStack};
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    consensus::ConsensusConstants,
    fee::Fee,
    helpers::borsh::SerializedSize,
    key_manager::{TariKeyId, TransactionKeyManagerInterface, TxoStage},
    transaction_builder::{
        error::TransactionBuilderError,
        models::{FinalizedTransaction, OutputPair, RecipientDetails},
    },
    transaction_components::{
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        CoreTransactionBuilder,
        KernelBuilder,
        KernelFeatures,
        OutputFeatures,
        TransactionKernel,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
        WalletOutput,
        WalletOutputBuilder,
        MAX_TRANSACTION_INPUTS,
        MAX_TRANSACTION_OUTPUTS,
    },
    tx_outputs_to_tx_id,
    MicroMinotari,
};

pub const LOG_TARGET: &str = "c::tx::tx_builder";

#[derive(Clone)]
pub struct TransactionBuilder<KM> {
    consensus_constants: ConsensusConstants,
    key_manager: KM,
    fee_per_gram: Option<MicroMinotari>,
    fee: MicroMinotari,
    recipient_outputs: Vec<RecipientDetails>,
    inputs: Vec<OutputPair>,
    custom_outputs: Vec<OutputPair>,
    prevent_fee_gt_amount: bool,
    tx_type: TxType,
    memo_field: Option<MemoField>,
    lock_height: u64,
    kernel_features: KernelFeatures,
    burn_commitment: Option<CompressedCommitment>,
    own_address: TariAddress,
}

impl<KM> TransactionBuilder<KM>
where KM: TransactionKeyManagerInterface
{
    pub fn new(
        consensus_constants: ConsensusConstants,
        key_manager: KM,
        network: Network,
    ) -> Result<Self, TransactionBuilderError> {
        let view_key = key_manager.get_view_key();
        let spend_key = key_manager.get_spend_key();
        let own_address = TariAddress::new_dual_address(
            view_key.pub_key.clone(),
            spend_key.pub_key.clone(),
            network,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )?;

        Ok(Self {
            consensus_constants,
            key_manager,
            fee_per_gram: None,
            fee: MicroMinotari::zero(),
            recipient_outputs: Vec::new(),
            inputs: Vec::new(),
            custom_outputs: Vec::new(),
            prevent_fee_gt_amount: true,
            tx_type: TxType::PaymentToOther,
            memo_field: None,
            lock_height: 0,
            kernel_features: KernelFeatures::empty(),
            burn_commitment: None,
            own_address,
        })
    }

    /// Set the fee per weight for the transaction. See (Fee::calculate)[Struct.Fee.html#calculate] for how the
    /// absolute fee is calculated from the fee-per-gram value. This will take precedence over the fee set by
    /// `with_fee`.
    pub fn with_fee_per_gram(&mut self, fee_per_gram: MicroMinotari) -> &mut Self {
        self.fee_per_gram = Some(fee_per_gram);
        self
    }

    pub fn fee(&self) -> MicroMinotari {
        self.fee
    }

    pub fn fee_per_gram(&self) -> Option<MicroMinotari> {
        self.fee_per_gram
    }

    pub fn with_lock_height(&mut self, lock_height: u64) -> &mut Self {
        self.lock_height = lock_height;
        self
    }

    /// Sets the fee of the transaction. Fee per gram takes precedence over this value.
    pub fn with_fee(&mut self, fee: MicroMinotari) -> &mut Self {
        self.fee = fee;
        self
    }

    /// Sets the transaction type, default is TxType::PaymentToOther
    pub fn with_tx_type(&mut self, tx_type: TxType) -> &mut Self {
        self.tx_type = tx_type;
        self
    }

    /// Sets the payment id for the transaction. This is used to identify the transaction and is included in the
    /// transaction metadata.
    pub fn with_memo(&mut self, memo: MemoField) -> &mut Self {
        self.memo_field = Some(memo);
        self
    }

    pub fn with_kernel_features(&mut self, kernel_features: KernelFeatures) -> &mut Self {
        self.kernel_features = kernel_features;
        self
    }

    /// Add a recipient to the transaction.
    pub fn add_recipient(
        &mut self,
        recipient_address: TariAddress,
        recipient_output: WalletOutput,
        sender_offset_key_id: Option<TariKeyId>,
        custom_recovery_key_id: Option<TariKeyId>,
    ) -> Result<&mut Self, TransactionBuilderError> {
        let kernel_nonce = self.key_manager.get_random_key(None, None)?;
        let recipient_output = OutputPair::new(
            recipient_output,
            kernel_nonce.key_id,
            sender_offset_key_id,
            custom_recovery_key_id,
        );
        let recipient_details = RecipientDetails {
            output: recipient_output,
            recipient_address,
        };
        self.recipient_outputs.push(recipient_details);
        Ok(self)
    }

    pub fn add_stealth_recipient(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        output_features: OutputFeatures,
        memo_field: MemoField,
    ) -> Result<(), TransactionBuilderError> {
        // if this is a ledger wallet, this needs to come from the ledger as it needs to sign with this key for the
        // metadata signatures
        let sender_offset_private_key = self
            .key_manager
            .get_random_key(None, Some(LedgerKeyBranch::OneSidedSenderOffset))?;

        let commitment_mask_key_id = TariKeyId::DHCommitmentMask {
            private_key: sender_offset_private_key.key_id.clone().into(),
            public_key: destination
                .public_view_key()
                .ok_or(TransactionBuilderError::InvalidAddressNoViewKey)?
                .clone(),
        };

        let encryption_key = TariKeyId::DHEncryptedData {
            private_key: sender_offset_private_key.key_id.clone().into(),
            public_key: destination
                .public_view_key()
                .ok_or(TransactionBuilderError::InvalidAddressNoViewKey)?
                .clone(),
        };
        let script_spending_key = self
            .key_manager
            .stealth_address_script_spending_key(&commitment_mask_key_id, destination.public_spend_key())?;
        let script = push_pubkey_script(&script_spending_key);

        let sender_offset_public_key = self
            .key_manager
            .get_public_key_at_key_id(&sender_offset_private_key.key_id)?;

        let minimum_value_promise = MicroMinotari::zero();
        let output = WalletOutputBuilder::new(amount, commitment_mask_key_id)
            .with_features(output_features)
            .with_script(script)
            .encrypt_data_for_recovery(&self.key_manager, Some(&encryption_key), memo_field)?
            .with_input_data(Default::default())
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(TariKeyId::Zero)
            .with_minimum_value_promise(minimum_value_promise)
            //We add a placeholder so that we only sign the metadata signature once all fees are calculated, and the user if they are on a ledger, only gets the prompt once
            .with_place_holder_metadata_signature(&self.key_manager, &sender_offset_private_key.key_id)?
            .try_build(&self.key_manager)?;

        self.add_recipient(
            destination,
            output,
            Some(sender_offset_private_key.key_id),
            Some(encryption_key),
        )?;
        Ok(())
    }

    pub fn with_input(&mut self, input: WalletOutput) -> Result<&mut Self, TransactionBuilderError> {
        let nonce = self.key_manager.get_random_key(None, None)?;
        let pair = OutputPair::new(input, nonce.key_id, None, None);
        self.inputs.push(pair);
        Ok(self)
    }

    /// This will allow the receipient to sign the burn commitment
    pub fn with_burn_commitment(&mut self, commitment: Option<CompressedCommitment>) -> &mut Self {
        self.burn_commitment = commitment;
        self
    }

    pub fn with_output(
        &mut self,
        output: WalletOutput,
        sender_offset_key_id: TariKeyId,
        custom_recovery_key_id: Option<TariKeyId>,
    ) -> Result<&mut Self, TransactionBuilderError> {
        let nonce = self.key_manager.get_random_key(None, None)?;
        let pair = OutputPair::new(output, nonce.key_id, Some(sender_offset_key_id), custom_recovery_key_id);
        self.custom_outputs.push(pair);
        Ok(self)
    }

    /// Enable or disable spending of an amount less than the fee
    pub fn with_prevent_fee_gt_amount(&mut self, prevent_fee_gt_amount: bool) -> &mut Self {
        self.prevent_fee_gt_amount = prevent_fee_gt_amount;
        self
    }

    fn get_total_features_and_scripts_size_for_outputs(&self) -> Result<usize, TransactionBuilderError> {
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        let mut size = 0;
        for o in &self.custom_outputs {
            size += fee_weighting.weighting().round_up_features_and_scripts_size(
                o.output
                    .features_and_scripts_byte_size()
                    .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
            );
        }
        for recipient in &self.recipient_outputs {
            size += fee_weighting.weighting().round_up_features_and_scripts_size(
                recipient
                    .output
                    .output
                    .features_and_scripts_byte_size()
                    .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?,
            );
        }
        Ok(size)
    }

    pub fn get_pre_build_change_output(
        &mut self,
    ) -> Result<(MicroMinotari, Option<OutputPair>, TxId), TransactionBuilderError> {
        let (fee, change, tx_id) = self.add_change_if_required(true)?;
        Ok((fee, change, tx_id.unwrap_or_else(TxId::new_random)))
    }

    pub fn get_total_input_value(&self) -> Result<MicroMinotari, TransactionBuilderError> {
        self.inputs
            .iter()
            .map(|i| i.output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })
    }

    pub fn inputs(&self) -> &[OutputPair] {
        &self.inputs
    }

    pub fn recipient_outputs(&self) -> &[RecipientDetails] {
        &self.recipient_outputs
    }

    pub fn custom_outputs(&self) -> &[OutputPair] {
        &self.custom_outputs
    }

    pub fn get_fee_estimate_without_change(&self) -> Result<MicroMinotari, TransactionBuilderError> {
        let num_outputs = self.custom_outputs.len() + self.recipient_outputs.len();
        let num_inputs = self.inputs.len();
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        Ok(match self.fee_per_gram {
            Some(fee_per_gram) => {
                let features_and_scripts_size_without_change =
                    self.get_total_features_and_scripts_size_for_outputs()?;
                fee_weighting.calculate(
                    fee_per_gram,
                    1,
                    num_inputs,
                    num_outputs,
                    features_and_scripts_size_without_change,
                )
            },
            None => self.fee,
        })
    }

    fn check_conditions(&self) -> Result<(), TransactionBuilderError> {
        if self.fee_per_gram.is_none() && self.fee == MicroMinotari::zero() {
            return Err(TransactionBuilderError::FeeNotSet);
        }
        if self.recipient_outputs.is_empty() && self.custom_outputs.is_empty() {
            return Err(TransactionBuilderError::NoRecipients);
        }
        if self.inputs.is_empty() {
            return Err(TransactionBuilderError::NoInputs);
        }
        if self.inputs.len() > MAX_TRANSACTION_INPUTS {
            return Err(TransactionBuilderError::ExceedsMaxInputs(MAX_TRANSACTION_INPUTS));
        }
        if self.recipient_outputs.len() + self.custom_outputs.len() > MAX_TRANSACTION_OUTPUTS {
            return Err(TransactionBuilderError::ExceedsMaxOutputs(MAX_TRANSACTION_OUTPUTS));
        }
        Ok(())
    }

    fn add_change_if_required(
        &mut self,
        calculate_tx_id: bool,
    ) -> Result<(MicroMinotari, Option<OutputPair>, Option<TxId>), TransactionBuilderError> {
        let total_being_spent =
            self.inputs
                .iter()
                .map(|i| i.output.value())
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        let mut total_sent =
            self.custom_outputs
                .iter()
                .map(|o| o.output.value())
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        total_sent += self
            .recipient_outputs
            .iter()
            .map(|o| o.output.output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })?;
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        let fee_without_change = self.get_fee_estimate_without_change()?;
        let temp_script = script!(PushPubKey(Box::default()))?;
        let change_features_and_scripts_size = OutputFeatures::default()
            .get_serialized_size()
            .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))? +
            temp_script
                .get_serialized_size()
                .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string()))?;
        let change_features_and_scripts_size = fee_weighting
            .weighting()
            .round_up_features_and_scripts_size(change_features_and_scripts_size);
        let combined_sent = total_sent
            .checked_add(fee_without_change)
            .ok_or(TransactionBuilderError::TransactionAmountOverflow)?;

        let change_amount = total_being_spent.checked_sub(combined_sent);
        let (fee, change) = match change_amount {
            None => {
                return Err(TransactionBuilderError::SpendingMoreThanAvailable {
                    available: total_being_spent,
                    sent: combined_sent,
                })
            },
            Some(MicroMinotari(0)) => (fee_without_change, None),
            Some(remainder_without_change) => {
                let change_fee = match self.fee_per_gram {
                    Some(fee_per_gram) => {
                        fee_weighting.calculate(fee_per_gram, 0, 0, 1, change_features_and_scripts_size)
                    },
                    None => 0.into(),
                };
                let change_amount = remainder_without_change.checked_sub(change_fee);
                match change_amount {
                    // You can't win. Just add the change to the fee (which is less than the cost of adding another
                    // output and go without a change output
                    None => (fee_without_change + remainder_without_change, None),
                    Some(MicroMinotari(0)) => (fee_without_change + remainder_without_change, None),
                    Some(v) => (fee_without_change + change_fee, self.build_change(v)?),
                }
            },
        };
        if fee > total_sent {
            warn!(
                target: LOG_TARGET,
                "Fee ({}) is greater than amount ({}) being sent for Transaction.",
                fee,
                total_sent,
            );
            if self.prevent_fee_gt_amount {
                return Err(TransactionBuilderError::FeeGreaterThanAmount { fee, sent: total_sent });
            }
        }
        let view_key = self.key_manager.get_view_key().pub_key;
        let tx_id = if calculate_tx_id {
            Some(
                change
                    .as_ref()
                    .map(|c| c.output.calculate_tx_id(view_key.clone().as_bytes()))
                    .or_else(|| {
                        self.recipient_outputs
                            .first()
                            .map(|r| r.output.output.calculate_tx_id(view_key.clone().as_bytes()))
                    })
                    .or_else(|| {
                        self.custom_outputs
                            .first()
                            .map(|c| c.output.calculate_tx_id(view_key.clone().as_bytes()))
                    })
                    .unwrap_or_else(TxId::new_random),
            )
        } else {
            None
        };

        Ok((fee, change, tx_id))
    }

    fn create_change_memo(&self, amount: MicroMinotari) -> Result<MemoField, TransactionBuilderError> {
        let mut memo = MemoField::new_transaction_info(
            TariAddress::default(),
            MicroMinotari::default(),
            amount,
            true,
            self.tx_type,
            Vec::new(),
            self.memo_field
                .as_ref()
                .map(|pay_id| pay_id.payment_id_as_bytes())
                .unwrap_or_default(),
        )
        .map_err(TransactionBuilderError::InvalidMemo)?;

        // we only set for the first output, otherwise the extra data gets too large
        if let Some(recipient) = self.recipient_outputs.first() {
            memo.transaction_info_set_amount(recipient.output.output.value());
            match memo.get_type() {
                TxType::PaymentToOther => memo
                    .transaction_info_set_address(recipient.recipient_address.clone())
                    .map_err(TransactionBuilderError::InvalidMemo)?,
                TxType::PaymentToSelf |
                TxType::CoinSplit |
                TxType::CoinJoin |
                TxType::ValidatorNodeRegistration |
                TxType::CodeTemplateRegistration |
                TxType::ClaimAtomicSwap |
                TxType::HtlcAtomicSwapRefund => memo
                    .transaction_info_set_address(self.own_address.clone())
                    .map_err(TransactionBuilderError::InvalidMemo)?,
                _ => {},
            }
        } else {
            memo.transaction_info_set_amount(amount);
            memo.transaction_info_set_address(self.own_address.clone())
                .map_err(TransactionBuilderError::InvalidMemo)?;
        }
        let mut sent_hashes = Vec::new();
        for recipient in &self.recipient_outputs {
            sent_hashes.push(recipient.output.output.output_hash());
        }
        // if its too much outputs, we dont track this
        if sent_hashes.len() <= 2 {
            memo.transaction_info_set_sent_output_hashes(sent_hashes)
                .map_err(TransactionBuilderError::InvalidMemo)?;
        }
        Ok(memo)
    }

    fn build_change(&mut self, amount: MicroMinotari) -> Result<Option<OutputPair>, TransactionBuilderError> {
        let (change_commitment_mask_key, change_script_key) =
            self.key_manager.get_next_commitment_mask_and_script_key()?;
        let memo = self.create_change_memo(amount)?;
        let sender_offset_public = self.key_manager.get_random_key(None, None)?;
        let script = script!(PushPubKey(Box::new(change_script_key.pub_key.clone())))?;
        let input_data = ExecutionStack::default();

        let encrypted_data = self.key_manager.encrypt_data_for_recovery(
            &change_commitment_mask_key.key_id,
            None,
            amount.as_u64(),
            memo.clone(),
        )?;

        let minimum_value_promise = MicroMinotari::zero();

        let output_version = TransactionOutputVersion::get_current_version();

        let features = OutputFeatures::default();
        let covenant = Covenant::default();
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            output_version,
            &script,
            &features,
            &covenant,
            &encrypted_data,
            &minimum_value_promise,
        );

        let metadata_sig = self.key_manager.get_metadata_signature(
            &change_commitment_mask_key.key_id,
            &amount.into(),
            &sender_offset_public.key_id,
            output_version,
            &metadata_message,
            features.range_proof_type,
        )?;

        let change_wallet_output = WalletOutput::new_current_version(
            amount,
            change_commitment_mask_key.key_id.clone(),
            features,
            script,
            input_data,
            change_script_key.key_id,
            sender_offset_public.pub_key.clone(),
            metadata_sig,
            0,
            covenant,
            encrypted_data,
            minimum_value_promise,
            memo,
            &self.key_manager,
        )?;
        let nonce = self.key_manager.get_random_key(None, None)?;
        Ok(Some(OutputPair::new(
            change_wallet_output,
            nonce.key_id,
            Some(sender_offset_public.key_id),
            None,
        )))
    }

    fn calculate_total_nonce_and_total_public_excess(
        &self,
        change: &Option<OutputPair>,
    ) -> Result<(CompressedPublicKey, CompressedPublicKey), TransactionBuilderError> {
        // lets calculate the total sender kernel signature nonce
        let mut public_nonce = UncompressedPublicKey::default();
        // lets calculate the total sender kernel exess
        let mut public_excess = UncompressedPublicKey::default();
        for input in &self.inputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&input.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess -
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        input.output.commitment_mask_key_id(),
                        &input.kernel_nonce,
                    )?
                    .to_public_key()?;
        }
        for output in &self.custom_outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        output.output.commitment_mask_key_id(),
                        &output.kernel_nonce,
                    )?
                    .to_public_key()?;
        }

        for output in &self.recipient_outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.output.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        output.output.output.commitment_mask_key_id(),
                        &output.output.kernel_nonce,
                    )?
                    .to_public_key()?;
        }

        if let Some(change) = change {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&change.kernel_nonce)?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        change.output.commitment_mask_key_id(),
                        &change.kernel_nonce,
                    )?
                    .to_public_key()?;
        }
        Ok((
            CompressedPublicKey::new_from_pk(public_nonce),
            CompressedPublicKey::new_from_pk(public_excess),
        ))
    }

    // Helper function to change the memo field and encrypted data if the fee has changed due to a change output
    fn update_encrypted_data_and_metadata_sig(
        key_manager: &KM,
        output_pair: &mut OutputPair,
        final_fee: MicroMinotari,
        recipient_address: Option<&TariAddress>,
    ) -> Result<(), TransactionBuilderError> {
        let mut memo_field = output_pair.output.payment_id().clone();
        let mut need_update = false;
        if let Some(existing_fee) = memo_field.get_fee() {
            if existing_fee == final_fee {
                debug!(
                    target: LOG_TARGET,
                    "[Update fee] Fee ({}) was correct for output '{}'",
                    existing_fee, output_pair.output.commitment().to_hex()
                );
            } else {
                debug!(
                    target: LOG_TARGET,
                    "[Update fee] Changing fee changed from {} to {} for output '{}'",
                    existing_fee, final_fee, output_pair.output.commitment().to_hex()
                );
                need_update = true;
            }
        }
        if output_pair.output.metadata_signature() == &ComAndPubSignature::default() {
            debug!(
                target: LOG_TARGET,
                "[Update fee] Metadata signature is a placeholder for output '{}', updating encrypted data",
                output_pair.output.commitment().to_hex()
            );
            need_update = true;
        };
        if need_update {
            memo_field.set_fee(final_fee);
            let encrypted_data = key_manager.encrypt_data_for_recovery(
                output_pair.output.commitment_mask_key_id(),
                output_pair.custom_recovery_key_id.as_ref(),
                output_pair.output.value().as_u64(),
                memo_field.clone(),
            )?;
            // This will change all the necessary fields in the wallet output
            if let Some(recipient) = recipient_address {
                output_pair.output.change_encrypted_data_with_verified_signature(
                    encrypted_data,
                    output_pair
                        .sender_offset_key_id
                        .as_ref()
                        .ok_or(TransactionBuilderError::SenderOffsetKeyIdMissing)?,
                    memo_field,
                    recipient,
                    key_manager,
                )?;
            } else {
                output_pair.output.change_encrypted_data(
                    encrypted_data,
                    output_pair
                        .sender_offset_key_id
                        .as_ref()
                        .ok_or(TransactionBuilderError::SenderOffsetKeyIdMissing)?,
                    memo_field,
                    key_manager,
                )?;
            }
        }

        Ok(())
    }

    /// Build the transaction. This will return an error if the transaction is invalid.
    #[allow(clippy::too_many_lines)]
    pub fn build(mut self) -> Result<FinalizedTransaction, TransactionBuilderError> {
        self.check_conditions()?;

        let (total_fee, mut change_output, _) = self.add_change_if_required(false)?;
        let mut core_tx_builder = CoreTransactionBuilder::new();

        let (total_public_nonce, total_public_excess) =
            self.calculate_total_nonce_and_total_public_excess(&change_output)?;

        let mut script_keys = Vec::new();
        let mut sender_offset_keys = Vec::new();
        let mut offset = PrivateKey::default();
        let mut signature = UncompressedSignature::default();

        let kernel_version = TransactionKernelVersion::get_current_version();
        for input in &self.inputs {
            core_tx_builder.add_input(input.output.to_transaction_input(&self.key_manager)?.clone());
        }
        for output in &self.custom_outputs {
            core_tx_builder.add_output(output.output.to_transaction_output()?);
        }
        let mut sent_outputs = Vec::new();
        for recipient in &mut self.recipient_outputs {
            Self::update_encrypted_data_and_metadata_sig(
                &self.key_manager,
                &mut recipient.output,
                total_fee,
                Some(&recipient.recipient_address),
            )?;

            let output = recipient.output.output.to_transaction_output()?;
            sent_outputs.push(recipient.output.clone());
            if self.tx_type == TxType::Burn {
                // lets do some burn logic
                if output.is_burned() {
                    match self.burn_commitment {
                        Some(_burn_commitment) => {
                            // we can only have a single burn commitment here, so we error here
                            return Err(TransactionBuilderError::MultipleBurnCommitments);
                        },
                        None => {
                            self.burn_commitment = Some(output.commitment.clone());
                        },
                    }
                }
            }
            core_tx_builder.add_output(output);
        }

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            TransactionKernelVersion::get_current_version(),
            total_fee,
            self.lock_height,
            &self.kernel_features,
            &self.burn_commitment,
        );

        for input in &self.inputs {
            signature = &signature +
                &(self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        input.output.commitment_mask_key_id(),
                        &input.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Input,
                    )?
                    .to_schnorr_signature()?);
            offset = offset -
                self.key_manager
                    .get_txo_private_kernel_offset(input.output.commitment_mask_key_id(), &input.kernel_nonce)?;
            script_keys.push(input.output.script_key_id().clone());
        }

        for output in &mut self.custom_outputs {
            Self::update_encrypted_data_and_metadata_sig(&self.key_manager, output, total_fee, None)?;
            signature = &signature +
                self.key_manager
                    .get_partial_txo_kernel_signature(
                        output.output.commitment_mask_key_id(),
                        &output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(output.output.commitment_mask_key_id(), &output.kernel_nonce)?;
            let sender_offset_key_id = output
                .sender_offset_key_id
                .clone()
                .ok_or(TransactionBuilderError::SenderOffsetKeyIdMissing)?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        for output in &self.recipient_outputs {
            signature = &signature +
                self.key_manager
                    .get_partial_txo_kernel_signature(
                        output.output.output.commitment_mask_key_id(),
                        &output.output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )?
                    .to_schnorr_signature()?;
            offset = offset +
                &self.key_manager.get_txo_private_kernel_offset(
                    output.output.output.commitment_mask_key_id(),
                    &output.output.kernel_nonce,
                )?;
            let sender_offset_key_id = output
                .output
                .sender_offset_key_id
                .clone()
                .ok_or(TransactionBuilderError::SenderOffsetKeyIdMissing)?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        if let Some(change) = &mut change_output {
            Self::update_encrypted_data_and_metadata_sig(&self.key_manager, change, total_fee, None)?;
            core_tx_builder.add_output(change.output.to_transaction_output()?);
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        change.output.commitment_mask_key_id(),
                        &change.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(change.output.commitment_mask_key_id(), &change.kernel_nonce)?;
            let sender_offset_key_id = change
                .sender_offset_key_id
                .clone()
                .ok_or(TransactionBuilderError::SenderOffsetKeyIdMissing)?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        let script_offset = self.key_manager.get_script_offset(&script_keys, &sender_offset_keys)?;

        core_tx_builder.add_offset(offset);
        core_tx_builder.add_script_offset(script_offset);
        let excess = CompressedCommitment::from_compressed_key(total_public_excess);

        let kernel = KernelBuilder::new()
            .with_fee(total_fee)
            .with_features(self.kernel_features)
            .with_lock_height(self.lock_height)
            .with_burn_commitment(self.burn_commitment.clone())
            .with_excess(&excess)
            .with_signature(CompressedSignature::new_from_schnorr(signature))
            .build()?;
        core_tx_builder.with_kernel(kernel);
        let tx = core_tx_builder.build()?;

        let view_key = self.key_manager.get_view_key().pub_key;
        let tx_id = if let Some(wallet_output) = &change_output {
            wallet_output.output.calculate_tx_id(view_key.as_bytes())
        } else {
            tx_outputs_to_tx_id(view_key.as_bytes(), tx.body.outputs())
        };

        let destination_addresses = self
            .recipient_outputs
            .iter()
            .map(|r| r.recipient_address.clone())
            .collect::<Vec<TariAddress>>();

        let mut amount = self
            .recipient_outputs
            .iter()
            .map(|r| r.output.output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })?;
        amount += self
            .custom_outputs
            .iter()
            .map(|o| o.output.value())
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })?;
        let mut sent_hashes = Vec::new();
        for recipient in &self.recipient_outputs {
            sent_hashes.push(recipient.output.output.output_hash());
        }
        let mut received_hashes = Vec::new();
        for output in &self.custom_outputs {
            received_hashes.push(output.output.output_hash());
        }
        let change_output_hash = match &change_output {
            Some(o) => vec![o.output.output_hash()],
            None => vec![],
        };

        let payment_id = if let Some(mut memo_field) = self.memo_field {
            if let Some(fee) = memo_field.get_fee() {
                if fee == total_fee {
                    debug!(target: LOG_TARGET, "[Update fee] Fee ({}) was correct for entire transaction", total_fee);
                } else {
                    debug!(target: LOG_TARGET,
                        "[Update fee] Fee changed from {} to {} for entire transaction",
                        fee, total_fee
                    );
                }

                memo_field.set_fee(total_fee);
                memo_field
            } else {
                memo_field
            }
        } else {
            let mut memo = MemoField::default();
            memo.set_fee(total_fee);
            memo
        };

        Ok(FinalizedTransaction {
            tx_id,
            source_address: self.own_address,
            destination_addresses,
            amount,
            fee: total_fee,
            transaction: tx,
            payment_id,
            change: change_output.map(|o| o.output),
            sent_outputs,
            // Hashes of outputs being sent to others (excluding change)
            sent_output_hashes: sent_hashes,
            // Hashes of outputs received from others (excluding change)
            received_output_hashes: received_hashes,
            // Hashes of change outputs (for reference)
            change_output_hashes: change_output_hash,
        })
    }
}
// For some reason clippy picks up the debug impl as not used, and key_manager is a trait without debug, so we need to
// manually implement Debug for TransactionBuilder
#[allow(dead_code)]
impl<KM> Debug for TransactionBuilder<KM> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        #[derive(Debug)]
        pub struct TransactionBuilder<'a> {
            consensus_constants: &'a ConsensusConstants,
            fee_per_gram: &'a Option<MicroMinotari>,
            fee: &'a MicroMinotari,
            recipient_outputs: &'a Vec<RecipientDetails>,
            inputs: &'a Vec<OutputPair>,
            custom_outputs: &'a Vec<OutputPair>,
            prevent_fee_gt_amount: &'a bool,
            tx_type: &'a TxType,
            memo_field: &'a Option<MemoField>,
            lock_height: &'a u64,
            kernel_features: &'a KernelFeatures,
            burn_commitment: &'a Option<CompressedCommitment>,
            own_address: &'a TariAddress,
        }

        let Self {
            consensus_constants,
            key_manager: _,
            fee_per_gram,
            fee,
            recipient_outputs,
            inputs,
            custom_outputs,
            prevent_fee_gt_amount,
            tx_type,
            memo_field,
            lock_height,
            kernel_features,
            burn_commitment,
            own_address,
        } = self;

        fmt::Debug::fmt(
            &TransactionBuilder {
                consensus_constants,
                fee_per_gram,
                fee,
                recipient_outputs,
                inputs,
                custom_outputs,
                prevent_fee_gt_amount,
                tx_type,
                memo_field,
                lock_height,
                kernel_features,
                burn_commitment,
                own_address,
            },
            f,
        )
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::seeds::{cipher_seed::CipherSeed, mnemonic::Mnemonic, seed_words::SeedWords};
    use tari_script::Opcode;
    use tari_utilities::Hidden;

    use crate::{
        key_manager::SecretTransactionKeyManagerInterface,
        transaction_components::{
            one_sided::{public_key_to_output_encryption_key, public_key_to_output_spending_key},
            EncryptedData,
        },
    };
    fn create_view_key_manager(view_wallet: ViewWallet) -> Result<KeyManager, KeyManagerError> {
        let wallet = WalletType::ViewWallet(view_wallet);
        KeyManager::new(wallet)
    }
    use chacha20poly1305::aead::OsRng;
    use tari_crypto::keys::SecretKey;
    use tari_script::{script, TariScript};

    use super::*;
    use crate::{
        crypto_factories::CryptoFactories,
        key_manager::{
            error::KeyManagerError,
            wallet_types::{SeedWordsWallet, ViewWallet, WalletType},
            KeyManager,
        },
        tari_amount::{uT, MicroMinotari},
        test_helpers::{
            create_consensus_constants,
            create_consensus_manager,
            create_test_input,
            create_wallet_output_with_data,
            TestParams,
            UtxoTestParams,
        },
        transaction_builder::TransactionBuilder,
        transaction_components::{MemoField, OutputFeatures, WalletOutputBuilder},
        validation::transaction::TransactionInternalConsistencyValidator,
    };

    /// Hit the edge case where our change isn't enough to cover the cost of an extra output
    #[test]
    #[allow(clippy::identity_op)]
    fn change_edge_case() {
        // Create some inputs
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let constants = create_consensus_constants(0);
        let weighting = constants.transaction_weight_params();
        let tx_fee = Fee::new(*weighting).calculate(1.into(), 1, 1, 1, 0);
        let fee_for_change_output = weighting.params().output_weight * uT;
        // fee == 340, output = 80
        // outputs weight: 1060, kernel weight: 10, input weight: 9, output weight: 53,

        // Pay out so that I should get change, but not enough to pay for the output
        let input = create_test_input(
            // one under the amount required to pay the fee for a change output
            2000 * uT + tx_fee + fee_for_change_output - 1 * uT,
            0,
            &key_manager,
            vec![],
            None,
        );
        let output = p
            .create_output(
                UtxoTestParams {
                    value: 2000 * uT,
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap();
        // Start the builder
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_output(output, p.sender_offset_key_id, None)
            .unwrap()
            .with_input(input)
            .unwrap()
            .with_fee_per_gram(MicroMinotari(1))
            .with_prevent_fee_gt_amount(false);
        let result = builder.build().unwrap();
        assert_eq!(
            result.transaction.body.kernels().first().unwrap().lock_height,
            0,
            "Lock height"
        );
        assert_eq!(result.fee, tx_fee + fee_for_change_output - 1 * uT, "Fee");
        assert_eq!(
            result.transaction.body.kernels().first().unwrap().fee,
            tx_fee + fee_for_change_output - 1 * uT,
            "Fee"
        );
        assert_eq!(result.transaction.body.outputs().len(), 1, "There should be 1 output");
        assert_eq!(result.transaction.body.inputs().len(), 1, "There should be 1 input");
    }

    #[test]
    fn too_many_inputs() {
        // Create some inputs
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);

        let output = create_wallet_output_with_data(
            script!(Nop).unwrap(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(500),
            &key_manager,
        )
        .unwrap();
        let constants = create_consensus_constants(0);
        // Start the builder
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_output(output, p.sender_offset_key_id, None)
            .unwrap()
            .with_fee_per_gram(MicroMinotari(2));
        let input_base = create_test_input(MicroMinotari(50), 0, &key_manager, vec![], None);
        for _ in 0..=MAX_TRANSACTION_INPUTS {
            builder.with_input(input_base.clone()).unwrap();
        }
        let _err = builder.build().unwrap_err();
        // this needs a refactor to get in, we cannot enable partialeq TransactionBuilderError
        // assert_eq!(err, TransactionBuilderError::ExceedsMaxInputs(MAX_TRANSACTION_INPUTS));
    }

    #[test]
    fn not_enough_funds() {
        // Create some inputs
        let key_manager = KeyManager::new_random().unwrap();
        let p = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(400), 0, &key_manager, vec![], None);
        let script = script!(Nop).unwrap();
        let output = create_wallet_output_with_data(
            script.clone(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(400),
            &key_manager,
        )
        .unwrap();
        // Start the builder
        let constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_input(input)
            .unwrap()
            .with_output(output, p.sender_offset_key_id.clone(), None)
            .unwrap()
            .with_fee_per_gram(MicroMinotari(1));
        let _err = builder.build().unwrap_err();
    }

    #[test]
    fn zero_recipient_outputs() {
        let key_manager = KeyManager::new_random().unwrap();
        let p1 = TestParams::new(&key_manager);
        let p2 = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None);
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        let script = TariScript::default();
        let output_features = OutputFeatures::default();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(2))
            .with_input(input)
            .unwrap()
            .with_output(
                create_wallet_output_with_data(
                    script.clone(),
                    output_features.clone(),
                    &p1,
                    MicroMinotari(500),
                    &key_manager,
                )
                .unwrap(),
                p1.sender_offset_key_id.clone(),
                None,
            )
            .unwrap()
            .with_output(
                create_wallet_output_with_data(script, output_features, &p2, MicroMinotari(400), &key_manager).unwrap(),
                p2.sender_offset_key_id.clone(),
                None,
            )
            .unwrap();
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        let rules = create_consensus_manager();
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    fn single_recipient_no_change() {
        let rules = create_consensus_manager();
        let factories = CryptoFactories::default();
        let key_manager = KeyManager::new_random().unwrap();
        let bob_key = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None);
        let utxo = input.to_transaction_input(&key_manager).unwrap();
        let script = script!(Nop).unwrap();
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        let fee_per_gram = MicroMinotari(4);
        let fee = Fee::new(*consensus_constants.transaction_weight_params()).calculate(fee_per_gram, 1, 1, 1, 0);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        let bob_sender_offset = key_manager.get_random_key(None, None).unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(
            MicroMinotari(1200) - fee - MicroMinotari(10),
            bob_key.commitment_mask_key_id,
        )
        .with_features(OutputFeatures::default())
        .with_script(script.clone())
        .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
        .unwrap()
        .with_input_data(Default::default())
        .with_sender_offset_public_key(bob_public_key)
        .with_script_key(bob_key.script_key_id)
        .with_minimum_value_promise(0.into())
        .sign_metadata_signature_user_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
        .unwrap()
        .try_build(&key_manager)
        .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id), None)
            .unwrap();

        let finalized = builder.build().unwrap();

        let tx = finalized.transaction;
        assert_eq!(tx.body.kernels().first().unwrap().fee, fee + MicroMinotari(10)); // Check the twist above
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.inputs().first().unwrap().commitment(), utxo.commitment());
        assert_eq!(tx.body.outputs().len(), 1);
        assert!(tx.body.outputs().first().unwrap().verify_metadata_signature().is_ok());
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn single_recipient_with_change() {
        let rules = create_consensus_manager();
        let key_manager = KeyManager::new_random().unwrap();
        let factories = CryptoFactories::default();
        // Alice's parameters
        let alice_key = TestParams::new(&key_manager);
        // Bob's parameters
        let bob_key = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(25000), 0, &key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        let script = script!(PushPubKey(Box::default())).unwrap();
        let expected_fee = Fee::new(*consensus_constants.transaction_weight_params()).calculate(
            MicroMinotari(20),
            1,
            1,
            2,
            alice_key
                .get_size_for_default_features_and_scripts(2)
                .expect("Failed to get size for default features and scripts"),
        );
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap();
        let bob_sender_offset = key_manager.get_random_key(None, None).unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();

        let bob_output = WalletOutputBuilder::new(MicroMinotari(5000), bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_metadata_signature_user_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .unwrap()
            .try_build(&key_manager)
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id), None)
            .unwrap();
        // Transaction should be complete
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        assert_eq!(tx.body.kernels().first().unwrap().fee, expected_fee);
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    fn single_recipient_multiple_inputs_with_change() {
        let rules = create_consensus_manager();
        let key_manager = KeyManager::new_random().unwrap();
        let factories = CryptoFactories::default();
        // Bob's parameters
        let bob_key = TestParams::new(&key_manager);
        let input = create_test_input(MicroMinotari(10000), 0, &key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        let script = script!(Nop).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();
        let bob_sender_offset = key_manager.get_random_key(None, None).unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(MicroMinotari(5000), bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_metadata_signature_user_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .unwrap()
            .try_build(&key_manager)
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id), None)
            .unwrap();
        let finalized = builder.build().unwrap();

        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 3);
        assert_eq!(tx.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    fn add_stealth_recipient() {
        let rules = create_consensus_manager();
        let key_manager = KeyManager::new_random().unwrap();
        let factories = CryptoFactories::default();
        let input = create_test_input(MicroMinotari(10000), 0, &key_manager, vec![], None);
        let input2 = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![], None);
        let input3 = create_test_input(MicroMinotari(15000), 0, &key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder =
            TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .unwrap()
            .with_input(input2)
            .unwrap()
            .with_input(input3)
            .unwrap();
        let bob_address = TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        )
        .unwrap();

        builder
            .add_stealth_recipient(
                bob_address.clone(),
                MicroMinotari(5000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        let bob_output = builder.recipient_outputs.last().unwrap().output.output.clone();
        let bob_sender_offset = builder
            .recipient_outputs
            .last()
            .unwrap()
            .output
            .sender_offset_key_id
            .clone()
            .unwrap();
        let shared_secret = key_manager
            .get_diffie_hellman_shared_secret(&bob_sender_offset, bob_address.public_view_key().unwrap())
            .unwrap();
        let commitment_mask_private_key = public_key_to_output_spending_key(&shared_secret).unwrap();
        let commitment_mask_pvt = key_manager
            .get_private_key(bob_output.commitment_mask_key_id())
            .unwrap();
        assert_eq!(commitment_mask_private_key, commitment_mask_pvt);

        let script_spending_key = key_manager
            .stealth_address_script_spending_key(bob_output.commitment_mask_key_id(), bob_address.public_spend_key())
            .unwrap();
        let script = push_pubkey_script(&script_spending_key);

        assert_eq!(*bob_output.script(), script);

        let encryption_private_key = public_key_to_output_encryption_key(&shared_secret).unwrap();
        let bob_tx_output = bob_output.to_transaction_output().unwrap();
        assert!(key_manager
            .is_this_output_ours(
                bob_tx_output.commitment(),
                bob_output.encrypted_data(),
                Some(encryption_private_key)
            )
            .unwrap());

        let finalized = builder.build().unwrap();

        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 3);
        assert_eq!(tx.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    fn disallow_fee_larger_than_amount() {
        // Alice's parameters
        let key_manager = KeyManager::new_random().unwrap();
        let (utxo_amount, fee_per_gram, amount) = (MicroMinotari(2500), MicroMinotari(10), MicroMinotari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager, vec![], None);
        let script = script!(Nop).unwrap();
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();

        let bob_key = TestParams::new(&key_manager);
        let bob_sender_offset = key_manager.get_random_key(None, None).unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(amount, bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_metadata_signature_user_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .unwrap()
            .try_build(&key_manager)
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id), None)
            .unwrap();
        let _err = builder.build().unwrap_err();

        // this needs a refactor to get in, we cannot enable partialeq TransactionBuilderError
        // assert_eq!(err, TransactionBuilderError::FeeGreaterThanAmount);
    }

    #[test]
    fn allow_fee_larger_than_amount() {
        // Alice's parameters
        let key_manager = KeyManager::new_random().unwrap();
        let (utxo_amount, fee_per_gram, amount) = (MicroMinotari(2500), MicroMinotari(10), MicroMinotari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager, vec![], None);
        let script = script!(Nop).unwrap();
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap()
            .with_prevent_fee_gt_amount(false);

        let bob_key = TestParams::new(&key_manager);
        let bob_sender_offset = key_manager.get_random_key(None, None).unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(amount, bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_metadata_signature_user_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .unwrap()
            .try_build(&key_manager)
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id), None)
            .unwrap();
        // Test if the transaction passes the initial 'fee greater than amount' check when it is constructed
        match builder.build() {
            Ok(_) => {},
            Err(e) => panic!("Unexpected error: {e:?}"),
        };
    }

    #[test]
    fn create_multi_recipients_transaction() {
        let rules = create_consensus_manager();
        let factories = CryptoFactories::default();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let carol_key_manager = KeyManager::new_random().unwrap();

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = carol_key_manager.get_spend_key().pub_key;
        let view_key = carol_key_manager.get_view_key().pub_key;
        let carol_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let input = create_test_input(MicroMinotari(5000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                bob_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder
            .add_stealth_recipient(
                carol_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.outputs().len(), 3);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn recover_multi_recipients_transaction() {
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let alice_view_key_manager = create_view_key_manager(alice_keys).unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let bob_keys = ViewWallet::new(
            bob_key_manager.get_spend_key().pub_key,
            bob_key_manager.get_private_view_key(),
            None,
        );
        let bob_view_key_manager = create_view_key_manager(bob_keys).unwrap();
        let carol_key_manager = KeyManager::new_random().unwrap();
        let carol_keys = ViewWallet::new(
            carol_key_manager.get_spend_key().pub_key,
            carol_key_manager.get_private_view_key(),
            None,
        );
        let carol_view_key_manager = create_view_key_manager(carol_keys).unwrap();

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let spend_key = carol_key_manager.get_spend_key().pub_key;
        let view_key = carol_key_manager.get_view_key().pub_key;
        let carol_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let input = create_test_input(MicroMinotari(5000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                bob_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        builder
            .add_stealth_recipient(
                carol_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .unwrap();
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        let mut alice_count = 0;
        let mut bob_count = 0;
        let mut carol_count = 0;
        let mut wrong = 0;
        for output in tx.body.outputs() {
            // alice change output
            if alice_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                alice_count += 1;
            }
            // let assume a stealth key for alice
            let alice_shared_secret = alice_key_manager
                .get_diffie_hellman_shared_secret(
                    &alice_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let alice_encryption_private_key = public_key_to_output_encryption_key(&alice_shared_secret).unwrap();
            if alice_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(alice_encryption_private_key),
                )
                .unwrap()
            {
                wrong += 1;
            }

            // bob change output
            if bob_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let bob_shared_secret = bob_key_manager
                .get_diffie_hellman_shared_secret(
                    &bob_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let bob_encryption_private_key = public_key_to_output_encryption_key(&bob_shared_secret).unwrap();
            if bob_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(bob_encryption_private_key),
                )
                .unwrap()
            {
                bob_count += 1;
            }

            // carol change output
            if carol_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let carol_shared_secret = carol_key_manager
                .get_diffie_hellman_shared_secret(
                    &carol_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let carol_encryption_private_key = public_key_to_output_encryption_key(&carol_shared_secret).unwrap();
            if carol_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(carol_encryption_private_key),
                )
                .unwrap()
            {
                carol_count += 1;
            }
        }
        assert_eq!(alice_count, 1); // alice change output
        assert_eq!(bob_count, 1); // bob recipient output
        assert_eq!(carol_count, 1); // carol recipient output
        assert_eq!(wrong, 0);

        // lets do view only
        let mut alice_count = 0;
        let mut bob_count = 0;
        let mut carol_count = 0;
        let mut wrong = 0;
        for output in tx.body.outputs() {
            // alice change output
            if alice_view_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                alice_count += 1;
            }
            // let assume a stealth key for alice
            let alice_shared_secret = alice_view_key_manager
                .get_diffie_hellman_shared_secret(
                    &alice_view_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let alice_encryption_private_key = public_key_to_output_encryption_key(&alice_shared_secret).unwrap();
            if alice_view_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(alice_encryption_private_key),
                )
                .unwrap()
            {
                wrong += 1;
            }

            // bob change output
            if bob_view_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let bob_shared_secret = bob_view_key_manager
                .get_diffie_hellman_shared_secret(
                    &bob_view_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let bob_encryption_private_key = public_key_to_output_encryption_key(&bob_shared_secret).unwrap();
            if bob_view_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(bob_encryption_private_key),
                )
                .unwrap()
            {
                bob_count += 1;
            }

            // carol change output
            if carol_view_key_manager
                .is_this_output_ours(&output.commitment, &output.encrypted_data, None)
                .unwrap()
            {
                wrong += 1;
            }
            // let assume a stealth key for bob
            let carol_shared_secret = carol_view_key_manager
                .get_diffie_hellman_shared_secret(
                    &carol_view_key_manager.get_view_key().key_id,
                    &output.sender_offset_public_key,
                )
                .unwrap();
            let carol_encryption_private_key = public_key_to_output_encryption_key(&carol_shared_secret).unwrap();
            if carol_view_key_manager
                .is_this_output_ours(
                    &output.commitment,
                    &output.encrypted_data,
                    Some(carol_encryption_private_key),
                )
                .unwrap()
            {
                carol_count += 1;
            }
        }
        assert_eq!(alice_count, 1); // alice change output
        assert_eq!(bob_count, 1); // bob recipient output
        assert_eq!(carol_count, 1); // carol recipient output
        assert_eq!(wrong, 0);
    }

    #[test]
    fn create_very_large_multi_recipients_transaction() {
        let rules = create_consensus_manager();
        let factories = CryptoFactories::default();
        let alice_key_manager = KeyManager::new_random().unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let input = create_test_input(MicroMinotari(500000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        for _ in 0..100 {
            builder
                .add_stealth_recipient(
                    bob_address.clone(),
                    MicroMinotari(1000),
                    OutputFeatures::default(),
                    MemoField::new_empty(),
                )
                .unwrap();
        }

        let finalized = builder.build().unwrap();
        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.outputs().len(), 101);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    /// this test will test recovery of a pregenerated transaction alice sent bob, they both need to recover one output
    /// each
    #[test]
    fn recover_historic_transaction_data() {
        let alice_wallet_seeds = "leopard tilt extend file rescue purity day blind office laptop task today stairs \
                                  now stairs conduct fruit pigeon make urban grace gasp suit drill"
            .to_string();
        let alice_seeds = SeedWords::new(
            alice_wallet_seeds
                .split(' ')
                .map(|s| Hidden::hide(s.to_string()))
                .collect::<Vec<Hidden<String>>>(),
        );

        let alice_cipher = CipherSeed::from_mnemonic(&alice_seeds, None).unwrap();
        let alice_wallet = WalletType::SeedWords(SeedWordsWallet::construct_new(alice_cipher).unwrap());
        let alice_key_manager = KeyManager::new(alice_wallet).unwrap();
        let bob_wallet_seeds = "park bright young fitness twin globe fresh dose gesture inmate already word minimum \
                                waste shop chair chef quick stairs subway paper brave case vessel"
            .to_string();
        let seeds = SeedWords::new(
            bob_wallet_seeds
                .split(' ')
                .map(|s| Hidden::hide(s.to_string()))
                .collect::<Vec<Hidden<String>>>(),
        );

        let cipher = CipherSeed::from_mnemonic(&seeds, None).unwrap();

        let bob_wallet = WalletType::SeedWords(SeedWordsWallet::construct_new(cipher).unwrap());
        let bob_key_manager = KeyManager::new(bob_wallet).unwrap();

        struct RecoverData {
            pub commitment: CompressedCommitment,
            pub encrypted_data: EncryptedData,
            pub sender_offset_public_key: CompressedPublicKey,
            pub script: TariScript,
        }
        let outputs = [
            RecoverData{
                commitment: CompressedCommitment::from_hex("006399307893ae875ac7677b564ba068a9bc18eb903f5245a39a78aeebecc87b").unwrap(),
                encrypted_data: EncryptedData::from_hex("0b52d2d3fc3ee4b1d660effbb925e14adbecbb411ef6a15263977c98a60e28cf0391f7fa462d25550b9dde46f1a918135de15bc2416e07bcc2979daf2913c489c1d7392c893abcda8702b855980beed01b90161b8c6be81e631a5b137c04822edbefc7e2ad176ea6c0a55f12d70878b82575adc3d0cee368fe752d76e2916ec142eecf22bd4d73f4c5c8898ffc62860f06ed89346dec6c8d7e0b464c25e517b7c39d576c4d49eacfd063f09fa576a2e9ba4e73887f7f31a4a1bdcf5f872ffdfade6bddd844bbcedea9ac818112ab7a2266d2").unwrap(),
                sender_offset_public_key: CompressedPublicKey::from_hex("cefb54097d450a875959dd50d6c6e2e17d0f2d285ac58005bcda7ff1e805c048").unwrap(),
                script: TariScript::from_hex("7e7cf0a1cefaca6355741d47564a8162b57a735f8c4fd1d3afde0bc9ac44df6f09").unwrap(),
            },
            RecoverData{
                commitment: CompressedCommitment::from_hex("e6b0f7ca79ae1d12fb44ca678f96296b25fefd6da25cde01d4f566f963c9d96e").unwrap(),
                encrypted_data: EncryptedData::from_hex("5355a9fa27de31511619e22810d41a73b9d0eed8dcfaa1cdd129b7dc4afa9905081e3c631f43b1946d5aef12413fe5f2e64c739d926d8d72883daa2577771776a45e5e6b1da727e0130f6b15c1fcf3ec").unwrap(),
                sender_offset_public_key: CompressedPublicKey::from_hex("b6de6ce6ca11e5fcbfddadc6277a97ce480ab0ca9ba09714e3cca1a7e8ebe34d").unwrap(),
                script: TariScript::from_hex("7e722481cc06bb13b6732edd6d19bdea213549f0b51df201a2dc8f04f9ddf7a039").unwrap(),
            },
        ];

        let mut alice_count = 0;
        let mut bob_count = 0;
        for output in outputs {
            // alice change output
            if let Ok(Some((commitment_key, _, _))) = alice_key_manager.try_output_key_recovery(
                &output.commitment,
                &output.encrypted_data,
                &output.sender_offset_public_key,
            ) {
                alice_count += 1;
                let script_spending_key = alice_key_manager
                    .stealth_address_script_spending_key(&commitment_key, &alice_key_manager.get_spend_key().pub_key)
                    .unwrap();
                if let [Opcode::PushPubKey(scanned_pk)] = output.script.as_slice() {
                    if script_spending_key != **scanned_pk {
                        panic!("should have found this");
                    }
                } else {
                    panic!("unexpected script");
                }
            }
            if let Ok(Some((commitment_key, _, _))) = bob_key_manager.try_output_key_recovery(
                &output.commitment,
                &output.encrypted_data,
                &output.sender_offset_public_key,
            ) {
                bob_count += 1;
                let script_spending_key = bob_key_manager
                    .stealth_address_script_spending_key(&commitment_key, &bob_key_manager.get_spend_key().pub_key)
                    .unwrap();
                if let [Opcode::PushPubKey(scanned_pk)] = output.script.as_slice() {
                    if script_spending_key != **scanned_pk {
                        panic!("should have found this");
                    }
                } else {
                    panic!("unexpected script");
                }
            }
        }
        assert_eq!(alice_count, 1); // alice change output
        assert_eq!(bob_count, 1); // bob recipient output
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn transaction_details_correct() {
        let alice_key_manager = KeyManager::new_random().unwrap();
        let alice_keys = ViewWallet::new(
            alice_key_manager.get_spend_key().pub_key,
            alice_key_manager.get_private_view_key(),
            None,
        );
        let spend_key = alice_key_manager.get_spend_key().pub_key;
        let view_key = alice_key_manager.get_view_key().pub_key;
        let alice_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        let bob_key_manager = KeyManager::new_random().unwrap();
        let bob_keys = ViewWallet::new(
            bob_key_manager.get_spend_key().pub_key,
            bob_key_manager.get_private_view_key(),
            None,
        );

        let spend_key = bob_key_manager.get_spend_key().pub_key;
        let view_key = bob_key_manager.get_view_key().pub_key;
        let bob_address = TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

        let input = create_test_input(MicroMinotari(5000), 0, &alice_key_manager, vec![], None);
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(
            consensus_constants.clone(),
            alice_key_manager.clone(),
            Network::LocalNet,
        )
        .unwrap();
        let fee_per_gram = MicroMinotari(4);
        let payment_id =
            MemoField::new_address_and_data(alice_address, 1.into(), true, TxType::PaymentToOther, vec![]).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .unwrap();
        builder
            .add_stealth_recipient(
                bob_address,
                MicroMinotari(1000),
                OutputFeatures::default(),
                payment_id.clone(),
            )
            .unwrap();
        builder.with_memo(payment_id);
        let finalized = builder.build().unwrap();
        let tx = finalized.transaction.clone();
        let mut alice_memo = None;
        let mut bob_memo = None;
        for output in tx.body.outputs() {
            // alice output
            if let Some(output) = alice_key_manager
                .try_output_key_recovery(
                    &output.commitment,
                    &output.encrypted_data,
                    &output.sender_offset_public_key,
                )
                .unwrap()
            {
                alice_memo = Some(output.2);
            }

            // bob output
            if let Some(output) = bob_key_manager
                .try_output_key_recovery(
                    &output.commitment,
                    &output.encrypted_data,
                    &output.sender_offset_public_key,
                )
                .unwrap()
            {
                bob_memo = Some(output.2);
            }
        }
        let alice_memo = alice_memo.unwrap();
        let bob_memo = bob_memo.unwrap();
        assert_eq!(alice_memo.get_tx_type(), Some(TxType::PaymentToOther));
        assert_eq!(bob_memo.get_tx_type(), Some(TxType::PaymentToOther));
        assert_eq!(alice_memo.get_fee().unwrap(), tx.body.get_total_fee().unwrap());
        assert_eq!(bob_memo.get_fee().unwrap(), tx.body.get_total_fee().unwrap());
        assert_eq!(tx.body.get_total_fee().unwrap(), finalized.fee);
        assert_eq!(
            tx.body.get_total_fee().unwrap(),
            finalized.payment_id.get_fee().unwrap()
        );
    }
}
