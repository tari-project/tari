// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use log::*;
use tari_common::configuration::Network;
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::{TariAddress, TariAddressFeatures},
    types::{CompressedCommitment, CompressedPublicKey, Signature, UncompressedPublicKey, UncompressedSignature},
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

pub struct TransactionBuilder<KM> {
    consensus_constants: ConsensusConstants,
    key_manager: KM,
    fee_per_gram: Option<MicroMinotari>,
    fee: MicroMinotari,
    recipients: Vec<RecipientDetails>,
    inputs: Vec<OutputPair>,
    sender_custom_outputs: Vec<OutputPair>,
    prevent_fee_gt_amount: bool,
    tx_type: TxType,
    payment_id: Option<MemoField>,
    lock_height: u64,
    kernel_features: KernelFeatures,
    network: Network,
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
            recipients: Vec::new(),
            inputs: Vec::new(),
            sender_custom_outputs: Vec::new(),
            prevent_fee_gt_amount: false,
            tx_type: TxType::PaymentToOther,
            payment_id: None,
            lock_height: 0,
            kernel_features: KernelFeatures::empty(),
            burn_commitment: None,
            network,
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
    pub fn with_payment_id(&mut self, payment_id: MemoField) -> &mut Self {
        self.payment_id = Some(payment_id);
        self
    }

    /// Add a recipient to the transaction.
    pub fn add_recipient(
        &mut self,
        recipient_address: TariAddress,
        recipient_output: WalletOutput,
        kernel_nonce: TariKeyId,
        sender_offset_key_id: Option<TariKeyId>,
    ) -> &mut Self {
        let recipient_output = OutputPair {
            output: recipient_output,
            kernel_nonce,
            sender_offset_key_id,
        };
        let recipient_details = RecipientDetails {
            output: recipient_output,
            recipient_address,
        };
        self.recipients.push(recipient_details);
        self
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
        self.sender_custom_outputs.push(pair);
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
        size += self
            .sender_custom_outputs
            .iter()
            .map(|o| {
                fee_weighting.weighting().round_up_features_and_scripts_size(
                    o.output
                        .features_and_scripts_byte_size()
                        .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string)),
                )
            })
            .collect::<Result<Vec<usize>, TransactionBuilderError>>()?
            .sum::<usize>();
        size += self
            .recipients
            .iter()
            .map(|o| {
                fee_weighting.weighting().round_up_features_and_scripts_size(
                    o.output
                        .features_and_scripts_byte_size()
                        .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e.to_string)),
                )
            })
            .collect::<Result<Vec<usize>, TransactionBuilderError>>()?
            .sum::<usize>();

        Ok(size)
    }

    fn check_conditions(&self) -> Result<(), TransactionBuilderError> {
        if self.fee_per_gram.is_none() && self.fee == MicroMinotari::zero() {
            return Err(TransactionBuilderError::FeeNotSet);
        }
        if self.recipients.is_empty() && self.sender_custom_outputs.is_empty() {
            return Err(TransactionBuilderError::NoRecipients);
        }
        if self.inputs.is_empty() {
            return Err(TransactionBuilderError::NoInputs);
        }
        if self.inputs.len() > MAX_TRANSACTION_INPUTS {
            return Err(TransactionBuilderError::ExceedsMaxInputs(MAX_TRANSACTION_INPUTS));
        }
        if self.recipients.len() + self.sender_custom_outputs.len() > MAX_TRANSACTION_OUTPUTS {
            return Err(TransactionBuilderError::ExceedsMaxOutputs(MAX_TRANSACTION_OUTPUTS));
        }
        Ok(())
    }

    async fn add_change_if_required(&mut self) -> Result<(MicroMinotari, Option<OutputPair>), TransactionBuilderError> {
        // The number of outputs excluding a possible residual change output
        let num_outputs = self.sender_custom_outputs.len() + self.recipients.len();
        let num_inputs = self.inputs.len();
        let total_being_spent =
            self.inputs
                .iter()
                .map(|i| i.output.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        let mut total_sent =
            self.sender_custom_outputs
                .iter()
                .map(|o| o.output.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        total_sent +=
            self.recipients
                .iter()
                .map(|o| o.output.output.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        let fee_weighting = Fee::new(*self.consensus_constants.transaction_weight_params());
        let fee_without_change = match self.fee_per_gram {
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
        };

        let change_features_and_scripts_size = OutputFeatures::default()
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
            Some(MicroMinotari(0)) => (MicroMinotari(0), None),
            Some(v) => {
                let change_fee = match self.fee_per_gram {
                    Some(fee_per_gram) => {
                        fee_weighting.calculate(fee_per_gram, 0, 0, 1, change_features_and_scripts_size)
                    },
                    None => self.fee,
                };

                let change_amount = v.checked_sub(change_fee);
                match change_amount {
                    // You can't win. Just add the change to the fee (which is less than the cost of adding another
                    // output and go without a change output
                    None => (fee_without_change + v, None),
                    Some(MicroMinotari(0)) => (fee_without_change + v, None),
                    Some(v) => self.build_change(v).await?,
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

    async fn build_change(
        &self,
        amount: MicroMinotari,
    ) -> Result<(MicroMinotari, Option<OutputPair>), TransactionBuilderError> {
        let (change_commitment_mask_key, change_script_key) =
            self.key_manager.get_next_commitment_mask_and_script_key().await?;

        let sender_offset_public = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await?;
        let script = script!(PushPubKey(Box::new(change_script_key.pub_key.clone())))?;
        let input_data = ExecutionStack::default();
        let mut payment_id = MemoField::new_transaction_info(
            TariAddress::default(),
            MicroMinotari::default(),
            amount,
            true,
            self.tx_type,
            Vec::new(),
            self.payment_id
                .as_ref()
                .map(|pay_id| pay_id.payment_id_as_bytes())
                .unwrap_or_default(),
        )
        .map_err(|e| TransactionBuilderError::InvalidSerializedSize(e))?;

        // we only set for the first output, otherwise the extra data gets too large
        if let Some(recipient) = self.recipients.first() {
            payment_id.transaction_info_set_amount(recipient.output.output.value);
            match payment_id.get_type() {
                TxType::PaymentToOther => payment_id
                    .transaction_info_set_address(recipient.recipient_address.clone())
                    .map_err(|e| TransactionBuilderError::InvalidMemo(e))?,
                TxType::PaymentToSelf |
                TxType::CoinSplit |
                TxType::CoinJoin |
                TxType::ValidatorNodeRegistration |
                TxType::CodeTemplateRegistration |
                TxType::ClaimAtomicSwap |
                TxType::HtlcAtomicSwapRefund => payment_id
                    .transaction_info_set_address(self.own_address.clone())
                    .map_err(|e| TransactionBuilderError::InvalidMemo(e))?,
                _ => {},
            }
        } else {
            payment_id.transaction_info_set_amount(amount);
            payment_id
                .transaction_info_set_address(self.own_address.clone())
                .map_err(|e| TransactionBuilderError::InvalidMemo(e))?;
        }
        let mut sent_hashes = Vec::new();
        for recipient in &self.recipients {
            sent_hashes.push(recipient.output.tx_output(&self.key_manager).await?.hash());
        }
        for output in &self.sender_custom_outputs {
            sent_hashes.push(output.tx_output(&self.key_manager).await?.hash());
        }
        payment_id
            .transaction_info_set_sent_output_hashes(sent_hashes)
            .map_err(|e| TransactionBuilderError::InvalidMemo(e))?;

        let encrypted_data = self
            .key_manager
            .encrypt_data_for_recovery(
                &change_commitment_mask_key.key_id,
                None,
                amount.as_u64(),
                payment_id.clone(),
            )
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
            payment_id,
            &self.key_manager,
        )
        .await?;
        let nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        Ok((
            amount,
            Some(OutputPair::new(
                change_wallet_output,
                nonce.key_id,
                Some(sender_offset_public.key_id),
            )),
        ))
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
        for output in &self.sender_custom_outputs {
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

        for output in &self.recipients {
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
    pub async fn build(mut self) -> Result<FinalizedTransaction, TransactionBuilderError> {
        self.check_conditions()?;

        let (total_fee, change_output) = self.add_change_if_required().await?;
        let mut core_tx_builder = CoreTransactionBuilder::new();

        let (total_public_nonce, total_public_excess) = self
            .calculate_total_nonce_and_total_public_excess(&change_output)
            .await?;

        let mut script_keys = Vec::new();
        let mut sender_offset_keys = Vec::new();
        let mut offset = Default::default();
        let mut signature = UncompressedSignature::default();

        let kernel_version = TransactionKernelVersion::get_current_version();

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &TransactionKernelVersion::get_current_version(),
            total_fee,
            self.lock_height,
            &self.kernel_features,
            &self.burn_commitment,
        );

        for input in &self.inputs {
            core_tx_builder.add_input(input.tx_input(&self.key_manager).await?.clone());
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

        for output in &self.sender_custom_outputs {
            core_tx_builder.add_output(output.tx_output(&self.key_manager).await?);
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
            let sender_offset_key_id = output.sender_offset_key_id.clone();
            sender_offset_keys.push(sender_offset_key_id);
        }

        for output in &self.recipients {
            core_tx_builder.add_output(output.output.tx_output(&self.key_manager).await?);
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
            let sender_offset_key_id = output.output.sender_offset_key_id.clone();
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
            let sender_offset_key_id = change.sender_offset_key_id.clone();
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
            .with_features(self.kernel_features.clone())
            .with_lock_height(self.lock_height)
            .with_burn_commitment(self.burn_commitment.clone())
            .with_excess(&excess)
            .with_signature(Signature::new_from_schnorr(signature))
            .build()?;
        core_tx_builder.with_kernel(kernel);
        let tx = core_tx_builder.build()?;

        let destination_addresses = self
            .recipients
            .iter()
            .map(|r| r.recipient_address.clone())
            .collect::<Vec<TariAddress>>();

        let mut amount =
            self.recipients
                .iter()
                .map(|r| r.output.output.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        amount +=
            self.sender_custom_outputs
                .iter()
                .map(|o| o.output.value)
                .try_fold(MicroMinotari::zero(), |acc, x| {
                    acc.checked_add(x)
                        .ok_or(TransactionBuilderError::TransactionAmountOverflow)
                })?;
        let mut sent_hashes = Vec::new();
        for recipient in &self.recipients {
            sent_hashes.push(recipient.output.tx_output(&self.key_manager).await?.hash());
        }
        let mut received_hashes = Vec::new();
        for output in &self.sender_custom_outputs {
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
            payment_id: self.payment_id.unwrap_or_default(),
            // Hashes of outputs being sent to others (excluding change)
            sent_output_hashes: sent_hashes,
            // Hashes of outputs received from others (excluding change)
            received_output_hashes: received_hashes,
            // Hashes of change outputs (for reference)
            change_output_hashes: change_output_hash,
        })
    }
}
