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

use std::fmt::{Debug, Error, Formatter};

use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    transaction::TxId,
    types::{Commitment, PrivateKey, PublicKey, Signature},
};
use tari_key_manager::key_manager_service::KeyManagerServiceError;
use tari_script::{ExecutionStack, TariScript};

use crate::{
    borsh::SerializedSize,
    consensus::ConsensusConstants,
    covenants::Covenant,
    transactions::{
        fee::Fee,
        key_manager::{TariKeyId, TransactionKeyManagerBranch, TransactionKeyManagerInterface},
        tari_amount::*,
        transaction_components::{
            OutputFeatures,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
            MAX_TRANSACTION_INPUTS,
            MAX_TRANSACTION_OUTPUTS,
        },
        transaction_protocol::{
            sender::{calculate_tx_id, OutputPair, RawTransactionInfo, SenderState, SenderTransactionProtocol},
            KernelFeatures,
            TransactionMetadata,
        },
    },
};

pub const LOG_TARGET: &str = "c::tx::tx_protocol::tx_initializer";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct ChangeDetails {
    change_spending_key_id: TariKeyId,
    change_script: TariScript,
    change_input_data: ExecutionStack,
    change_script_key_id: TariKeyId,
    change_covenant: Covenant,
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
    recipient_text_message: Option<String>,
    prevent_fee_gt_amount: bool,
    tx_id: Option<TxId>,
    kernel_features: KernelFeatures,
    burn_commitment: Option<Commitment>,
    fee: Fee,
    key_manager: KM,
}

pub struct BuildError<KM> {
    pub builder: SenderTransactionInitializer<KM>,
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
            recipient_text_message: None,
            prevent_fee_gt_amount: true,
            recipient: None,
            kernel_features: KernelFeatures::empty(),
            burn_commitment: None,
            tx_id: None,
            key_manager,
        }
    }

    /// Set the fee per weight for the transaction. See (Fee::calculate)[Struct.Fee.html#calculate] for how the
    /// absolute fee is calculated from the fee-per-gram value.
    pub fn with_fee_per_gram(&mut self, fee_per_gram: MicroMinotari) -> &mut Self {
        self.fee_per_gram = Some(fee_per_gram);
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
    ) -> Result<&mut Self, KeyManagerServiceError> {
        let (recipient_ephemeral_public_key_nonce, _) = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await?;
        let (recipient_sender_offset_key_id, _) = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await?;
        let recipient_details = RecipientDetails {
            recipient_output_features,
            recipient_script,
            recipient_sender_offset_key_id,
            recipient_covenant,
            recipient_minimum_value_promise,
            recipient_ephemeral_public_key_nonce,
            amount,
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
        let (nonce_id, _) = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let pair = OutputPair {
            output: input,
            kernel_nonce: nonce_id,
            sender_offset_key_id: None,
        };
        self.inputs.push(pair);
        Ok(self)
    }

    /// As the Sender adds an output to the transaction.
    pub async fn with_output(
        &mut self,
        output: WalletOutput,
        sender_offset_key_id: TariKeyId,
    ) -> Result<&mut Self, KeyManagerServiceError> {
        let (nonce_id, _) = self
            .key_manager
            .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
            .await?;
        let pair = OutputPair {
            output,
            kernel_nonce: nonce_id,
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
        change_spending_key_id: TariKeyId,
        change_covenant: Covenant,
    ) -> &mut Self {
        let details = ChangeDetails {
            change_spending_key_id,
            change_script,
            change_input_data,
            change_script_key_id,
            change_covenant,
        };
        self.change = Some(details);
        self
    }

    /// Provide a text message for receiver
    pub fn with_message(&mut self, message: String) -> &mut Self {
        self.recipient_text_message = Some(message);
        self
    }

    /// This will select the desired kernel features to be signed by the receiver
    pub fn with_kernel_features(&mut self, features: KernelFeatures) -> &mut Self {
        self.kernel_features = features;
        self
    }

    /// This will allow the receipient to sign the burn commitment
    pub fn with_burn_commitment(&mut self, commitment: Option<Commitment>) -> &mut Self {
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
                        let change_key_id = change_data.change_spending_key_id.clone();
                        let (sender_offset_key_id, sender_offset_public_key) = self
                            .key_manager
                            .get_next_key(&TransactionKeyManagerBranch::SenderOffset.get_branch_key())
                            .await
                            .map_err(|e| e.to_string())?;
                        let input_data = change_data.change_input_data.clone();

                        let covenant = self
                            .change
                            .as_ref()
                            .ok_or("Change covenant was not provided")?
                            .change_covenant
                            .clone();

                        let encrypted_data = self
                            .key_manager
                            .encrypt_data_for_recovery(&change_key_id, None, v.as_u64())
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
                                &sender_offset_key_id,
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
                            sender_offset_public_key.clone(),
                            metadata_sig,
                            0,
                            covenant,
                            encrypted_data,
                            minimum_value_promise,
                            &self.key_manager,
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                        Ok((
                            fee_without_change + change_fee,
                            v,
                            Some((change_wallet_output, sender_offset_key_id)),
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
            builder: self,
            message: msg.to_string(),
        })
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
    pub async fn build(mut self) -> Result<SenderTransactionProtocol, BuildError<KM>> {
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
        // Some checks on the fee
        if total_fee < Fee::MINIMUM_TRANSACTION_FEE {
            return self.build_err("Fee is less than the minimum");
        }

        let change_output_pair = match change_output {
            Some((output, sender_offset_key_id)) => {
                if self.sender_custom_outputs.len() >= MAX_TRANSACTION_OUTPUTS {
                    return self.build_err("Too many outputs in transaction");
                }
                let (nonce_id, _) = match self
                    .key_manager
                    .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
                    .await
                {
                    Ok(key_id) => key_id,
                    Err(e) => return self.build_err(&e.to_string()),
                };
                Some(OutputPair {
                    output,
                    kernel_nonce: nonce_id,
                    sender_offset_key_id: Some(sender_offset_key_id),
                })
            },
            None => None,
        };

        let spending_key = match self
            .key_manager
            .get_public_key_at_key_id(&self.inputs[0].output.spending_key_id)
            .await
        {
            Ok(key) => key,
            Err(e) => return self.build_err(&e.to_string()),
        };
        // we need some random data here, the public excess of the commitment is random.
        let tx_id = match self.tx_id {
            Some(id) => id,
            None => calculate_tx_id(&spending_key, 0),
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

        // cached data

        // Everything is here. Let's send some Minotari!
        let sender_info = RawTransactionInfo {
            tx_id,
            recipient_data: self.recipient,
            recipient_output: None,
            recipient_partial_kernel_excess: PublicKey::default(),
            recipient_partial_kernel_signature: Signature::default(),
            recipient_partial_kernel_offset: PrivateKey::default(),
            change_output: change_output_pair,
            total_sender_nonce: PublicKey::default(),
            total_sender_excess: PublicKey::default(),
            metadata: TransactionMetadata {
                fee: total_fee,
                lock_height: self.lock_height.unwrap(),
                kernel_features: self.kernel_features,
                burn_commitment: self.burn_commitment.clone(),
            },
            inputs: self.inputs,
            outputs: self.sender_custom_outputs,
            text_message: self.recipient_text_message.unwrap_or_default(),
        };

        let state = SenderState::Initializing(Box::new(sender_info));
        let state = state
            .initialize()
            .expect("It should be possible to call initialize from Initializing state");
        Ok(state.into())
    }
}

//----------------------------------------         Tests          ----------------------------------------------------//

#[cfg(test)]
mod test {
    use tari_script::{inputs, script};

    use crate::{
        covenants::Covenant,
        test_helpers::create_consensus_constants,
        transactions::{
            fee::Fee,
            key_manager::create_memory_db_key_manager,
            tari_amount::*,
            test_helpers::{create_test_input, create_wallet_output_with_data, TestParams, UtxoTestParams},
            transaction_components::{OutputFeatures, MAX_TRANSACTION_INPUTS},
            transaction_protocol::{sender::SenderState, transaction_initializer::SenderTransactionInitializer},
        },
    };

    /// One input, 2 outputs
    #[tokio::test]
    async fn no_receivers() -> std::io::Result<()> {
        // Create some inputs
        let key_manager = create_memory_db_key_manager();
        let p = TestParams::new(&key_manager).await;
        // Start the builder
        let builder = SenderTransactionInitializer::new(&create_consensus_constants(0), key_manager.clone());
        let err = builder.build().await.unwrap_err();
        let script = script!(Nop);
        // We should have a bunch of fields missing still, but we can recover and continue
        assert_eq!(err.message, "Missing Lock Height,Missing Fee per gram");

        let mut builder = err.builder;
        builder.with_lock_height(100);
        builder
            .with_output(
                create_wallet_output_with_data(
                    script.clone(),
                    OutputFeatures::default(),
                    &p,
                    MicroMinotari(100),
                    &key_manager,
                )
                .await
                .unwrap(),
                p.sender_offset_key_id.clone(),
            )
            .await
            .unwrap();
        let input = TestParams::new(&key_manager)
            .await
            .create_input(
                UtxoTestParams {
                    value: MicroMinotari(5_000),
                    ..Default::default()
                },
                &key_manager,
            )
            .await;
        builder.with_input(input).await.unwrap();
        builder.with_fee_per_gram(MicroMinotari(20));
        let expected_fee = builder.fee().calculate(
            MicroMinotari(20),
            1,
            1,
            2,
            p.get_size_for_default_features_and_scripts(2)?,
        );
        // We needed a change input, so this should fail
        let err = builder.build().await.unwrap_err();
        assert_eq!(err.message, "Change data was not provided");
        // Ok, give them a change output
        let mut builder = err.builder;
        let change = TestParams::new(&key_manager).await;
        builder.with_change_data(
            script!(Nop),
            Default::default(),
            change.script_key_id.clone(),
            change.spend_key_id.clone(),
            Covenant::default(),
        );
        let result = builder.build().await.unwrap();
        // Peek inside and check the results
        if let SenderState::Finalizing(info) = result.into_state() {
            assert_eq!(info.metadata.lock_height, 100, "Lock height");
            assert_eq!(info.metadata.fee, expected_fee, "Fee");
            assert_eq!(info.outputs.len(), 1, "There should be 1 output1");
            assert!(info.change_output.is_some(), "There should be 1 change output1");
            assert_eq!(info.inputs.len(), 1, "There should be 1 input");
        } else {
            panic!("There were no recipients, so we should be finalizing");
        }

        Ok(())
    }

    /// One output, one input
    #[tokio::test]
    async fn no_change_or_receivers() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager();
        let p = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(5000), 0, &key_manager, vec![]).await;
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
            script!(Nop),
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
        let key_manager = create_memory_db_key_manager();
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
        let key_manager = create_memory_db_key_manager();
        let p = TestParams::new(&key_manager).await;

        let output = create_wallet_output_with_data(
            script!(Nop),
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
        let input_base = create_test_input(MicroMinotari(50), 0, &key_manager, vec![]).await;
        for _ in 0..=MAX_TRANSACTION_INPUTS {
            builder.with_input(input_base.clone()).await.unwrap();
        }
        let err = builder.build().await.unwrap_err();
        assert_eq!(err.message, "Too many inputs in transaction");
    }

    #[tokio::test]
    async fn fee_too_low() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager();
        let p = TestParams::new(&key_manager).await;
        let tx_fee = p.fee().calculate(
            MicroMinotari(1),
            1,
            1,
            1,
            p.get_size_for_default_features_and_scripts(1)
                .expect("Failed to borsh serialized size"),
        );
        let input = create_test_input(500 * uT + tx_fee, 0, &key_manager, vec![]).await;
        let script = script!(Nop);
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
                script!(Nop),
                inputs!(change.script_key_pk),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_fee_per_gram(MicroMinotari(1))
            .with_recipient_data(
                script,
                Default::default(),
                Default::default(),
                0.into(),
                MicroMinotari(500),
            )
            .await
            .unwrap();
        let err = builder.build().await.unwrap_err();
        assert_eq!(err.message, "Fee is less than the minimum");
    }

    #[tokio::test]
    async fn not_enough_funds() {
        // Create some inputs
        let key_manager = create_memory_db_key_manager();
        let p = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroMinotari(400), 0, &key_manager, vec![]).await;
        let script = script!(Nop);
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
                script!(Nop),
                inputs!(change.script_key_pk),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_fee_per_gram(MicroMinotari(1))
            .with_recipient_data(
                script.clone(),
                Default::default(),
                Default::default(),
                0.into(),
                MicroMinotari::zero(),
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
        let key_manager = create_memory_db_key_manager();
        let p = TestParams::new(&key_manager).await;
        let input1 = create_test_input(MicroMinotari(2000), 0, &key_manager, vec![]).await;
        let input2 = create_test_input(MicroMinotari(3000), 0, &key_manager, vec![]).await;
        let fee_per_gram = MicroMinotari(6);

        let script = script!(Nop);
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
                script!(Nop),
                inputs!(change.script_key_pk),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_fee_per_gram(fee_per_gram)
            .with_recipient_data(
                script.clone(),
                Default::default(),
                Default::default(),
                0.into(),
                MicroMinotari(2500),
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
