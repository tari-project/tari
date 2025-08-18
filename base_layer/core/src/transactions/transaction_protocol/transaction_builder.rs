// Copyright 2024 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use std::fmt::{Debug, Formatter};

use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, PrivateKey, Signature},
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_script::{ExecutionStack, TariScript};

use crate::{
    consensus::ConsensusConstants,
    covenants::Covenant,
    one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key},
    transactions::{
        fee::Fee,
        tari_amount::MicroMinotari,
        transaction_components::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionBuilder as CoreTransactionBuilder,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
            WalletOutputBuilder,
            MAX_TRANSACTION_INPUTS,
            MAX_TRANSACTION_OUTPUTS,
        },
        transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface, TxoStage},
        transaction_protocol::{
            sender::{OutputPair, RawTransactionInfo},
            transaction_initializer::RecipientDetails,
            TransactionMetadata,
        },
    },
};

pub const LOG_TARGET: &str = "c::tx::tx_protocol::tx_builder";

/// Change output details for transaction building
#[derive(Clone, Debug)]
pub(super) struct ChangeDetails {
    pub change_commitment_mask_key_id: TariKeyId,
    pub change_script: TariScript,
    pub change_input_data: ExecutionStack,
    pub change_script_key_id: TariKeyId,
    pub change_covenant: Covenant,
    pub own_address: TariAddress,
}

/// Error structure for build failures
pub struct BuildError<KM> {
    pub builder: OneSidedTransactionBuilder<KM>,
    pub message: String,
}

impl<KM> Debug for BuildError<KM> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuildError").field("message", &self.message).finish()
    }
}

/// One-sided transaction builder that follows similar pattern to SenderTransactionInitializer
/// This builder handles the cryptographic construction of one-sided transactions
pub struct OneSidedTransactionBuilder<KM> {
    lock_height: Option<u64>,
    fee_per_gram: Option<MicroMinotari>,
    inputs: Vec<OutputPair>,
    sender_custom_outputs: Vec<OutputPair>,
    change: Option<ChangeDetails>,
    recipient: Option<RecipientDetails>,
    tx_id: Option<TxId>,
    kernel_features: KernelFeatures,
    burn_commitment: Option<CompressedCommitment>,
    fee: Option<MicroMinotari>,
    key_manager: KM,
    sender_address: TariAddress,
    consensus_constants: ConsensusConstants,
}

impl<KM> OneSidedTransactionBuilder<KM>
where KM: TransactionKeyManagerInterface
{
    /// Create a new OneSidedTransactionBuilder
    pub fn new(consensus_constants: ConsensusConstants, key_manager: KM, sender_address: TariAddress) -> Self {
        Self {
            lock_height: None,
            fee_per_gram: None,
            inputs: Vec::new(),
            sender_custom_outputs: Vec::new(),
            change: None,
            recipient: None,
            tx_id: Some(TxId::new_random()),
            kernel_features: KernelFeatures::default(),
            burn_commitment: None,
            fee: None,
            key_manager,
            sender_address,
            consensus_constants,
        }
    }

    /// Set the fee per gram
    pub fn with_fee_per_gram(mut self, fee_per_gram: MicroMinotari) -> Self {
        self.fee_per_gram = Some(fee_per_gram);
        self
    }

    /// Set the lock height
    pub fn with_lock_height(mut self, lock_height: u64) -> Self {
        self.lock_height = Some(lock_height);
        self
    }

    /// Add an input to the transaction
    pub async fn with_input(mut self, input: WalletOutput, kernel_nonce: TariKeyId) -> Result<Self, String> {
        if self.inputs.len() >= MAX_TRANSACTION_INPUTS {
            return Err("Too many inputs".to_string());
        }
        self.inputs.push(OutputPair {
            output: input,
            kernel_nonce,
            sender_offset_key_id: None,
        });
        Ok(self)
    }

    /// Add an output to the transaction
    pub async fn with_output(
        mut self,
        output: WalletOutput,
        kernel_nonce: TariKeyId,
        sender_offset_key_id: TariKeyId,
    ) -> Result<Self, String> {
        if self.sender_custom_outputs.len() >= MAX_TRANSACTION_OUTPUTS {
            return Err("Too many outputs".to_string());
        }
        self.sender_custom_outputs.push(OutputPair {
            output,
            kernel_nonce,
            sender_offset_key_id: Some(sender_offset_key_id),
        });
        Ok(self)
    }

    /// Set recipient details for one-sided transaction
    pub async fn with_recipient_data(
        mut self,
        amount: MicroMinotari,
        recipient_address: TariAddress,
        recipient_output_features: OutputFeatures,
        recipient_script: TariScript,
        recipient_covenant: Covenant,
        recipient_minimum_value_promise: MicroMinotari,
    ) -> Result<Self, String> {
        let recipient_sender_offset_key_id = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::OneSidedSenderOffset.get_branch_key())
            .await
            .map_err(|e| e.to_string())?;

        let recipient_ephemeral_public_key_nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .map_err(|e| e.to_string())?;

        self.recipient = Some(RecipientDetails {
            amount,
            recipient_output_features,
            recipient_script,
            recipient_sender_offset_key_id: recipient_sender_offset_key_id.key_id,
            recipient_covenant,
            recipient_minimum_value_promise,
            recipient_ephemeral_public_key_nonce: recipient_ephemeral_public_key_nonce.key_id,
            recipient_address,
        });
        Ok(self)
    }

    /// Set change details
    pub fn with_change_data(
        mut self,
        change_script: TariScript,
        change_input_data: ExecutionStack,
        change_script_key_id: TariKeyId,
        change_covenant: Covenant,
        change_commitment_mask_key_id: TariKeyId,
    ) -> Self {
        self.change = Some(ChangeDetails {
            change_commitment_mask_key_id,
            change_script,
            change_input_data,
            change_script_key_id,
            change_covenant,
            own_address: self.sender_address.clone(),
        });
        self
    }

    /// Set the transaction ID
    pub fn with_tx_id(mut self, tx_id: TxId) -> Self {
        self.tx_id = Some(tx_id);
        self
    }

    /// Set kernel features
    pub fn with_kernel_features(mut self, features: KernelFeatures) -> Self {
        self.kernel_features = features;
        self
    }

    /// Set burn commitment
    pub fn with_burn_commitment(mut self, burn_commitment: Option<CompressedCommitment>) -> Self {
        self.burn_commitment = burn_commitment;
        self
    }

    /// Helper to check if a value is set
    fn check_value<T>(name: &str, val: &Option<T>, message: &mut Vec<String>) {
        if val.is_none() {
            message.push(name.to_string());
        }
    }

    /// Create a build error
    fn build_err(self, msg: &str) -> Result<(Transaction, Option<OutputPair>, MicroMinotari), BuildError<KM>> {
        Err(BuildError {
            builder: self,
            message: msg.to_string(),
        })
    }

    /// Get a build error
    fn get_build_err(&self, msg: &str) -> BuildError<KM> {
        BuildError {
            builder: unsafe { std::ptr::read(self) },
            message: msg.to_string(),
        }
    }

    /// Calculate the total fee for the transaction
    fn calculate_fee(&self, num_outputs: usize) -> Result<MicroMinotari, String> {
        let fee_per_gram = self.fee_per_gram.ok_or("Fee per gram not set")?;
        let num_kernels = 1;
        let num_inputs = self.inputs.len();

        Ok(Fee::from(fee_per_gram).calculate(
            self.consensus_constants.transaction_weight_params(),
            1,
            num_inputs,
            num_outputs,
            num_kernels,
        ))
    }

    /// Calculate total nonce and public excess for the transaction
    async fn calculate_total_nonce_and_total_public_excess(
        &self,
        info: &RawTransactionInfo,
    ) -> Result<(CompressedPublicKey, CompressedPublicKey), String> {
        let mut total_public_nonce = CompressedPublicKey::default();
        let mut total_public_excess = CompressedPublicKey::default();

        for input in &info.inputs {
            total_public_nonce = total_public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&input.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;

            total_public_excess = total_public_excess -
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;
        }

        for output in &info.outputs {
            total_public_nonce = total_public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&output.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;

            total_public_excess = total_public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;
        }

        if let Some(change) = &info.change_output {
            total_public_nonce = total_public_nonce +
                self.key_manager
                    .get_public_key_at_key_id(&change.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;

            total_public_excess = total_public_excess +
                self.key_manager
                    .get_txo_kernel_signature_excess_with_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;
        }

        Ok((
            CompressedPublicKey::from(total_public_nonce),
            CompressedPublicKey::from(total_public_excess),
        ))
    }

    /// Build the one-sided transaction with proper cryptographic steps
    pub async fn build(self) -> Result<(Transaction, Option<OutputPair>, MicroMinotari), BuildError<KM>> {
        // Validate required fields
        let mut message = Vec::new();
        Self::check_value("Missing Lock Height", &self.lock_height, &mut message);
        Self::check_value("Missing Fee per gram", &self.fee_per_gram, &mut message);
        Self::check_value("Missing Recipient", &self.recipient, &mut message);

        if !message.is_empty() {
            return self.build_err(&message.join(","));
        }

        if self.inputs.is_empty() {
            return self.build_err("A transaction cannot have zero inputs");
        }

        if self.inputs.len() > MAX_TRANSACTION_INPUTS {
            return self.build_err("Too many inputs in transaction");
        }

        let tx_id = self.tx_id.unwrap_or_else(TxId::new_random);
        let lock_height = self.lock_height.unwrap();
        let recipient = self.recipient.as_ref().unwrap();

        // Calculate shared secret for one-sided transaction
        let dest_public_key = recipient
            .recipient_address
            .public_view_key()
            .ok_or_else(|| self.get_build_err("Recipient must have public view key"))?;

        let shared_secret = self
            .key_manager
            .get_diffie_hellman_shared_secret(&recipient.recipient_sender_offset_key_id, dest_public_key)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        // Derive spending and encryption keys from shared secret
        let commitment_mask_private_key =
            shared_secret_to_output_spending_key(&shared_secret).map_err(|e| self.get_build_err(&e.to_string()))?;
        let encryption_private_key =
            shared_secret_to_output_encryption_key(&shared_secret).map_err(|e| self.get_build_err(&e.to_string()))?;

        let spending_key_id = self
            .key_manager
            .import_key(commitment_mask_private_key)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        let _encryption_key_id = self
            .key_manager
            .import_key(encryption_private_key)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        // Build recipient output
        let recipient_output = self
            .build_recipient_output(&spending_key_id)
            .await
            .map_err(|e| self.get_build_err(&e))?;

        // Calculate fee and determine if change is needed
        let total_input_value = self
            .calculate_total_input_value()
            .await
            .map_err(|e| self.get_build_err(&e))?;
        let total_output_value = recipient.amount;
        let fee = self.calculate_fee(1).map_err(|e| self.get_build_err(&e))?;

        let change_amount = total_input_value
            .checked_sub(total_output_value + fee)
            .ok_or_else(|| self.get_build_err("Insufficient funds"))?;

        // Build change output if needed
        let change_output = if change_amount > MicroMinotari::zero() {
            Some(
                self.build_change_output(change_amount)
                    .await
                    .map_err(|e| self.get_build_err(&e))?,
            )
        } else {
            None
        };

        // Prepare transaction info
        let raw_info = RawTransactionInfo {
            tx_id,
            recipient_data: self.recipient.clone(),
            recipient_output: Some(recipient_output.clone()),
            recipient_partial_kernel_excess: CompressedPublicKey::default(),
            recipient_partial_kernel_signature: Signature::default(),
            recipient_partial_kernel_offset: PrivateKey::default(),
            change_output: change_output.clone(),
            inputs: self.inputs.clone(),
            outputs: self.sender_custom_outputs.clone(),
            total_sender_excess: CompressedPublicKey::default(),
            total_sender_nonce: CompressedPublicKey::default(),
            metadata: TransactionMetadata {
                fee,
                lock_height,
                kernel_features: self.kernel_features.clone(),
                burn_commitment: self.burn_commitment.clone(),
            },
            payment_id: Default::default(),
            sender_address: self.sender_address.clone(),
        };

        // Calculate nonces and excesses
        let (total_nonce, total_excess) = self
            .calculate_total_nonce_and_total_public_excess(&raw_info)
            .await
            .map_err(|e| self.get_build_err(&e))?;

        // Build the final transaction
        let transaction = self
            .build_transaction(raw_info, total_nonce, total_excess)
            .await
            .map_err(|e| self.get_build_err(&e))?;

        Ok((transaction, change_output, change_amount))
    }

    /// Build the recipient output
    async fn build_recipient_output(&self, spending_key_id: &TariKeyId) -> Result<TransactionOutput, String> {
        let recipient = self.recipient.as_ref().ok_or("Recipient not set")?;

        let output_features = recipient.recipient_output_features.clone();
        let script = recipient.recipient_script.clone();
        let covenant = recipient.recipient_covenant.clone();
        let minimum_value_promise = recipient.recipient_minimum_value_promise;

        let output = WalletOutputBuilder::new(recipient.amount, spending_key_id.clone())
            .with_features(output_features)
            .with_script(script)
            .encrypt_data_for_recovery(&self.key_manager, None, Default::default())
            .await
            .map_err(|e| e.to_string())?
            .with_input_data(ExecutionStack::default())
            .with_version(TransactionOutputVersion::get_current_version())
            .with_minimum_value_promise(minimum_value_promise)
            .with_covenant(covenant)
            .sign_as_sender_and_receiver(&self.key_manager, &recipient.recipient_sender_offset_key_id)
            .await
            .map_err(|e| e.to_string())?
            .try_build(&self.key_manager)
            .await
            .map_err(|e| e.to_string())?;

        output
            .to_transaction_output(&self.key_manager)
            .await
            .map_err(|e| e.to_string())
    }

    /// Build change output if needed
    async fn build_change_output(&self, amount: MicroMinotari) -> Result<OutputPair, String> {
        let change = self.change.as_ref().ok_or("Change details not set")?;

        let kernel_nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .map_err(|e| e.to_string())?;

        let sender_offset_key_id = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .map_err(|e| e.to_string())?;

        let output = WalletOutputBuilder::new(amount, change.change_commitment_mask_key_id.clone())
            .with_features(OutputFeatures::default())
            .with_script(change.change_script.clone())
            .encrypt_data_for_recovery(&self.key_manager, None, Default::default())
            .await
            .map_err(|e| e.to_string())?
            .with_input_data(change.change_input_data.clone())
            .with_version(TransactionOutputVersion::get_current_version())
            .with_covenant(change.change_covenant.clone())
            .with_script_key(change.change_script_key_id.clone())
            .sign_as_sender_and_receiver(&self.key_manager, &sender_offset_key_id.key_id)
            .await
            .map_err(|e| e.to_string())?
            .try_build(&self.key_manager)
            .await
            .map_err(|e| e.to_string())?;

        Ok(OutputPair {
            output,
            kernel_nonce: kernel_nonce.key_id,
            sender_offset_key_id: Some(sender_offset_key_id.key_id),
        })
    }

    /// Calculate total input value
    async fn calculate_total_input_value(&self) -> Result<MicroMinotari, String> {
        let mut total = MicroMinotari::zero();
        for input in &self.inputs {
            total = total.checked_add(input.output.value).ok_or("Input value overflow")?;
        }
        Ok(total)
    }

    /// Build the final transaction
    async fn build_transaction(
        &self,
        info: RawTransactionInfo,
        total_nonce: CompressedPublicKey,
        total_excess: CompressedPublicKey,
    ) -> Result<Transaction, String> {
        let mut tx_builder = CoreTransactionBuilder::new();
        let kernel_version = TransactionKernelVersion::get_current_version();

        // Build kernel message
        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &kernel_version,
            info.metadata.fee,
            info.metadata.lock_height,
            &info.metadata.kernel_features,
            &info.metadata.burn_commitment,
        );

        // Calculate signatures and offsets
        let mut signature = Signature::default().to_schnorr_signature().map_err(|e| e.to_string())?;
        let mut offset = PrivateKey::default();
        let mut script_keys = Vec::new();
        let mut sender_offset_keys = Vec::new();

        // Process inputs
        for input in &info.inputs {
            tx_builder.add_input(
                input
                    .output
                    .to_transaction_input(&self.key_manager)
                    .await
                    .map_err(|e| e.to_string())?,
            );

            let partial_sig = self
                .key_manager
                .get_partial_txo_kernel_signature(
                    &input.output.spending_key_id,
                    &input.kernel_nonce,
                    &total_nonce,
                    &total_excess,
                    &kernel_version,
                    &kernel_message,
                    &info.metadata.kernel_features,
                    TxoStage::Input,
                )
                .await
                .map_err(|e| e.to_string())?;

            signature = &signature + &partial_sig.to_schnorr_signature().map_err(|e| e.to_string())?;

            offset = offset -
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;

            script_keys.push(input.output.script_key_id.clone());
        }

        // Process outputs
        for output in &info.outputs {
            tx_builder.add_output(
                output
                    .output
                    .to_transaction_output(&self.key_manager)
                    .await
                    .map_err(|e| e.to_string())?,
            );

            let partial_sig = self
                .key_manager
                .get_partial_txo_kernel_signature(
                    &output.output.spending_key_id,
                    &output.kernel_nonce,
                    &total_nonce,
                    &total_excess,
                    &kernel_version,
                    &kernel_message,
                    &info.metadata.kernel_features,
                    TxoStage::Output,
                )
                .await
                .map_err(|e| e.to_string())?;

            signature = &signature + &partial_sig.to_schnorr_signature().map_err(|e| e.to_string())?;

            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;

            if let Some(sender_offset_key_id) = &output.sender_offset_key_id {
                sender_offset_keys.push(sender_offset_key_id.clone());
            }
        }

        // Add recipient output
        if let Some(recipient_output) = &info.recipient_output {
            tx_builder.add_output(recipient_output.clone());
        }

        // Add recipient sender offset key
        if let Some(recipient_data) = &info.recipient_data {
            sender_offset_keys.push(recipient_data.recipient_sender_offset_key_id.clone());
        }

        // Process change output if present
        if let Some(change) = &info.change_output {
            tx_builder.add_output(
                change
                    .output
                    .to_transaction_output(&self.key_manager)
                    .await
                    .map_err(|e| e.to_string())?,
            );

            let partial_sig = self
                .key_manager
                .get_partial_txo_kernel_signature(
                    &change.output.spending_key_id,
                    &change.kernel_nonce,
                    &total_nonce,
                    &total_excess,
                    &kernel_version,
                    &kernel_message,
                    &info.metadata.kernel_features,
                    TxoStage::Output,
                )
                .await
                .map_err(|e| e.to_string())?;

            signature = &signature + &partial_sig.to_schnorr_signature().map_err(|e| e.to_string())?;

            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await
                    .map_err(|e| e.to_string())?;

            if let Some(sender_offset_key_id) = &change.sender_offset_key_id {
                sender_offset_keys.push(sender_offset_key_id.clone());
            }
        }

        // Calculate script offset
        let script_offset = self
            .key_manager
            .get_script_offset(&script_keys, &sender_offset_keys)
            .await
            .map_err(|e| e.to_string())?;

        tx_builder.add_offset(offset);
        tx_builder.add_script_offset(script_offset);

        // Build kernel
        let excess = CompressedCommitment::from_compressed_key(total_excess);
        let kernel = KernelBuilder::new()
            .with_fee(info.metadata.fee)
            .with_features(info.metadata.kernel_features)
            .with_lock_height(info.metadata.lock_height)
            .with_burn_commitment(info.metadata.burn_commitment)
            .with_excess(&excess)
            .with_signature(Signature::new_from_schnorr(signature))
            .build()
            .map_err(|e| e.to_string())?;

        tx_builder.with_kernel(kernel);

        tx_builder.build().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn test_build_one_sided_transaction() {}
}
