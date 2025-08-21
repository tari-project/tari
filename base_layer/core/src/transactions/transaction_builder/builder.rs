// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, fmt::Debug};

use log::*;
use tari_common::configuration::Network;
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::{TariAddress, TariAddressFeatures},
    types::{
        CompressedCommitment,
        CompressedPublicKey,
        PrivateKey,
        Signature,
        UncompressedPublicKey,
        UncompressedSignature,
    },
};
use tari_script::{script, ExecutionStack};

use crate::{
    borsh::SerializedSize,
    consensus::ConsensusConstants,
    covenants::Covenant,
    transactions::{
        fee::Fee,
        tari_amount::MicroMinotari,
        transaction_builder::{
            error::TransactionBuilderError,
            models::{FinalizedTransaction, OutputPair, RecipientDetails},
        },
        transaction_components::{
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
            MAX_TRANSACTION_INPUTS,
            MAX_TRANSACTION_OUTPUTS,
        },
        transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface, TxoStage},
    },
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
    pub async fn new(
        consensus_constants: ConsensusConstants,
        key_manager: KM,
        network: Network,
    ) -> Result<Self, TransactionBuilderError> {
        let view_key = key_manager.get_view_key().await?;
        let spend_key = key_manager.get_spend_key().await?;
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
    pub async fn add_recipient(
        &mut self,
        recipient_address: TariAddress,
        recipient_output: WalletOutput,
        sender_offset_key_id: Option<TariKeyId>,
    ) -> Result<&mut Self, TransactionBuilderError> {
        let kernel_nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let recipient_output = OutputPair::new(recipient_output, kernel_nonce.key_id, sender_offset_key_id);
        let recipient_details = RecipientDetails {
            output: recipient_output,
            recipient_address,
        };
        self.recipient_outputs.push(recipient_details);
        Ok(self)
    }

    pub async fn with_input(&mut self, input: WalletOutput) -> Result<&mut Self, TransactionBuilderError> {
        let nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let pair = OutputPair::new(input, nonce.key_id, None);
        self.inputs.push(pair);
        Ok(self)
    }

    /// This will allow the receipient to sign the burn commitment
    pub fn with_burn_commitment(&mut self, commitment: Option<CompressedCommitment>) -> &mut Self {
        self.burn_commitment = commitment;
        self
    }

    pub async fn with_output(
        &mut self,
        output: WalletOutput,
        sender_offset_key_id: TariKeyId,
    ) -> Result<&mut Self, TransactionBuilderError> {
        let nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let pair = OutputPair::new(output, nonce.key_id, Some(sender_offset_key_id));
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

    pub async fn get_pre_build_change_output(&self) -> Result<Option<OutputPair>, TransactionBuilderError> {
        Ok(self.add_change_if_required().await?.1)
    }

    pub fn get_total_input_value(&self) -> Result<MicroMinotari, TransactionBuilderError> {
        self.inputs
            .iter()
            .map(|i| i.output.value)
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

    pub fn get_fee_estimate(&self) -> Result<MicroMinotari, TransactionBuilderError> {
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

    async fn add_change_if_required(&self) -> Result<(MicroMinotari, Option<OutputPair>), TransactionBuilderError> {
        let total_being_spent =
            self.inputs
                .iter()
                .map(|i| i.output.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        let mut total_sent =
            self.custom_outputs
                .iter()
                .map(|o| o.output.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        total_sent += self.recipient_outputs.iter().map(|o| o.output.output.value).try_fold(
            MicroMinotari::zero(),
            |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            },
        )?;
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        let fee_without_change = self.get_fee_estimate()?;
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
                    Some(v) => (fee_without_change + change_fee, self.build_change(v).await?),
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
        Ok((fee, change))
    }

    async fn create_change_memo(&self, amount: MicroMinotari) -> Result<MemoField, TransactionBuilderError> {
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
            memo.transaction_info_set_amount(recipient.output.output.value);
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
            sent_hashes.push(recipient.output.tx_output(&self.key_manager).await?.hash());
        }
        memo.transaction_info_set_sent_output_hashes(sent_hashes)
            .map_err(TransactionBuilderError::InvalidMemo)?;
        Ok(memo)
    }

    async fn build_change(&self, amount: MicroMinotari) -> Result<Option<OutputPair>, TransactionBuilderError> {
        let (change_commitment_mask_key, change_script_key) =
            self.key_manager.get_next_commitment_mask_and_script_key().await?;
        let memo = self.create_change_memo(amount).await?;
        let sender_offset_public = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await?;
        let script = script!(PushPubKey(Box::new(change_script_key.pub_key.clone())))?;
        let input_data = ExecutionStack::default();

        let encrypted_data = self
            .key_manager
            .encrypt_data_for_recovery(&change_commitment_mask_key.key_id, None, amount.as_u64(), memo.clone())
            .await?;

        let minimum_value_promise = MicroMinotari::zero();

        let output_version = TransactionOutputVersion::get_current_version();

        let features = OutputFeatures::default();
        let covenant = Covenant::default();
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &output_version,
            &script,
            &features,
            &covenant,
            &encrypted_data,
            &minimum_value_promise,
        );

        let metadata_sig = self
            .key_manager
            .get_metadata_signature(
                &change_commitment_mask_key.key_id,
                &amount.into(),
                &sender_offset_public.key_id,
                &output_version,
                &metadata_message,
                features.range_proof_type,
            )
            .await?;

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
        )
        .await?;
        let nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        Ok(Some(OutputPair::new(
            change_wallet_output,
            nonce.key_id,
            Some(sender_offset_public.key_id),
        )))
    }

    async fn calculate_total_nonce_and_total_public_excess(
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
                    .get_public_key_at_key_id(&input.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess -
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await?
                    .to_public_key()?;
        }
        for output in &self.custom_outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await?
                    .to_public_key()?;
        }

        for output in &self.recipient_outputs {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.output.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(
                        &output.output.output.spending_key_id,
                        &output.output.kernel_nonce,
                    )
                    .await?
                    .to_public_key()?;
        }

        if let Some(change) = change {
            public_nonce = public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&change.kernel_nonce)
                    .await?
                    .to_public_key()?;
            public_excess = public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await?
                    .to_public_key()?;
        }
        Ok((
            CompressedPublicKey::new_from_pk(public_nonce),
            CompressedPublicKey::new_from_pk(public_excess),
        ))
    }

    /// Build the transaction. This will return an error if the transaction is invalid.
    #[allow(clippy::too_many_lines)]
    pub async fn build(mut self) -> Result<FinalizedTransaction, TransactionBuilderError> {
        self.check_conditions()?;

        let (total_fee, change_output) = self.add_change_if_required().await?;
        let mut core_tx_builder = CoreTransactionBuilder::new();

        let (total_public_nonce, total_public_excess) = self
            .calculate_total_nonce_and_total_public_excess(&change_output)
            .await?;

        let mut script_keys = Vec::new();
        let mut sender_offset_keys = Vec::new();
        let mut offset = PrivateKey::default();
        let mut signature = UncompressedSignature::default();

        let kernel_version = TransactionKernelVersion::get_current_version();
        for input in &self.inputs {
            core_tx_builder.add_input(input.tx_input(&self.key_manager).await?.clone());
        }
        for output in &self.custom_outputs {
            core_tx_builder.add_output(output.tx_output(&self.key_manager).await?);
        }
        let mut sent_outputs = Vec::new();
        for recipient in &self.recipient_outputs {
            let output = recipient.output.tx_output(&self.key_manager).await?;
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
            &TransactionKernelVersion::get_current_version(),
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
                        &input.output.spending_key_id,
                        &input.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Input,
                    )
                    .await?
                    .to_schnorr_signature()?);
            offset = offset -
                self.key_manager
                    .get_txo_private_kernel_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await?;
            script_keys.push(input.output.script_key_id.clone());
        }

        for output in &self.custom_outputs {
            signature = &signature +
                self.key_manager
                    .get_partial_txo_kernel_signature(
                        &output.output.spending_key_id,
                        &output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )
                    .await?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await?;
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
                        &output.output.output.spending_key_id,
                        &output.output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )
                    .await?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&output.output.output.spending_key_id, &output.output.kernel_nonce)
                    .await?;
            let sender_offset_key_id = output
                .output
                .sender_offset_key_id
                .clone()
                .ok_or(TransactionBuilderError::SenderOffsetKeyIdMissing)?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        if let Some(change) = &change_output {
            core_tx_builder.add_output(change.output.to_transaction_output(&self.key_manager).await?);
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &change.output.spending_key_id,
                        &change.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &self.kernel_features,
                        TxoStage::Output,
                    )
                    .await?
                    .to_schnorr_signature()?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await?;
            let sender_offset_key_id = change
                .sender_offset_key_id
                .clone()
                .ok_or(TransactionBuilderError::SenderOffsetKeyIdMissing)?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        let script_offset = self
            .key_manager
            .get_script_offset(&script_keys, &sender_offset_keys)
            .await?;

        core_tx_builder.add_offset(offset);
        core_tx_builder.add_script_offset(script_offset);
        let excess = CompressedCommitment::from_compressed_key(total_public_excess);

        let kernel = KernelBuilder::new()
            .with_fee(total_fee)
            .with_features(self.kernel_features)
            .with_lock_height(self.lock_height)
            .with_burn_commitment(self.burn_commitment.clone())
            .with_excess(&excess)
            .with_signature(Signature::new_from_schnorr(signature))
            .build()?;
        core_tx_builder.with_kernel(kernel);
        let tx = core_tx_builder.build()?;

        let destination_addresses = self
            .recipient_outputs
            .iter()
            .map(|r| r.recipient_address.clone())
            .collect::<Vec<TariAddress>>();

        let mut amount = self.recipient_outputs.iter().map(|r| r.output.output.value).try_fold(
            MicroMinotari::zero(),
            |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            },
        )?;
        amount += self
            .custom_outputs
            .iter()
            .map(|o| o.output.value)
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x)
                    .ok_or(TransactionBuilderError::TransactionAmountOverflow)
            })?;
        let mut sent_hashes = Vec::new();
        for recipient in &self.recipient_outputs {
            sent_hashes.push(recipient.output.tx_output(&self.key_manager).await?.hash());
        }
        let mut received_hashes = Vec::new();
        for output in &self.custom_outputs {
            received_hashes.push(output.tx_output(&self.key_manager).await?.hash());
        }
        let change_output_hash = match &change_output {
            Some(o) => vec![o.output.to_transaction_output(&self.key_manager).await?.hash()],
            None => vec![],
        };
        Ok(FinalizedTransaction {
            source_address: self.own_address,
            destination_addresses,
            amount,
            fee: total_fee,
            transaction: tx,
            payment_id: self.memo_field.unwrap_or_default(),
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
    use tari_common_types::key_branches::TransactionKeyManagerBranch;
    use tari_script::{script, TariScript};

    use super::*;
    use crate::{
        test_helpers::{create_consensus_constants, create_consensus_rules},
        transactions::{
            tari_amount::{uT, MicroMinotari},
            test_helpers::{create_test_input, create_wallet_output_with_data, TestParams, UtxoTestParams},
            transaction_builder::TransactionBuilder,
            transaction_components::{memo_field::MemoField, OutputFeatures, WalletOutputBuilder},
            transaction_key_manager::create_memory_db_key_manager,
            CryptoFactories,
        },
        validation::transaction::TransactionInternalConsistencyValidator,
    };

    /// Hit the edge case where our change isn't enough to cover the cost of an extra output
    #[tokio::test]
    #[allow(clippy::identity_op)]
    async fn change_edge_case() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager().unwrap();
        let p = TestParams::new(&key_manager).await;
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
        )
        .await;
        let output = p
            .create_output(
                UtxoTestParams {
                    value: 2000 * uT,
                    ..Default::default()
                },
                &key_manager,
            )
            .await
            .unwrap();
        // Start the builder
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet)
            .await
            .unwrap();
        builder
            .with_lock_height(0)
            .with_output(output, p.sender_offset_key_id)
            .await
            .unwrap()
            .with_input(input)
            .await
            .unwrap()
            .with_fee_per_gram(MicroMinotari(1))
            .with_prevent_fee_gt_amount(false);
        let result = builder.build().await.unwrap();
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

    #[tokio::test]
    async fn too_many_inputs() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager().unwrap();
        let p = TestParams::new(&key_manager).await;

        let output = create_wallet_output_with_data(
            script!(Nop).unwrap(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(500),
            &key_manager,
        )
        .await
        .unwrap();
        let constants = create_consensus_constants(0);
        // Start the builder
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet)
            .await
            .unwrap();
        builder
            .with_lock_height(0)
            .with_output(output, p.sender_offset_key_id)
            .await
            .unwrap()
            .with_fee_per_gram(MicroMinotari(2));
        let input_base = create_test_input(MicroMinotari(50), 0, &key_manager, vec![], None).await;
        for _ in 0..=MAX_TRANSACTION_INPUTS {
            builder.with_input(input_base.clone()).await.unwrap();
        }
        let _err = builder.build().await.unwrap_err();
        // this needs a refactor to get in, we cannot enable partialeq TransactionBuilderError
        // assert_eq!(err, TransactionBuilderError::ExceedsMaxInputs(MAX_TRANSACTION_INPUTS));
    }

    #[tokio::test]
    async fn not_enough_funds() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager().unwrap();
        let p = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(400), 0, &key_manager, vec![], None).await;
        let script = script!(Nop).unwrap();
        let output = create_wallet_output_with_data(
            script.clone(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(400),
            &key_manager,
        )
        .await
        .unwrap();
        // Start the builder
        let constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet)
            .await
            .unwrap();
        builder
            .with_lock_height(0)
            .with_input(input)
            .await
            .unwrap()
            .with_output(output, p.sender_offset_key_id.clone())
            .await
            .unwrap()
            .with_fee_per_gram(MicroMinotari(1));
        let _err = builder.build().await.unwrap_err();

        // this needs a refactor to get in, we cannot enable partialeq TransactionBuilderError
        // assert_eq!(err, TransactionBuilderError::SpendingMoreThanAvailable {
        //     available: MicroMinotari(400),
        //     sent: MicroMinotari(400)
        // });
    }

    #[tokio::test]
    async fn zero_recipient_outputs() {
        let key_manager = create_memory_db_key_manager().unwrap();
        let p1 = TestParams::new(&key_manager).await;
        let p2 = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None).await;
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet)
                .await
                .unwrap();
        let script = TariScript::default();
        let output_features = OutputFeatures::default();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(2))
            .with_input(input)
            .await
            .unwrap()
            .with_output(
                create_wallet_output_with_data(
                    script.clone(),
                    output_features.clone(),
                    &p1,
                    MicroMinotari(500),
                    &key_manager,
                )
                .await
                .unwrap(),
                p1.sender_offset_key_id.clone(),
            )
            .await
            .unwrap()
            .with_output(
                create_wallet_output_with_data(script, output_features, &p2, MicroMinotari(400), &key_manager)
                    .await
                    .unwrap(),
                p2.sender_offset_key_id.clone(),
            )
            .await
            .unwrap();
        let finalized = builder.build().await.unwrap();
        let tx = finalized.transaction;
        let rules = create_consensus_rules();
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn single_recipient_no_change() {
        let rules = create_consensus_rules();
        let factories = CryptoFactories::default();
        let key_manager = create_memory_db_key_manager().unwrap();
        let bob_key = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(1200), 0, &key_manager, vec![], None).await;
        let utxo = input.to_transaction_input(&key_manager).await.unwrap();
        let script = script!(Nop).unwrap();
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet)
            .await
            .unwrap();
        let fee_per_gram = MicroMinotari(4);
        let fee = Fee::new(*consensus_constants.transaction_weight_params()).calculate(fee_per_gram, 1, 1, 1, 0);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .await
            .unwrap();
        let bob_sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(
            MicroMinotari(1200) - fee - MicroMinotari(10),
            bob_key.commitment_mask_key_id,
        )
        .with_features(OutputFeatures::default())
        .with_script(script.clone())
        .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
        .await
        .unwrap()
        .with_input_data(Default::default())
        .with_sender_offset_public_key(bob_public_key)
        .with_script_key(bob_key.script_key_id)
        .with_minimum_value_promise(0.into())
        .sign_as_sender_and_receiver_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
        .await
        .unwrap()
        .try_build(&key_manager)
        .await
        .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id))
            .await
            .unwrap();

        let finalized = builder.build().await.unwrap();

        let tx = finalized.transaction;
        assert_eq!(tx.body.kernels().first().unwrap().fee, fee + MicroMinotari(10)); // Check the twist above
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.inputs().first().unwrap().commitment(), utxo.commitment());
        assert_eq!(tx.body.outputs().len(), 1);
        assert!(tx.body.outputs().first().unwrap().verify_metadata_signature().is_ok());
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn single_recipient_with_change() {
        let rules = create_consensus_rules();
        let key_manager = create_memory_db_key_manager().unwrap();
        let factories = CryptoFactories::default();
        // Alice's parameters
        let alice_key = TestParams::new(&key_manager).await;
        // Bob's parameters
        let bob_key = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(25000), 0, &key_manager, vec![], None).await;
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet)
            .await
            .unwrap();
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
            .await
            .unwrap();
        let bob_sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();

        let bob_output = WalletOutputBuilder::new(MicroMinotari(5000), bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .await
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_as_sender_and_receiver_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .await
            .unwrap()
            .try_build(&key_manager)
            .await
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id))
            .await
            .unwrap();
        // Transaction should be complete
        let finalized = builder.build().await.unwrap();
        let tx = finalized.transaction;
        assert_eq!(tx.body.kernels().first().unwrap().fee, expected_fee);
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn single_recipient_multiple_inputs_with_change() {
        let rules = create_consensus_rules();
        let key_manager = create_memory_db_key_manager().unwrap();
        let factories = CryptoFactories::default();
        // Bob's parameters
        let bob_key = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(10000), 0, &key_manager, vec![], None).await;
        let input2 = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![], None).await;
        let input3 = create_test_input(MicroMinotari(15000), 0, &key_manager, vec![], None).await;
        let consensus_constants = create_consensus_constants(0);
        let mut builder = TransactionBuilder::new(consensus_constants.clone(), key_manager.clone(), Network::LocalNet)
            .await
            .unwrap();
        let script = script!(Nop).unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroMinotari(20))
            .with_input(input)
            .await
            .unwrap()
            .with_input(input2)
            .await
            .unwrap()
            .with_input(input3)
            .await
            .unwrap();
        let bob_sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(MicroMinotari(5000), bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .await
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_as_sender_and_receiver_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .await
            .unwrap()
            .try_build(&key_manager)
            .await
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id))
            .await
            .unwrap();
        let finalized = builder.build().await.unwrap();

        let tx = finalized.transaction;
        assert_eq!(tx.body.inputs().len(), 3);
        assert_eq!(tx.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(&tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn disallow_fee_larger_than_amount() {
        // Alice's parameters
        let key_manager = create_memory_db_key_manager().unwrap();
        let (utxo_amount, fee_per_gram, amount) = (MicroMinotari(2500), MicroMinotari(10), MicroMinotari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager, vec![], None).await;
        let script = script!(Nop).unwrap();
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet)
                .await
                .unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .await
            .unwrap();

        let bob_key = TestParams::new(&key_manager).await;
        let bob_sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(amount, bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .await
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_as_sender_and_receiver_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .await
            .unwrap()
            .try_build(&key_manager)
            .await
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id))
            .await
            .unwrap();
        let _err = builder.build().await.unwrap_err();

        // this needs a refactor to get in, we cannot enable partialeq TransactionBuilderError
        // assert_eq!(err, TransactionBuilderError::FeeGreaterThanAmount);
    }

    #[tokio::test]
    async fn allow_fee_larger_than_amount() {
        // Alice's parameters
        let key_manager = create_memory_db_key_manager().unwrap();
        let (utxo_amount, fee_per_gram, amount) = (MicroMinotari(2500), MicroMinotari(10), MicroMinotari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager, vec![], None).await;
        let script = script!(Nop).unwrap();
        let mut builder =
            TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet)
                .await
                .unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .await
            .unwrap()
            .with_prevent_fee_gt_amount(false);

        let bob_key = TestParams::new(&key_manager).await;
        let bob_sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let bob_public_key = bob_sender_offset.pub_key.clone();
        let bob_output = WalletOutputBuilder::new(amount, bob_key.commitment_mask_key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::new_empty())
            .await
            .unwrap()
            .with_input_data(Default::default())
            .with_sender_offset_public_key(bob_public_key)
            .with_script_key(bob_key.script_key_id)
            .with_minimum_value_promise(0.into())
            .sign_as_sender_and_receiver_verified(&key_manager, &bob_sender_offset.key_id, &Default::default())
            .await
            .unwrap()
            .try_build(&key_manager)
            .await
            .unwrap();

        builder
            .add_recipient(Default::default(), bob_output, Some(bob_sender_offset.key_id))
            .await
            .unwrap();
        // Test if the transaction passes the initial 'fee greater than amount' check when it is constructed
        match builder.build().await {
            Ok(_) => {},
            Err(e) => panic!("Unexpected error: {e:?}"),
        };
    }
}
