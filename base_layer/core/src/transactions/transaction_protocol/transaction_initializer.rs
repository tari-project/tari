// Copyright 2019. The Tari Project
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

use std::fmt::{format, Debug, Error, Formatter};

use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, PrivateKey, Signature, UncompressedPublicKey},
};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_script::{ExecutionStack, TariScript};

use crate::{
    borsh::SerializedSize,
    consensus::ConsensusConstants,
    covenants::Covenant,
    one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key},
    transactions::{
        fee::Fee,
        tari_amount::*,
        transaction_components::{
            memo_field::{MemoField, TxType},
            KernelBuilder,
            OutputFeatures,
            Transaction,
            TransactionBuilder,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
            WalletOutputBuilder,
            MAX_TRANSACTION_INPUTS,
            MAX_TRANSACTION_OUTPUTS,
        },
        transaction_key_manager::{error::KeyManagerServiceError, TariKeyId, TransactionKeyManagerInterface, TxoStage},
        transaction_protocol::{
            proto::recipient_signed_message,
            sender::{OutputPair, RawTransactionInfo, SenderState, SenderTransactionProtocol},
            KernelFeatures,
            TransactionMetadata,
        },
    },
};

pub const LOG_TARGET: &str = "c::tx::tx_protocol::tx_initializer";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct ChangeDetails {
    change_commitment_mask_key_id: TariKeyId,
    change_script: TariScript,
    change_input_data: ExecutionStack,
    change_script_key_id: TariKeyId,
    change_covenant: Covenant,
    own_address: TariAddress,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct RecipientDetails {
    pub amount: MicroMinotari,
    pub recipient_output_features: OutputFeatures,
    pub recipient_script: TariScript,
    pub recipient_sender_offset_key_id: TariKeyId,
    pub recipient_covenant: Covenant,
    pub recipient_minimum_value_promise: MicroMinotari,
    pub recipient_ephemeral_public_key_nonce: TariKeyId,
    pub recipient_address: TariAddress,
}

/// The SenderTransactionProtocolBuilder is a Builder that helps set up the initial state for the Sender party of a new
/// transaction Typically you don't instantiate this object directly. Rather use
/// ```ignore
/// # use crate::SenderTransactionProtocol;
/// SenderTransactionProtocol::new(1);
/// ```
/// which returns an instance of this builder. Once all the sender's information has been added via the builder
/// methods, you can call `build()` which will return a
#[derive(Debug, Clone)]
pub struct SenderTransactionInitializer<KM> {
    lock_height: Option<u64>,
    fee_per_gram: Option<MicroMinotari>,
    inputs: Vec<OutputPair>,
    sender_custom_outputs: Vec<OutputPair>,
    change: Option<ChangeDetails>,
    recipient: Option<RecipientDetails>,
    payment_id: Option<MemoField>,
    prevent_fee_gt_amount: bool,
    tx_id: Option<TxId>,
    kernel_features: KernelFeatures,
    burn_commitment: Option<CompressedCommitment>,
    fee: Fee,
    key_manager: KM,
    sender_address: TariAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SingleRoundSenderData {
    /// The transaction id generated by the sender for the recipient
    pub tx_id: TxId,
    /// The amount, in µT, being sent to the recipient
    pub amount: MicroMinotari,
    /// The offset public excess for this transaction
    pub public_excess: CompressedPublicKey,
    /// The sender's public nonce
    pub public_nonce: CompressedPublicKey,
    /// Metadata used to construct the transaction kernel
    pub metadata: TransactionMetadata,
    /// A user payment ID for the sender/receiver
    pub payment_id: MemoField,
    /// The output's features
    pub features: OutputFeatures,
    /// Script
    pub script: TariScript,
    /// Script offset public key
    pub sender_offset_public_key: CompressedPublicKey,
    /// The sender's ephemeral nonce
    pub ephemeral_public_nonce: CompressedPublicKey,
    /// Covenant
    pub covenant: Covenant,
    /// The minimum value of the commitment that is proven by the range proof
    pub minimum_value_promise: MicroMinotari,
    /// The version of this transaction output
    pub output_version: TransactionOutputVersion,
    /// The version of this transaction kernel
    pub kernel_version: TransactionKernelVersion,
    /// The senders address
    pub sender_address: TariAddress,
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientSignedMessage {
    pub tx_id: TxId,
    pub output: TransactionOutput,
    pub public_spend_key: CompressedPublicKey,
    pub partial_signature: Signature,
    pub tx_metadata: TransactionMetadata,
    pub offset: PrivateKey,
}

pub struct BuildError<KM> {
    pub builder: Box<SenderTransactionInitializer<KM>>,
    pub message: String,
}

impl<KM> Debug for BuildError<KM> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.write_str(&self.message)
    }
}

impl<KM> SenderTransactionInitializer<KM>
where KM: TransactionKeyManagerInterface
{
    pub fn new(consensus_constants: &ConsensusConstants, key_manager: KM) -> Self {
        Self {
            fee: Fee::new(*consensus_constants.transaction_weight_params()),
            lock_height: None,
            fee_per_gram: None,
            inputs: Vec::new(),
            sender_custom_outputs: Vec::new(),
            change: None,
            payment_id: None,
            prevent_fee_gt_amount: true,
            recipient: None,
            kernel_features: KernelFeatures::empty(),
            burn_commitment: None,
            tx_id: None,
            sender_address: TariAddress::default(),
            key_manager,
        }
    }

    /// Set the fee per weight for the transaction. See (Fee::calculate)[Struct.Fee.html#calculate] for how the
    /// absolute fee is calculated from the fee-per-gram value.
    pub fn with_fee_per_gram(&mut self, fee_per_gram: MicroMinotari) -> &mut Self {
        self.fee_per_gram = Some(fee_per_gram);
        self
    }

    /// Set the sender's address
    pub fn with_sender_address(&mut self, sender_address: TariAddress) -> &mut Self {
        self.sender_address = sender_address;
        self
    }

    /// Set the spending script of the ith recipient's output, a script offset will be generated for this recipient at
    /// the same time. This method will silently fail if `receiver_index` >= num_receivers.
    pub async fn with_recipient_data(
        &mut self,
        recipient_script: TariScript,
        recipient_output_features: OutputFeatures,
        recipient_covenant: Covenant,
        recipient_minimum_value_promise: MicroMinotari,
        amount: MicroMinotari,
        recipient_address: TariAddress,
    ) -> Result<&mut Self, KeyManagerServiceError> {
        let recipient_ephemeral_public_key_nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::MetadataEphemeralNonce.get_branch_key())
            .await?;
        let recipient_sender_offset = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await?;
        let recipient_details = RecipientDetails {
            recipient_output_features,
            recipient_script,
            recipient_sender_offset_key_id: recipient_sender_offset.key_id,
            recipient_covenant,
            recipient_minimum_value_promise,
            recipient_ephemeral_public_key_nonce: recipient_ephemeral_public_key_nonce.key_id,
            amount,
            recipient_address,
        };
        self.recipient = Some(recipient_details);
        Ok(self)
    }

    /// Sets the minimum block height that this transaction will be mined.
    pub fn with_lock_height(&mut self, lock_height: u64) -> &mut Self {
        self.lock_height = Some(lock_height);
        self
    }

    /// Adds an input to the transaction.
    pub async fn with_input(&mut self, input: WalletOutput) -> Result<&mut Self, KeyManagerServiceError> {
        let nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let pair = OutputPair {
            output: input,
            kernel_nonce: nonce.key_id,
            sender_offset_key_id: None,
        };
        self.inputs.push(pair);
        Ok(self)
    }

    /// As the Sender add an output to the transaction.
    pub async fn with_output(
        &mut self,
        output: WalletOutput,
        sender_offset_key_id: TariKeyId,
    ) -> Result<&mut Self, KeyManagerServiceError> {
        let nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let pair = OutputPair {
            output,
            kernel_nonce: nonce.key_id,
            sender_offset_key_id: Some(sender_offset_key_id),
        };
        self.sender_custom_outputs.push(pair);
        Ok(self)
    }

    /// Provide the change data that will be used to create change output.The amount of change will automatically be
    /// calculated when the transaction is built.
    pub fn with_change_data(
        &mut self,
        change_script: TariScript,
        change_input_data: ExecutionStack,
        change_script_key_id: TariKeyId,
        change_commitment_mask_key_id: TariKeyId,
        change_covenant: Covenant,
        own_address: TariAddress,
    ) -> &mut Self {
        let details = ChangeDetails {
            change_commitment_mask_key_id,
            change_script,
            change_input_data,
            change_script_key_id,
            change_covenant,
            own_address,
        };
        self.change = Some(details);
        self
    }

    /// Provide a payment id for receiver
    pub fn with_payment_id(&mut self, payment_id: MemoField) -> &mut Self {
        self.payment_id = Some(payment_id);
        self
    }

    /// This will select the desired kernel features to be signed by the receiver
    pub fn with_kernel_features(&mut self, features: KernelFeatures) -> &mut Self {
        self.kernel_features = features;
        self
    }

    /// This will allow the receipient to sign the burn commitment
    pub fn with_burn_commitment(&mut self, commitment: Option<CompressedCommitment>) -> &mut Self {
        self.burn_commitment = commitment;
        self
    }

    /// Enable or disable spending of an amount less than the fee
    pub fn with_prevent_fee_gt_amount(&mut self, prevent_fee_gt_amount: bool) -> &mut Self {
        self.prevent_fee_gt_amount = prevent_fee_gt_amount;
        self
    }

    fn get_total_features_and_scripts_size_for_outputs(&self) -> std::io::Result<usize> {
        let mut size = 0;
        size += self
            .sender_custom_outputs
            .iter()
            .map(|o| {
                self.fee.weighting().round_up_features_and_scripts_size(
                    o.output
                        .features_and_scripts_byte_size()
                        .expect("Invalid serialized size"),
                )
            })
            .sum::<usize>();
        if let Some(recipient_data) = &self.recipient {
            size += self.fee.weighting().round_up_features_and_scripts_size(
                self.get_recipient_output_features().get_serialized_size()? +
                    recipient_data.recipient_script.get_serialized_size()?,
            )
        }

        Ok(size)
    }

    fn get_recipient_output_features(&self) -> OutputFeatures {
        Default::default()
    }

    /// Tries to make a change output with the given transaction parameters and add it to the set of outputs. The total
    /// fee, including the additional change output (if any) is returned along with the amount of change.
    /// The change output **always has default output features**.
    #[allow(clippy::too_many_lines)]
    async fn add_change_if_required(
        &mut self,
    ) -> Result<(MicroMinotari, MicroMinotari, Option<(WalletOutput, TariKeyId)>), String> {
        // The number of outputs excluding a possible residual change output
        let num_outputs = self.sender_custom_outputs.len() + usize::from(self.recipient.is_some());
        let num_inputs = self.inputs.len();
        let total_being_spent = self
            .inputs
            .iter()
            .map(|i| i.output.value)
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x).ok_or("Total inputs being spent amount overflow")
            })?;
        let total_to_self = self
            .sender_custom_outputs
            .iter()
            .map(|o| o.output.value)
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x).ok_or("Total outputs to self amount overflow")
            })?;
        let total_amount = match &self.recipient {
            Some(data) => data.amount,
            None => 0.into(),
        };
        let fee_per_gram = self.fee_per_gram.ok_or("Fee per gram was not provided")?;

        let features_and_scripts_size_without_change = self
            .get_total_features_and_scripts_size_for_outputs()
            .map_err(|e| e.to_string())?;
        let fee_without_change = self.fee().calculate(
            fee_per_gram,
            1,
            num_inputs,
            num_outputs,
            features_and_scripts_size_without_change,
        );

        let output_features = OutputFeatures::default();
        let change_features_and_scripts_size = match &self.change {
            Some(data) => {
                data.change_script.get_serialized_size().map_err(|e| e.to_string())? +
                    OutputFeatures::default()
                        .get_serialized_size()
                        .map_err(|e| e.to_string())?
            },
            None => output_features.get_serialized_size().map_err(|e| e.to_string())?,
        };
        let change_features_and_scripts_size = self
            .fee()
            .weighting()
            .round_up_features_and_scripts_size(change_features_and_scripts_size);

        // Subtract with a check on going negative
        let total_input_value = [total_to_self, total_amount, fee_without_change]
            .iter()
            .try_fold(MicroMinotari::zero(), |acc, x| {
                acc.checked_add(x).ok_or("Total input value overflow")
            })?;
        let change_amount = total_being_spent.checked_sub(total_input_value);
        match change_amount {
            None => Err(format!(
                "You are spending more than you're providing: provided {}, required {}.",
                total_being_spent, total_input_value
            )),
            Some(MicroMinotari(0)) => Ok((fee_without_change, MicroMinotari(0), None)),
            Some(v) => {
                let change_fee = self
                    .fee()
                    .calculate(fee_per_gram, 0, 0, 1, change_features_and_scripts_size);
                let change_amount = v.checked_sub(change_fee);
                match change_amount {
                    // You can't win. Just add the change to the fee (which is less than the cost of adding another
                    // output and go without a change output
                    None => Ok((fee_without_change + v, MicroMinotari(0), None)),
                    Some(MicroMinotari(0)) => Ok((fee_without_change + v, MicroMinotari(0), None)),
                    Some(v) => {
                        let change_data = self.change.as_ref().ok_or("Change data was not provided")?;
                        let change_script = change_data.change_script.clone();
                        let change_script_key_id = change_data.change_script_key_id.clone();
                        let change_key_id = change_data.change_commitment_mask_key_id.clone();
                        let sender_offset_public = self
                            .key_manager
                            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
                            .await
                            .map_err(|e| e.to_string())?;
                        let input_data = change_data.change_input_data.clone();

                        let covenant = self
                            .change
                            .as_ref()
                            .ok_or("Change covenant was not provided")?
                            .change_covenant
                            .clone();
                        let own_address = self
                            .change
                            .as_ref()
                            .ok_or("address was not provided")?
                            .own_address
                            .clone();

                        let sender_one_sided = !self
                            .change
                            .as_ref()
                            .ok_or("address was not provided")?
                            .own_address
                            .features()
                            .contains(TariAddressFeatures::INTERACTIVE);

                        let mut payment_id = MemoField::new_transaction_info(
                            TariAddress::default(),
                            MicroMinotari::default(),
                            fee_without_change + change_fee,
                            sender_one_sided,
                            if self.kernel_features.is_burned() {
                                TxType::Burn
                            } else {
                                self.payment_id
                                    .as_ref()
                                    .map(|pay_id| pay_id.get_type())
                                    .unwrap_or_default()
                            },
                            Vec::new(),
                            self.payment_id
                                .as_ref()
                                .map(|pay_id| pay_id.payment_id_as_bytes())
                                .unwrap_or_default(),
                        )
                        .map_err(|e| e.to_string())?;
                        if let Some(recipient) = self.recipient.clone() {
                            payment_id.transaction_info_set_amount(recipient.amount);
                            match payment_id.get_type() {
                                TxType::PaymentToOther => {
                                    payment_id.transaction_info_set_address(recipient.recipient_address)?
                                },
                                TxType::PaymentToSelf |
                                TxType::CoinSplit |
                                TxType::CoinJoin |
                                TxType::ValidatorNodeRegistration |
                                TxType::CodeTemplateRegistration |
                                TxType::ClaimAtomicSwap |
                                TxType::HtlcAtomicSwapRefund => payment_id.transaction_info_set_address(own_address)?,
                                _ => {},
                            }
                        } else {
                            payment_id.transaction_info_set_amount(total_to_self);
                            payment_id.transaction_info_set_address(own_address)?;
                        }
                        trace!(target: LOG_TARGET, "Modified change payment id: {}, TxId: {:?}", payment_id, self.tx_id);

                        let encrypted_data = self
                            .key_manager
                            .encrypt_data_for_recovery(&change_key_id, None, v.as_u64(), payment_id.clone())
                            .await
                            .map_err(|e| e.to_string())?;

                        let minimum_value_promise = MicroMinotari::zero();

                        let output_version = TransactionOutputVersion::get_current_version();

                        let features = OutputFeatures::default();
                        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
                            &output_version,
                            &change_script,
                            &features,
                            &covenant,
                            &encrypted_data,
                            &minimum_value_promise,
                        );

                        let metadata_sig = self
                            .key_manager
                            .get_metadata_signature(
                                &change_key_id,
                                &v.into(),
                                &sender_offset_public.key_id,
                                &output_version,
                                &metadata_message,
                                features.range_proof_type,
                            )
                            .await
                            .map_err(|e| e.to_string())?;

                        let change_wallet_output = WalletOutput::new_current_version(
                            v,
                            change_key_id.clone(),
                            output_features,
                            change_script,
                            input_data,
                            change_script_key_id,
                            sender_offset_public.pub_key.clone(),
                            metadata_sig,
                            0,
                            covenant,
                            encrypted_data,
                            minimum_value_promise,
                            payment_id,
                            &self.key_manager,
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                        Ok((
                            fee_without_change + change_fee,
                            v,
                            Some((change_wallet_output, sender_offset_public.key_id)),
                        ))
                    },
                }
            },
        }
    }

    /// Specify the tx_id of this transaction, if not provided it will be calculated on build
    pub fn with_tx_id(&mut self, tx_id: TxId) -> &mut Self {
        self.tx_id = Some(tx_id);
        self
    }

    fn check_value<T>(name: &str, val: &Option<T>, vec: &mut Vec<String>) {
        if val.is_none() {
            vec.push(name.to_string());
        }
    }

    fn build_err<T>(self, msg: &str) -> Result<T, BuildError<KM>> {
        Err(BuildError {
            builder: Box::new(self),
            message: msg.to_string(),
        })
    }

    fn get_build_err(&self, msg: &str) -> BuildError<KM> {
        BuildError {
            builder: Box::new(self.clone()),
            message: msg.to_string(),
        }
    }

    pub(super) fn fee(&self) -> &Fee {
        &self.fee
    }

    /// Construct a `SenderTransactionProtocol` instance in and appropriate state. The data stored
    /// in the struct is _moved_ into the new struct. If any data is missing, the `self` instance is returned in the
    /// error (so that you can continue building) along with a string listing the missing fields.
    /// If all the input data is present, but one or more fields are invalid, the function will return a
    /// `SenderTransactionProtocol` instance in the Failed state.
    #[allow(clippy::too_many_lines)]
    pub async fn build(mut self) -> Result<(Transaction, Option<OutputPair>, MicroMinotari), BuildError<KM>> {
        // Compile a list of all data that is missing
        let mut message = Vec::new();
        Self::check_value("Missing Lock Height", &self.lock_height, &mut message);
        Self::check_value("Missing Fee per gram", &self.fee_per_gram, &mut message);

        if !message.is_empty() {
            return self.build_err(&message.join(","));
        }
        if self.inputs.is_empty() {
            return self.build_err("A transaction cannot have zero inputs");
        }
        // Prevent overflow attacks by imposing sane limits on inputs
        if self.inputs.len() > MAX_TRANSACTION_INPUTS {
            return self.build_err("Too many inputs in transaction");
        }
        // Calculate the fee based on whether we need to add a residual change output or not
        let (total_fee, change, change_output) = match self.add_change_if_required().await {
            Ok((fee, change, output)) => (fee, change, output),
            Err(e) => return self.build_err(&e),
        };
        debug!(
            target: LOG_TARGET,
            "Build transaction with Fee: {}. Change: {}. Output: {:?}", total_fee, change, change_output,
        );

        let change_output_pair = match change_output {
            Some((output, sender_offset_key_id)) => {
                if self.sender_custom_outputs.len() >= MAX_TRANSACTION_OUTPUTS {
                    return self.build_err("Too many outputs in transaction");
                }
                let nonce = match self
                    .key_manager
                    .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
                    .await
                {
                    Ok(key_id) => key_id,
                    Err(e) => return self.build_err(&e.to_string()),
                };
                Some(OutputPair {
                    output,
                    kernel_nonce: nonce.key_id,
                    sender_offset_key_id: Some(sender_offset_key_id),
                })
            },
            None => None,
        };

        // we need some random data here, the public excess of the commitment is random.
        let tx_id = match self.tx_id {
            Some(id) => id,
            None => TxId::new_random(),
        };

        // The fee should be less than the amount being sent. This isn't a protocol requirement, but it's what you want
        // 99.999% of the time, however, always preventing this will also prevent spending dust in some edge
        // cases.
        // Don't care about the fees when we are sending token.
        if let Some(data) = &self.recipient {
            if total_fee > data.amount {
                warn!(
                    target: LOG_TARGET,
                    "Fee ({}) is greater than amount ({}) being sent for Transaction (TxId: {}).",
                    total_fee,
                    data.amount,
                    tx_id
                );
                if self.prevent_fee_gt_amount {
                    return self.build_err("Fee is greater than amount");
                }
            }
        }

        let key = match self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::OneSidedSenderOffset.get_branch_key())
            .await
        {
            Ok(k) => k,
            Err(e) => return self.build_err(&e.to_string()),
        };

        let sender_offset_private_key = key.key_id;
        self.recipient = Some(RecipientDetails {
            recipient_sender_offset_key_id: sender_offset_private_key.clone(),
            ..self.recipient.expect("Recipient details should be set")
        });
        // Everything is here. Let's send some Minotari!
        let mut sender_info = RawTransactionInfo {
            tx_id,
            recipient_data: self.recipient.clone(),
            recipient_output: None,
            recipient_partial_kernel_excess: CompressedPublicKey::default(),
            recipient_partial_kernel_signature: Signature::default(),
            recipient_partial_kernel_offset: PrivateKey::default(),
            change_output: change_output_pair,
            total_sender_nonce: CompressedPublicKey::default(),
            total_sender_excess: CompressedPublicKey::default(),
            metadata: TransactionMetadata {
                fee: total_fee,
                lock_height: self.clone().lock_height.unwrap(),
                kernel_features: self.kernel_features.clone(),
                burn_commitment: self.burn_commitment.clone(),
            },
            inputs: self.inputs.clone(),
            outputs: self.sender_custom_outputs.clone(),
            payment_id: self.payment_id.clone().unwrap_or_default(),
            sender_address: self.sender_address.clone(),
        };

        let dest_address = self
            .recipient
            .clone()
            .expect("Recipient must be present")
            .recipient_address;
        let dest_public_key = dest_address
            .public_view_key()
            .expect("Receiptient must have public view key");
        // let single_round_message = self.get_single_round_message(&mut sender_info);

        let shared_secret = self
            .key_manager
            .get_diffie_hellman_shared_secret(&sender_offset_private_key, dest_public_key)
            .await
            .expect("Failed to calculate shared secret");
        let commitment_mask_private_key =
            shared_secret_to_output_spending_key(&shared_secret).expect("Failed to get commitment mask private key");
        let encryption_private_key =
            shared_secret_to_output_encryption_key(&shared_secret).expect("Failed to get encryption key");
        let encryption_key = self
            .key_manager
            .import_key(encryption_private_key)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        let spending_key_id = self
            .key_manager
            .import_key(commitment_mask_private_key)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()));

        let sender_offset_public_key = self
            .key_manager
            .get_public_key_at_key_id(&sender_offset_private_key)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        let output = self.get_output()?;
        let recipient_signed_message = self.get_recipient_signed_message(&sender_info, output);
        self.recipient_partial_kernel_excess = msg.public_spend_key;
        self.recipient_partial_kernel_signature = msg.partial_signature;
        self.recipient_partial_kernel_offset = msg.offset;
        if self.metadata.kernel_features.is_burned() {
            self.metadata.burn_commitment = Some(received_output.commitment.clone());
        }

        self.recipient_output = Some(received_output);
        let (transaction, change_output) = self.build_transaction(&sender_info).await?;
        Ok((
            Transaction::new(
                vec![],
                vec![],
                vec![],
                RistrettoSecretKey::default(),
                RistrettoSecretKey::default(),
            ),
            None,
            MicroMinotari::zero(),
        ))
    }

    async fn calculate_total_nonce_and_total_public_excess(
        &self,
        info: &RawTransactionInfo,
    ) -> Result<(CompressedPublicKey, CompressedPublicKey), String> {
        let key_manager = self.key_manager.clone();
        // lets calculate the total sender kernel signature nonce
        let mut public_nonce = UncompressedPublicKey::default();
        // lets calculate the total sender kernel exess
        let mut public_excess = UncompressedPublicKey::default();
        for input in &info.inputs {
            public_nonce = public_nonce +
                key_manager
                    .get_public_key_at_key_id(&input.kernel_nonce)
                    .await
                    .map_err(|e| format!("Failed to get public key: {}", e))?
                    .to_public_key()
                    .map_err(|e| e.to_string())?;
            public_excess = public_excess -
                key_manager
                    .get_txo_kernel_signature_excess_with_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await
                    .map_err(|e| format!("Failed to get kernel signature excess: {}", e))?
                    .to_public_key()
                    .map_err(|e| e.to_string())?;
        }
        for output in &info.outputs {
            public_nonce = public_nonce +
                key_manager
                    .get_public_key_at_key_id(&output.kernel_nonce)
                    .await
                    .map_err(|e| format!("Failed to get public key: {}", e))?
                    .to_public_key()
                    .map_err(|e| e.to_string())?;
            public_excess = public_excess +
                key_manager
                    .get_txo_kernel_signature_excess_with_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await
                    .map_err(|e| format!("Failed to get kernel signature excess: {}", e))?
                    .to_public_key()
                    .map_err(|e| e.to_string())?;
        }

        if let Some(change) = &info.change_output {
            public_nonce = public_nonce +
                key_manager
                    .get_public_key_at_key_id(&change.kernel_nonce)
                    .await
                    .map_err(|e| format!("Failed to get public key: {}", e))?
                    .to_public_key()
                    .map_err(|e| e.to_string())?;
            public_excess = public_excess +
                key_manager
                    .get_txo_kernel_signature_excess_with_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await
                    .map_err(|e| format!("Failed to get kernel signature excess: {}", e))?
                    .to_public_key()
                    .map_err(|e| e.to_string())?;
        }
        Ok((
            CompressedPublicKey::new_from_pk(public_nonce),
            CompressedPublicKey::new_from_pk(public_excess),
        ))
    }

    pub async fn get_single_round_message(
        &self,
        info: &mut RawTransactionInfo,
    ) -> Result<SingleRoundSenderData, String> {
        let key_manager = self.key_manager.clone();
        let recipient_data = info
            .recipient_data
            .as_ref()
            .ok_or_else(|| "Missing recipient data".to_string())?;
        let recipient_output_features = recipient_data.recipient_output_features.clone();
        let recipient_script = recipient_data.recipient_script.clone();
        let recipient_script_offset_secret_key_id = &recipient_data.recipient_sender_offset_key_id;
        let recipient_covenant = recipient_data.recipient_covenant.clone();
        let recipient_minimum_value_promise = recipient_data.recipient_minimum_value_promise;
        let amount = recipient_data.amount;
        let ephemeral_public_key_nonce = recipient_data.recipient_ephemeral_public_key_nonce.clone();

        let (public_nonce, public_excess) = self.calculate_total_nonce_and_total_public_excess(info).await?;
        let sender_offset_public_key = key_manager
            .get_public_key_at_key_id(recipient_script_offset_secret_key_id)
            .await
            .map_err(|e| format!("Failed to get public key: {}", e))?;
        // we update this as we send this to what we sent.
        info.total_sender_excess = public_excess.clone();
        info.total_sender_nonce = public_nonce.clone();

        let ephemeral_public_nonce = key_manager
            .get_public_key_at_key_id(&ephemeral_public_key_nonce)
            .await
            .map_err(|e| format!("Failed to get public key: {}", e))?;

        let output_version = TransactionOutputVersion::get_current_version();
        let kernel_version = TransactionKernelVersion::get_current_version();

        Ok(SingleRoundSenderData {
            tx_id: info.tx_id,
            amount,
            public_nonce,
            public_excess,
            metadata: info.metadata.clone(),
            payment_id: info.payment_id.clone(),
            features: recipient_output_features,
            script: recipient_script,
            sender_offset_public_key,
            ephemeral_public_nonce,
            covenant: recipient_covenant,
            minimum_value_promise: recipient_minimum_value_promise,
            output_version,
            kernel_version,
            sender_address: info.sender_address.clone(),
        })
    }

    pub async fn get_recipient_signed_message(
        &self,
        sender_info: &SingleRoundSenderData,
        output: &WalletOutput,
    ) -> Result<RecipientSignedMessage, BuildError<KM>> {
        let transaction_output = output
            .to_transaction_output(&self.key_manager)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        let public_nonce = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;
        let tx_meta = if output.is_burned() {
            let mut meta = sender_info.metadata.clone();
            meta.burn_commitment = Some(transaction_output.commitment().clone());
            meta
        } else {
            sender_info.metadata.clone()
        };
        let public_excess = &self
            .key_manager
            .get_txo_kernel_signature_excess_with_offset(&output.spending_key_id, &public_nonce.key_id)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &sender_info.kernel_version,
            tx_meta.fee,
            tx_meta.lock_height,
            &tx_meta.kernel_features,
            &tx_meta.burn_commitment,
        );
        let total_nonce = &sender_info
            .public_nonce
            .to_public_key()
            .map_err(|e| self.get_build_err(&e.to_string()))? +
            &public_nonce
                .pub_key
                .to_public_key()
                .map_err(|e| self.get_build_err(&e.to_string()))?;
        let total_excess = &sender_info
            .public_excess
            .to_public_key()
            .map_err(|e| self.get_build_err(&e.to_string()))? +
            &public_excess
                .to_public_key()
                .map_err(|e| self.get_build_err(&e.to_string()))?;
        let signature = &self
            .key_manager
            .get_partial_txo_kernel_signature(
                &output.spending_key_id,
                &public_nonce.key_id,
                &CompressedPublicKey::new_from_pk(total_nonce),
                &CompressedPublicKey::new_from_pk(total_excess),
                &sender_info.kernel_version,
                &kernel_message,
                &tx_meta.kernel_features,
                TxoStage::Output,
            )
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;
        let offset = &self
            .key_manager
            .get_txo_private_kernel_offset(&output.spending_key_id, &public_nonce.key_id)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        let data = RecipientSignedMessage {
            tx_id: sender_info.tx_id,
            output: transaction_output,
            public_spend_key: public_excess.clone(),
            partial_signature: signature.clone(),
            tx_metadata: tx_meta,
            offset: offset.clone(),
        };
        Ok(data)
    }

    async fn build_transaction(
        &self,
        info: &RawTransactionInfo,
    ) -> Result<(Transaction, Option<OutputPair>), BuildError<KM>> {
        let mut tx_builder = TransactionBuilder::new();
        let (total_public_nonce, total_public_excess) = if info.recipient_data.is_none() {
            // we dont have a recipient and thus we have not yet calculated the sender_nonce and sender_offset_excess
            self.calculate_total_nonce_and_total_public_excess(info)
                .await
                .map_err(|e| self.get_build_err(&e))?
        } else {
            let total_public_nonce = &info
                .total_sender_nonce
                .to_public_key()
                .map_err(|e| self.get_build_err(&e.to_string()))? +
                info.recipient_partial_kernel_signature
                    .get_compressed_public_nonce()
                    .to_public_key()
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            let total_public_excess = &info
                .total_sender_excess
                .to_public_key()
                .map_err(|e| self.get_build_err(&e.to_string()))? +
                &info
                    .recipient_partial_kernel_excess
                    .to_public_key()
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            (
                CompressedPublicKey::new_from_pk(total_public_nonce),
                CompressedPublicKey::new_from_pk(total_public_excess),
            )
        };

        // lets update our change if any
        let change_output = if let Some(change) = &info.change_output {
            let mut sent_hashes = Vec::new();
            if let Some(sent_output) = &info.recipient_output {
                sent_hashes.push(sent_output.hash());
            }

            let mut payment_id = change.output.payment_id.clone();
            payment_id
                .transaction_info_set_sent_output_hashes(sent_hashes)
                .map_err(|e| self.get_build_err(&e.to_string()))?;
            let encrypted_data = self
                .key_manager
                .encrypt_data_for_recovery(
                    &change.output.spending_key_id,
                    None,
                    change.output.value.as_u64(),
                    payment_id,
                )
                .await
                .map_err(|e| self.get_build_err(&e.to_string()))?;
            let mut change_output = change.output.clone();
            change_output
                .change_encrypted_data(
                    encrypted_data,
                    change
                        .sender_offset_key_id
                        .as_ref()
                        .ok_or(self.get_build_err("Sender offset key ID is missing"))?,
                    &self.key_manager,
                )
                .await
                .map_err(|e| self.get_build_err(&e.to_string()))?;
            Some(OutputPair {
                output: change_output,
                kernel_nonce: change.kernel_nonce.clone(),
                sender_offset_key_id: change.sender_offset_key_id.clone(),
            })
        } else {
            None
        };

        let mut offset = info.recipient_partial_kernel_offset.clone();
        let mut signature = info
            .recipient_partial_kernel_signature
            .clone()
            .to_schnorr_signature()
            .map_err(|e| self.get_build_err(&e.to_string()))?;
        let mut script_keys = Vec::new();
        let mut sender_offset_keys = Vec::new();
        let kernel_version = TransactionKernelVersion::get_current_version();

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &TransactionKernelVersion::get_current_version(),
            info.metadata.fee,
            info.metadata.lock_height,
            &info.metadata.kernel_features,
            &info.metadata.burn_commitment,
        );

        for input in &info.inputs {
            tx_builder.add_input(
                input
                    .output
                    .to_transaction_input(&self.key_manager)
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?,
            );
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &input.output.spending_key_id,
                        &input.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Input,
                    )
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?
                    .to_schnorr_signature()
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            offset = offset -
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            script_keys.push(input.output.script_key_id.clone());
        }

        for output in &info.outputs {
            tx_builder.add_output(
                output
                    .output
                    .to_transaction_output(&self.key_manager)
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?,
            );
            signature = &signature +
                &self
                    .key_manager
                    .get_partial_txo_kernel_signature(
                        &output.output.spending_key_id,
                        &output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &kernel_version,
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Output,
                    )
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?
                    .to_schnorr_signature()
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            let sender_offset_key_id = output
                .sender_offset_key_id
                .clone()
                .ok_or(self.get_build_err("No sender offset key id"))?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        if let Some(recipient_data) = &info.recipient_data {
            sender_offset_keys.push(recipient_data.recipient_sender_offset_key_id.clone());
        }
        if let Some(change) = &change_output {
            tx_builder.add_output(
                change
                    .output
                    .to_transaction_output(&self.key_manager)
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?,
            );
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
                        &info.metadata.kernel_features,
                        TxoStage::Output,
                    )
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?
                    .to_schnorr_signature()
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            offset = offset +
                &self
                    .key_manager
                    .get_txo_private_kernel_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await
                    .map_err(|e| self.get_build_err(&e.to_string()))?;
            let sender_offset_key_id = change
                .sender_offset_key_id
                .clone()
                .ok_or(self.get_build_err("Missing sender offset key id"))?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        if let Some(received_output) = &info.recipient_output {
            tx_builder.add_output(received_output.clone());
        }
        let script_offset = self
            .key_manager
            .get_script_offset(&script_keys, &sender_offset_keys)
            .await
            .map_err(|e| self.get_build_err(&e.to_string()))?;

        tx_builder.add_offset(offset);
        tx_builder.add_script_offset(script_offset);
        let excess = CompressedCommitment::from_compressed_key(total_public_excess);

        let kernel = KernelBuilder::new()
            .with_fee(info.metadata.fee)
            .with_features(info.metadata.kernel_features)
            .with_lock_height(info.metadata.lock_height)
            .with_burn_commitment(info.metadata.burn_commitment.clone())
            .with_excess(&excess)
            .with_signature(Signature::new_from_schnorr(signature))
            .build()
            .map_err(|e| self.get_build_err(&e.to_string()))?;
        tx_builder.with_kernel(kernel);
        let tx = tx_builder.build().map_err(|e| self.get_build_err(&e.to_string()))?;
        Ok((tx, change_output))
    }
}

//----------------------------------------         Tests          ----------------------------------------------------//

#[cfg(test)]
mod test {
    use tari_common_types::tari_address::TariAddress;
    use tari_script::{inputs, script};

    use crate::{
        covenants::Covenant,
        test_helpers::create_consensus_constants,
        transactions::{
            fee::Fee,
            tari_amount::*,
            test_helpers::{create_test_input, create_wallet_output_with_data, TestParams, UtxoTestParams},
            transaction_components::{OutputFeatures, MAX_TRANSACTION_INPUTS},
            transaction_key_manager::create_memory_db_key_manager,
            transaction_protocol::{sender::SenderState, transaction_initializer::SenderTransactionInitializer},
        },
    };

    /// One output, one input
    #[tokio::test]
    async fn no_change_or_receivers() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager().unwrap();
        let p = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(5000), 0, &key_manager, vec![], None).await;
        let constants = create_consensus_constants(0);
        let expected_fee = Fee::from(*constants.transaction_weight_params()).calculate(
            MicroMinotari(4),
            1,
            1,
            1,
            p.get_size_for_default_features_and_scripts(1)
                .expect("Failed to serialized size"),
        );

        let output = create_wallet_output_with_data(
            script!(Nop).unwrap(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(5000) - expected_fee,
            &key_manager,
        )
        .await
        .unwrap();
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(&constants, key_manager.clone());
        builder
            .with_lock_height(0)
            .with_output(output, p.sender_offset_key_id)
            .await
            .unwrap()
            .with_input(input)
            .await
            .unwrap()
            .with_fee_per_gram(MicroMinotari(4))
            .with_prevent_fee_gt_amount(false);
        let result = builder.build().await.unwrap();
        // Peek inside and check the results
        if let SenderState::Finalizing(info) = result.into_state() {
            assert_eq!(info.metadata.lock_height, 0, "Lock height");
            assert_eq!(info.metadata.fee, expected_fee, "Fee");
            assert_eq!(info.outputs.len(), 1, "There should be 1 output");
            assert_eq!(info.inputs.len(), 1, "There should be 1 input");
        } else {
            panic!("There were no recipients, so we should be finalizing");
        }
    }

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
        let mut builder = SenderTransactionInitializer::new(&constants, key_manager.clone());
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
        // Peek inside and check the results
        if let SenderState::Finalizing(info) = result.into_state() {
            assert_eq!(info.metadata.lock_height, 0, "Lock height");
            assert_eq!(info.metadata.fee, tx_fee + fee_for_change_output - 1 * uT, "Fee");
            assert_eq!(info.outputs.len(), 1, "There should be 1 output");
            assert_eq!(info.inputs.len(), 1, "There should be 1 input");
        } else {
            panic!("There were no recipients, so we should be finalizing");
        }
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
        let mut builder = SenderTransactionInitializer::new(&constants, key_manager.clone());
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
        let err = builder.build().await.unwrap_err();
        assert_eq!(err.message, "Too many inputs in transaction");
    }

    #[tokio::test]
    async fn zero_fee_allowed() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager().unwrap();
        let p = TestParams::new(&key_manager).await;
        let fee_per_gram = MicroMinotari(0);
        let tx_fee = p.fee().calculate(
            fee_per_gram,
            1,
            1,
            1,
            p.get_size_for_default_features_and_scripts(1)
                .expect("Failed to borsh serialized size"),
        );
        let input = create_test_input(500 * uT + tx_fee, 0, &key_manager, vec![], None).await;
        let script = script!(Nop).unwrap();
        // Start the builder
        let constants = create_consensus_constants(0);
        let mut builder = SenderTransactionInitializer::new(&constants, key_manager.clone());
        let change = TestParams::new(&key_manager).await;
        builder
            .with_lock_height(0)
            .with_input(input)
            .await
            .unwrap()
            .with_change_data(
                script!(Nop).unwrap(),
                inputs!(change.script_key_pk),
                change.script_key_id.clone(),
                change.commitment_mask_key_id.clone(),
                Covenant::default(),
                TariAddress::default(),
            )
            .with_fee_per_gram(fee_per_gram)
            .with_recipient_data(
                script,
                Default::default(),
                Default::default(),
                0.into(),
                MicroMinotari(500),
                TariAddress::default(),
            )
            .await
            .unwrap();
        assert!(builder.build().await.is_ok(), "Zero fee should be allowed");
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
        let mut builder = SenderTransactionInitializer::new(&constants, key_manager.clone());
        let change = TestParams::new(&key_manager).await;
        builder
            .with_lock_height(0)
            .with_input(input)
            .await
            .unwrap()
            .with_output(output, p.sender_offset_key_id.clone())
            .await
            .unwrap()
            .with_change_data(
                script!(Nop).unwrap(),
                inputs!(change.script_key_pk),
                change.script_key_id.clone(),
                change.commitment_mask_key_id.clone(),
                Covenant::default(),
                TariAddress::default(),
            )
            .with_fee_per_gram(MicroMinotari(1))
            .with_recipient_data(
                script.clone(),
                Default::default(),
                Default::default(),
                0.into(),
                MicroMinotari::zero(),
                TariAddress::default(),
            )
            .await
            .unwrap();
        let err = builder.build().await.unwrap_err();
        assert_eq!(
            err.message,
            "You are spending more than you're providing: provided 400 µT, required 528 µT."
        );
    }

    #[tokio::test]
    async fn single_recipient() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager().unwrap();
        let p = TestParams::new(&key_manager).await;
        let input1 = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![], None).await;
        let input2 = create_test_input(MicroMinotari(3000), 0, &key_manager, vec![], None).await;
        let fee_per_gram = MicroMinotari(6);

        let script = script!(Nop).unwrap();
        let constants = create_consensus_constants(0);
        let expected_fee = Fee::from(*constants.transaction_weight_params()).calculate(
            fee_per_gram,
            1,
            2,
            3,
            p.get_size_for_default_features_and_scripts(3)
                .expect("Failed to borsh serialized size"),
        );
        let output = create_wallet_output_with_data(
            script.clone(),
            OutputFeatures::default(),
            &p,
            MicroMinotari(1500) - expected_fee,
            &key_manager,
        )
        .await
        .unwrap();
        // Start the builder
        let mut builder = SenderTransactionInitializer::new(&constants, key_manager.clone());
        let change = TestParams::new(&key_manager).await;
        builder
            .with_lock_height(1234)
            .with_output(output, p.sender_offset_key_id.clone())
            .await
            .unwrap()
            .with_input(input1)
            .await
            .unwrap()
            .with_input(input2)
            .await
            .unwrap()
            .with_change_data(
                script!(Nop).unwrap(),
                inputs!(change.script_key_pk),
                change.script_key_id.clone(),
                change.commitment_mask_key_id.clone(),
                Covenant::default(),
                TariAddress::default(),
            )
            .with_fee_per_gram(fee_per_gram)
            .with_recipient_data(
                script.clone(),
                Default::default(),
                Default::default(),
                0.into(),
                MicroMinotari(2500),
                TariAddress::default(),
            )
            .await
            .unwrap();
        let result = builder.build().await.unwrap();
        // Peek inside and check the results
        if let SenderState::SingleRoundMessageReady(info) = result.into_state() {
            assert_eq!(info.metadata.lock_height, 1234, "Lock height");
            assert_eq!(info.metadata.fee, expected_fee, "Fee");
            assert_eq!(info.outputs.len(), 1, "There should be 1 outputs");
            assert!(info.change_output.is_some(), "There should be 1 change output");
            assert_eq!(info.inputs.len(), 2, "There should be 2 input");
        } else {
            panic!("There was a recipient, we should be ready to send a message");
        }
    }
}
