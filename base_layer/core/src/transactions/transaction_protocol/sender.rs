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

use std::fmt;

use serde::{Deserialize, Serialize};
use tari_common_types::{
    transaction::TxId,
    types::{PrivateKey, PublicKey, Signature},
};
use tari_crypto::{ristretto::pedersen::PedersenCommitment, tari_utilities::ByteArray};
pub use tari_key_manager::key_manager_service::KeyId;
use tari_script::TariScript;

use super::CalculateTxIdTransactionProtocolHasherBlake256;
use crate::{
    consensus::ConsensusConstants,
    covenants::Covenant,
    transactions::{
        fee::Fee,
        key_manager::{TariKeyId, TransactionKeyManagerInterface, TxoStage},
        tari_amount::*,
        transaction_components::{
            KernelBuilder,
            OutputFeatures,
            Transaction,
            TransactionBuilder,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            WalletOutput,
            MAX_TRANSACTION_INPUTS,
            MAX_TRANSACTION_OUTPUTS,
        },
        transaction_protocol::{
            recipient::RecipientSignedMessage,
            transaction_initializer::{RecipientDetails, SenderTransactionInitializer},
            TransactionMetadata,
            TransactionProtocolError as TPE,
        },
    },
};

//----------------------------------------   Local Data types     ----------------------------------------------------//
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct OutputPair {
    pub output: WalletOutput,
    pub kernel_nonce: TariKeyId,
    pub sender_offset_key_id: Option<TariKeyId>,
}

/// This struct contains all the information that a transaction initiator (the sender) will manage throughout the
/// Transaction construction process.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct RawTransactionInfo {
    pub tx_id: TxId,
    // the recipient's data
    pub recipient_data: Option<RecipientDetails>,
    pub recipient_output: Option<TransactionOutput>,
    pub recipient_partial_kernel_excess: PublicKey,
    pub recipient_partial_kernel_signature: Signature,
    pub recipient_partial_kernel_offset: PrivateKey,
    // The sender's portion of the public commitment nonce
    pub change_output: Option<OutputPair>,
    pub inputs: Vec<OutputPair>,
    pub outputs: Vec<OutputPair>,
    // cached data
    // this is calculated when sender sends single round message to receiver
    pub total_sender_excess: PublicKey,
    // this is calculated when sender sends single round message to receiver
    pub total_sender_nonce: PublicKey,

    pub metadata: TransactionMetadata,
    pub text_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SingleRoundSenderData {
    /// The transaction id for the recipient
    pub tx_id: TxId,
    /// The amount, in µT, being sent to the recipient
    pub amount: MicroTari,
    /// The offset public excess for this transaction
    pub public_excess: PublicKey,
    /// The sender's public nonce
    pub public_nonce: PublicKey,
    /// The transaction metadata
    pub metadata: TransactionMetadata,
    /// Plain text message to receiver
    pub message: String,
    /// The output's features
    pub features: OutputFeatures,
    /// Script
    pub script: TariScript,
    /// Script offset public key
    pub sender_offset_public_key: PublicKey,
    /// The sender's ephemeral nonce
    pub ephemeral_public_nonce: PublicKey,
    /// Covenant
    pub covenant: Covenant,
    /// The minimum value of the commitment that is proven by the range proof
    pub minimum_value_promise: MicroTari,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionSenderMessage {
    None,
    Single(Box<SingleRoundSenderData>),
    Multiple,
}

impl TransactionSenderMessage {
    pub fn new_single_round_message(single_round_data: SingleRoundSenderData) -> Self {
        Self::Single(Box::new(single_round_data))
    }

    pub fn single(&self) -> Option<&SingleRoundSenderData> {
        match self {
            TransactionSenderMessage::Single(m) => Some(m),
            _ => None,
        }
    }
}

//----------------------------------------  Sender State Protocol ----------------------------------------------------//
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SenderTransactionProtocol {
    state: SenderState,
}

impl SenderTransactionProtocol {
    // pub fn from_state(state: SenderState) -> Self {
    //     SenderTransactionProtocol { state }
    // }

    /// Begin constructing a new transaction. All the up-front data is collected via the
    /// `SenderTransactionInitializer` builder function
    pub fn builder<KM: TransactionKeyManagerInterface>(
        consensus_constants: ConsensusConstants,
        key_manager: KM,
    ) -> SenderTransactionInitializer<KM> {
        SenderTransactionInitializer::new(&consensus_constants, key_manager)
    }

    /// Convenience method to check whether we're receiving recipient data
    pub fn is_collecting_single_signature(&self) -> bool {
        matches!(&self.state, SenderState::CollectingSingleSignature(_))
    }

    /// Convenience method to check whether we're ready to send a message to a single recipient
    pub fn is_single_round_message_ready(&self) -> bool {
        matches!(&self.state, SenderState::SingleRoundMessageReady(_))
    }

    /// Method to determine if we are in the SenderState::Finalizing state
    pub fn is_finalizing(&self) -> bool {
        matches!(&self.state, SenderState::Finalizing(_))
    }

    /// Method to determine if we are in the SenderState::FinalizedTransaction state
    pub fn is_finalized(&self) -> bool {
        matches!(&self.state, SenderState::FinalizedTransaction(_))
    }

    pub fn get_transaction(&self) -> Result<&Transaction, TPE> {
        match &self.state {
            SenderState::FinalizedTransaction(tx) => Ok(tx),
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Returns the finalized transaction if the protocol is in the Finalised state and consumes the protocol object.
    /// Otherwise it returns an `InvalidStateError`. To keep the object and return a reference to the transaction, see
    /// [get_transaction].
    pub fn take_transaction(self) -> Result<Transaction, TPE> {
        match self.state {
            SenderState::FinalizedTransaction(tx) => Ok(tx),
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Method to determine if the transaction protocol has failed
    pub fn is_failed(&self) -> bool {
        matches!(&self.state, SenderState::Failed(_))
    }

    /// Method to return the error behind a failure, if one has occurred
    pub fn failure_reason(&self) -> Option<TPE> {
        match &self.state {
            SenderState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// Method to check if the provided tx_id matches this transaction
    pub fn check_tx_id(&self, tx_id: TxId) -> bool {
        match &self.state {
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => info.tx_id == tx_id,
            _ => false,
        }
    }

    pub fn get_tx_id(&self) -> Result<TxId, TPE> {
        match &self.state {
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.tx_id),
            _ => Err(TPE::InvalidStateError),
        }
    }

    pub fn get_amount_to_recipient(&self) -> Result<MicroTari, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info
                .recipient_data
                .as_ref()
                .map(|data| data.amount)
                .unwrap_or(MicroTari::zero())),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the total value of outputs being sent to yourself in the transaction including the
    /// change
    pub fn get_amount_to_self(&self) -> Result<MicroTari, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => {
                let mut amount = info
                    .change_output
                    .as_ref()
                    .map(|output| output.output.value)
                    .unwrap_or(MicroTari::zero());
                for output in &info.outputs {
                    amount += output.output.value
                }
                Ok(amount)
            },
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the value of the change transaction
    pub fn get_change_amount(&self) -> Result<MicroTari, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info
                .change_output
                .as_ref()
                .map(|output| output.output.value)
                .unwrap_or(MicroTari::zero())),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the change output
    pub fn get_change_output(&self) -> Result<Option<WalletOutput>, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => {
                Ok(info.change_output.as_ref().map(|output| output.output.clone()))
            },
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the script offset private keys for a single recipient
    pub fn get_recipient_sender_offset_private_key(&self) -> Result<Option<TariKeyId>, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok({
                info.recipient_data
                    .as_ref()
                    .map(|data| data.recipient_sender_offset_key_id.clone())
            }),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the value of the fee of this transaction
    pub fn get_fee_amount(&self) -> Result<MicroTari, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.metadata.fee),
            SenderState::FinalizedTransaction(info) => {
                Ok(info.body.kernels().first().ok_or(TPE::InvalidStateError)?.fee)
            },
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// Build the sender's message for the single-round protocol (one recipient) and move to next State
    pub async fn build_single_round_message<KM: TransactionKeyManagerInterface>(
        &mut self,
        key_manager: &KM,
    ) -> Result<SingleRoundSenderData, TPE> {
        if !matches!(&self.state, SenderState::SingleRoundMessageReady(_)) {
            return Err(TPE::InvalidStateError);
        };
        let result = self.get_single_round_message(key_manager).await?;
        if let SenderState::SingleRoundMessageReady(info) = &self.state {
            self.state = SenderState::CollectingSingleSignature(info.clone());
        }
        Ok(result)
    }

    /// Revert the sender state back to 'SingleRoundMessageReady', used if transactions gets queued
    pub fn revert_sender_state_to_single_round_message_ready(&mut self) -> Result<(), TPE> {
        match &self.state {
            SenderState::CollectingSingleSignature(info) => {
                self.state = SenderState::SingleRoundMessageReady(info.clone());
                Ok(())
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Return the single round sender message
    pub async fn get_single_round_message<KM: TransactionKeyManagerInterface>(
        &mut self,
        key_manager: &KM,
    ) -> Result<SingleRoundSenderData, TPE> {
        match &mut self.state {
            SenderState::SingleRoundMessageReady(info) | SenderState::CollectingSingleSignature(info) => {
                let recipient_data = info
                    .recipient_data
                    .as_ref()
                    .ok_or_else(|| TPE::IncompleteStateError("Missing recipient data".to_string()))?;
                let recipient_output_features = recipient_data.recipient_output_features.clone();
                let recipient_script = recipient_data.recipient_script.clone();
                let recipient_script_offset_secret_key_id = recipient_data.recipient_sender_offset_key_id.clone();
                let recipient_covenant = recipient_data.recipient_covenant.clone();
                let recipient_minimum_value_promise = recipient_data.recipient_minimum_value_promise;
                let amount = recipient_data.amount;
                let ephemeral_public_key_nonce = recipient_data.recipient_ephemeral_public_key_nonce.clone();

                let (public_nonce, public_excess) =
                    SenderTransactionProtocol::calculate_total_nonce_and_total_public_excess(info, key_manager).await?;
                let sender_offset_public_key = key_manager
                    .get_public_key_at_key_id(&recipient_script_offset_secret_key_id)
                    .await?;
                // we update this as we send this to what we sent.
                info.total_sender_excess = public_excess.clone();
                info.total_sender_nonce = public_nonce.clone();

                let ephemeral_public_nonce = key_manager
                    .get_public_key_at_key_id(&ephemeral_public_key_nonce)
                    .await?;

                Ok(SingleRoundSenderData {
                    tx_id: info.tx_id,
                    amount,
                    public_nonce,
                    public_excess,
                    metadata: info.metadata.clone(),
                    message: info.text_message.clone(),
                    features: recipient_output_features,
                    script: recipient_script,
                    sender_offset_public_key,
                    ephemeral_public_nonce,
                    covenant: recipient_covenant,
                    minimum_value_promise: recipient_minimum_value_promise,
                })
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    async fn calculate_total_nonce_and_total_public_excess<KM: TransactionKeyManagerInterface>(
        info: &RawTransactionInfo,
        key_manager: &KM,
    ) -> Result<(PublicKey, PublicKey), TPE> {
        // lets calculate the total sender kernel signature nonce
        let mut public_nonce = PublicKey::default();
        // lets calculate the total sender kernel exess
        let mut public_excess = PublicKey::default();
        for input in &info.inputs {
            public_nonce = public_nonce + key_manager.get_public_key_at_key_id(&input.kernel_nonce).await?;
            public_excess = public_excess -
                key_manager
                    .get_txo_kernel_signature_excess_with_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await?;
        }
        for output in &info.outputs {
            public_nonce = public_nonce + key_manager.get_public_key_at_key_id(&output.kernel_nonce).await?;
            public_excess = public_excess +
                key_manager
                    .get_txo_kernel_signature_excess_with_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await?;
        }

        if let Some(change) = &info.change_output {
            public_nonce = public_nonce + key_manager.get_public_key_at_key_id(&change.kernel_nonce).await?;
            public_excess = public_excess +
                key_manager
                    .get_txo_kernel_signature_excess_with_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await?;
        }
        Ok((public_nonce, public_excess))
    }

    /// Add the signed transaction from the recipient and move to the next state
    pub async fn add_single_recipient_info<KM: TransactionKeyManagerInterface>(
        &mut self,
        rec: RecipientSignedMessage,
        key_manager: &KM,
    ) -> Result<(), TPE> {
        match &mut self.state {
            SenderState::CollectingSingleSignature(info) => {
                // Consolidate transaction info
                let mut received_output = rec.output.clone();
                if received_output.verify_metadata_signature().is_err() {
                    // we need to make sure we use our values here and not the received values.
                    let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
                        &received_output.version,
                        &received_output.script, /* receiver chooses script here, can change fee per gram see issue: https://github.com/tari-project/tari/issues/5430 */
                        &info
                            .recipient_data
                            .as_ref()
                            .ok_or_else(|| {
                                TPE::IncompleteStateError("Missing data `recipient_output_features`".to_string())
                            })?
                            .recipient_output_features,
                        &info
                            .recipient_data
                            .as_ref()
                            .ok_or_else(|| TPE::IncompleteStateError("Missing data `recipient_covenant`".to_string()))?
                            .recipient_covenant,
                        &received_output.encrypted_data,
                        info.recipient_data
                            .as_ref()
                            .ok_or_else(|| {
                                TPE::IncompleteStateError("Missing data 'recipient_minimum_value_promise'".to_string())
                            })?
                            .recipient_minimum_value_promise,
                    );
                    let ephemeral_public_key_nonce = info
                        .recipient_data
                        .as_ref()
                        .ok_or_else(|| {
                            TPE::IncompleteStateError("Missing data `recipient_ephemeral_public_key_nonce`".to_string())
                        })?
                        .recipient_ephemeral_public_key_nonce
                        .clone();
                    let recipient_sender_offset_key_id = info
                        .recipient_data
                        .as_ref()
                        .ok_or_else(|| {
                            TPE::IncompleteStateError("Missing data `recipient_sender_offset_key_id`".to_string())
                        })?
                        .recipient_sender_offset_key_id
                        .clone();
                    let sender_metadata_signature = key_manager
                        .get_sender_partial_metadata_signature(
                            &ephemeral_public_key_nonce,
                            &recipient_sender_offset_key_id,
                            &received_output.commitment,
                            received_output.metadata_signature.ephemeral_commitment(),
                            &received_output.version,
                            &metadata_message,
                        )
                        .await?;
                    received_output.metadata_signature =
                        &received_output.metadata_signature + &sender_metadata_signature;
                    info.recipient_output = Some(received_output.clone());
                }
                info.recipient_partial_kernel_excess = rec.public_spend_key;
                info.recipient_partial_kernel_signature = rec.partial_signature;
                info.recipient_partial_kernel_offset = rec.offset;
                if info.metadata.kernel_features.is_burned() {
                    info.metadata.burn_commitment = Some(received_output.commitment);
                };

                self.state = SenderState::Finalizing(info.clone());
                Ok(())
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Attempts to build the final transaction.
    async fn build_transaction<KM: TransactionKeyManagerInterface>(
        info: &RawTransactionInfo,
        key_manager: &KM,
    ) -> Result<Transaction, TPE> {
        let mut tx_builder = TransactionBuilder::new();
        let (total_public_nonce, total_public_excess) = if info.recipient_data.is_none() {
            // we dont have a recipient and thus we have not yet calculated the sender_nonce and sender_offset_excess
            SenderTransactionProtocol::calculate_total_nonce_and_total_public_excess(info, key_manager).await?
        } else {
            let total_public_nonce =
                &info.total_sender_nonce + info.recipient_partial_kernel_signature.get_public_nonce();
            let total_public_excess = &info.total_sender_excess + &info.recipient_partial_kernel_excess;
            (total_public_nonce, total_public_excess)
        };

        let mut offset = info.recipient_partial_kernel_offset.clone();
        let mut signature = info.recipient_partial_kernel_signature.clone();
        let mut script_keys = Vec::new();
        let mut sender_offset_keys = Vec::new();

        let kernel_message = TransactionKernel::build_kernel_signature_message(
            &TransactionKernelVersion::get_current_version(),
            info.metadata.fee,
            info.metadata.lock_height,
            &info.metadata.kernel_features,
            &info.metadata.burn_commitment,
        );

        for input in &info.inputs {
            tx_builder.add_input(input.output.as_transaction_input(key_manager).await?);
            signature = &signature +
                &key_manager
                    .get_txo_kernel_signature(
                        &input.output.spending_key_id,
                        &input.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &TransactionKernelVersion::get_current_version(),
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Input,
                    )
                    .await?;
            offset = offset -
                &key_manager
                    .get_txo_private_kernel_offset(&input.output.spending_key_id, &input.kernel_nonce)
                    .await?;
            script_keys.push(input.output.script_key_id.clone());
            // let sig = key_manager
            //     .get_partial_kernel_signature(
            //         &input.output.spending_key_id,
            //         &input.kernel_nonce,
            //         &total_public_nonce,
            //         &total_public_excess,
            //         &TransactionKernelVersion::get_current_version(),
            //         &kernel_message,
            //         &info.metadata.kernel_features,
            //         TxoStage::Input,
            //     )
            //     .await?;
            // let excess = PublicKey::default() -
            // key_manager.get_partial_kernel_signature_excess_with_offset(&input.output.spending_key_id,&input.
            // kernel_nonce ).await.unwrap(); // let excess =
            // key_manager.get_partial_kernel_signature_excess_with_offset(&input.output.spending_key_id,&input.
            // kernel_nonce ).await.unwrap(); let sig_challenge =
            // TransactionKernel::finalize_kernel_signature_challenge(&TransactionKernelVersion::get_current_version(),
            // &total_public_nonce, &total_public_excess, &kernel_message); assert!(sig.verify(&excess,
            // &PrivateKey::from_bytes(&sig_challenge).unwrap()));
            //
            // assert!(false);
        }

        for output in &info.outputs {
            tx_builder.add_output(output.output.as_transaction_output(key_manager).await?);
            signature = &signature +
                &key_manager
                    .get_txo_kernel_signature(
                        &output.output.spending_key_id,
                        &output.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &TransactionKernelVersion::get_current_version(),
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Output,
                    )
                    .await?;
            offset = offset +
                &key_manager
                    .get_txo_private_kernel_offset(&output.output.spending_key_id, &output.kernel_nonce)
                    .await?;
            let sender_offset_key_id = output
                .sender_offset_key_id
                .clone()
                .ok_or_else(|| TPE::IncompleteStateError("Missing sender offset key id".to_string()))?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        if let Some(recipient_data) = &info.recipient_data {
            sender_offset_keys.push(recipient_data.recipient_sender_offset_key_id.clone());
        }
        if let Some(change) = &info.change_output {
            tx_builder.add_output(change.output.as_transaction_output(key_manager).await?);
            signature = &signature +
                &key_manager
                    .get_txo_kernel_signature(
                        &change.output.spending_key_id,
                        &change.kernel_nonce,
                        &total_public_nonce,
                        &total_public_excess,
                        &TransactionKernelVersion::get_current_version(),
                        &kernel_message,
                        &info.metadata.kernel_features,
                        TxoStage::Output,
                    )
                    .await?;
            offset = offset +
                &key_manager
                    .get_txo_private_kernel_offset(&change.output.spending_key_id, &change.kernel_nonce)
                    .await?;
            let sender_offset_key_id = change
                .sender_offset_key_id
                .clone()
                .ok_or_else(|| TPE::IncompleteStateError("Missing sender offset key id".to_string()))?;
            sender_offset_keys.push(sender_offset_key_id);
        }

        if let Some(received_output) = &info.recipient_output {
            tx_builder.add_output(received_output.clone());
        }
        let script_offset = key_manager.get_script_offset(&script_keys, &sender_offset_keys).await?;

        tx_builder.add_offset(offset);
        tx_builder.add_script_offset(script_offset);
        let excess = PedersenCommitment::from_public_key(&total_public_excess);

        let kernel = KernelBuilder::new()
            .with_fee(info.metadata.fee)
            .with_features(info.metadata.kernel_features)
            .with_lock_height(info.metadata.lock_height)
            .with_burn_commitment(info.metadata.burn_commitment.clone())
            .with_excess(&excess)
            .with_signature(signature)
            .build()?;
        tx_builder.with_kernel(kernel);
        tx_builder.build().map_err(TPE::from)
    }

    /// Performs sanity checks on the collected transaction pieces prior to building the final Transaction instance
    fn validate(&self) -> Result<(), TPE> {
        if let SenderState::Finalizing(info) = &self.state {
            let fee = info.metadata.fee;
            // The fee must be greater than MIN_FEE to prevent spam attacks
            if fee < Fee::MINIMUM_TRANSACTION_FEE {
                return Err(TPE::ValidationError("Fee is less than the minimum".into()));
            }
            // Prevent overflow attacks by imposing sane limits on some key parameters
            if info.inputs.len() > MAX_TRANSACTION_INPUTS {
                return Err(TPE::ValidationError("Too many inputs in transaction".into()));
            }
            if info.outputs.len() > MAX_TRANSACTION_OUTPUTS {
                return Err(TPE::ValidationError("Too many outputs in transaction".into()));
            }
            if info.inputs.is_empty() {
                return Err(TPE::ValidationError("A transaction cannot have zero inputs".into()));
            }
            Ok(())
        } else {
            Err(TPE::InvalidStateError)
        }
    }

    /// Try and finalise the transaction. If the current state is Finalizing, the result will be whether the
    /// transaction was valid or not. If the result is false, the transaction will be in a Failed state. Calling
    /// finalize while in any other state will result in an error.
    ///
    /// First we validate against internal sanity checks, then try build the transaction, and then
    /// formally validate the transaction terms (no inflation, signature matches etc). If any step fails,
    /// the transaction protocol moves to Failed state and we are done; you can't rescue the situation. The function
    /// returns `Ok(false)` in this instance.
    pub async fn finalize<KM: TransactionKeyManagerInterface>(&mut self, key_manager: &KM) -> Result<(), TPE> {
        match &self.state {
            SenderState::Finalizing(info) => {
                if let Err(e) = self.validate() {
                    self.state = SenderState::Failed(e.clone());
                    return Err(e);
                }
                match Self::build_transaction(info, key_manager).await {
                    Ok(transaction) => {
                        self.state = SenderState::FinalizedTransaction(transaction);
                        Ok(())
                    },
                    Err(e) => {
                        self.state = SenderState::Failed(e.clone());
                        Err(e)
                    },
                }
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Create an empty SenderTransactionProtocol that can be used as a placeholder in data structures that do not
    /// require a well formed version
    pub fn new_placeholder() -> Self {
        SenderTransactionProtocol {
            state: SenderState::Failed(TPE::IncompleteStateError("This is a placeholder protocol".to_string())),
        }
    }

    #[cfg(test)]
    pub(super) fn into_state(self) -> SenderState {
        self.state
    }
}

impl From<SenderState> for SenderTransactionProtocol {
    fn from(state: SenderState) -> Self {
        Self { state }
    }
}

impl fmt::Display for SenderTransactionProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.state)
    }
}

pub fn calculate_tx_id(pub_nonce: &PublicKey, index: usize) -> TxId {
    let hash = CalculateTxIdTransactionProtocolHasherBlake256::new()
        .chain(pub_nonce.as_bytes())
        .chain(index.to_le_bytes())
        .finalize();
    let mut bytes: [u8; 8] = [0u8; 8];
    bytes.copy_from_slice(&hash.as_ref()[..8]);
    u64::from_le_bytes(bytes).into()
}

//----------------------------------------      Sender State      ----------------------------------------------------//

/// This enum contains all the states of the Sender state machine
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) enum SenderState {
    /// Transitional state that kicks of the relevant transaction protocol
    Initializing(Box<RawTransactionInfo>),
    /// The message for the recipient in a single-round scheme is ready
    SingleRoundMessageReady(Box<RawTransactionInfo>),
    /// Waiting for the signed transaction data in the single-round protocol
    CollectingSingleSignature(Box<RawTransactionInfo>),
    /// The final transaction state is being validated - it will automatically transition to Failed or Finalized from
    /// here
    Finalizing(Box<RawTransactionInfo>),
    /// The final transaction is ready to be broadcast
    FinalizedTransaction(Transaction),
    /// An unrecoverable failure has occurred and the transaction must be abandoned
    Failed(TPE),
}

impl SenderState {
    /// Puts the Sender FSM into the appropriate initial state, based on the number of recipients. Don't call this
    /// function directly. It is called by the `TransactionInitializer` builder
    pub(super) fn initialize(self) -> Result<SenderState, TPE> {
        match self {
            SenderState::Initializing(info) => match info.recipient_data.is_some() {
                false => Ok(SenderState::Finalizing(info)),
                true => Ok(SenderState::SingleRoundMessageReady(info)),
            },
            _ => Err(TPE::InvalidTransitionError),
        }
    }
}

impl fmt::Display for SenderState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use SenderState::*;
        match self {
            Initializing(info) => write!(
                f,
                "Initializing({} input(s), {} output(s))",
                info.inputs.len(),
                info.outputs.len()
            ),
            SingleRoundMessageReady(info) => write!(
                f,
                "SingleRoundMessageReady({} input(s), {} output(s))",
                info.inputs.len(),
                info.outputs.len()
            ),
            CollectingSingleSignature(info) => write!(
                f,
                "CollectingSingleSignature({} input(s), {} output(s))",
                info.inputs.len(),
                info.outputs.len()
            ),
            Finalizing(info) => write!(
                f,
                "Finalizing({} input(s), {} output(s))",
                info.inputs.len(),
                info.outputs.len()
            ),
            FinalizedTransaction(txn) => write!(
                f,
                "FinalizedTransaction({} input(s), {} output(s))",
                txn.body.inputs().len(),
                txn.body.outputs().len()
            ),
            Failed(err) => write!(f, "Failed({:?})", err),
        }
    }
}

//----------------------------------------         Tests          ----------------------------------------------------//

#[cfg(test)]
mod test {
    use tari_common_types::types::PrivateKey;
    use tari_crypto::signatures::CommitmentAndPublicKeySignature;
    use tari_key_manager::key_manager_service::KeyManagerInterface;
    use tari_script::{inputs, script, ExecutionStack, TariScript};
    use tari_utilities::hex::Hex;

    use super::SenderState;
    use crate::{
        covenants::Covenant,
        test_helpers::{
            create_consensus_constants,
            create_consensus_rules,
            create_test_core_key_manager_with_memory_db,
            create_test_core_key_manager_with_memory_db_with_range_proof_size,
        },
        transactions::{
            crypto_factories::CryptoFactories,
            key_manager::{TransactionKeyManagerBranch, TransactionKeyManagerInterface},
            tari_amount::*,
            test_helpers::{create_key_manager_output_with_data, create_test_input, TestParams},
            transaction_components::{
                EncryptedData,
                OutputFeatures,
                TransactionOutput,
                TransactionOutputVersion,
                WalletOutput,
            },
            transaction_protocol::{
                sender::{SenderTransactionProtocol, TransactionSenderMessage},
                single_receiver::SingleReceiverTransactionProtocol,
                TransactionError,
                TransactionProtocolError,
            },
        },
        validation::transaction::TransactionInternalConsistencyValidator,
    };

    #[test]
    fn test_not_single() {
        assert_eq!(TransactionSenderMessage::None.single(), None);
        assert_eq!(TransactionSenderMessage::Multiple.single(), None);
    }

    #[tokio::test]
    async fn test_errors() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let stp = SenderTransactionProtocol {
            state: SenderState::Failed(TransactionProtocolError::InvalidStateError),
        };
        assert_eq!(stp.get_transaction(), Err(TransactionProtocolError::InvalidStateError));
        assert_eq!(
            stp.clone().take_transaction(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert!(!stp.check_tx_id(0u64.into()));
        assert_eq!(stp.get_tx_id(), Err(TransactionProtocolError::InvalidStateError));
        assert_eq!(
            stp.get_amount_to_self(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_change_amount(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_change_output(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_recipient_sender_offset_private_key(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(stp.get_fee_amount(), Err(TransactionProtocolError::InvalidStateError));
        assert_eq!(
            stp.clone().build_single_round_message(&key_manager).await,
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.clone().revert_sender_state_to_single_round_message_ready(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.clone().get_single_round_message(&key_manager).await,
            Err(TransactionProtocolError::InvalidStateError)
        );
    }

    #[tokio::test]
    async fn test_metadata_signature_finalize() {
        // Defaults
        let key_manager = create_test_core_key_manager_with_memory_db();

        // Sender data
        let (ephemeral_pubkey_id, ephemeral_pubkey) = key_manager
            .get_next_key(TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await
            .unwrap();
        let value = 1000u64;
        let (sender_offset_key_id, sender_offset_public_key) = key_manager
            .get_next_key(TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await
            .unwrap();
        let txo_version = TransactionOutputVersion::get_current_version();

        // Shared contract data
        let script = Default::default();
        let output_features = Default::default();

        // Receiver data
        let (spending_key_id, _, _script_key_id, _) = key_manager.get_next_spend_and_script_key_ids().await.unwrap();
        let commitment = key_manager
            .get_commitment(&spending_key_id, &PrivateKey::from(value))
            .await
            .unwrap();
        let minimum_value_promise = MicroTari::zero();
        let proof = key_manager
            .construct_range_proof(&spending_key_id, value, minimum_value_promise.into())
            .await
            .unwrap();
        let covenant = Covenant::default();

        // Encrypted value
        let encrypted_data = key_manager
            .encrypt_data_for_recovery(&spending_key_id, None, value)
            .await
            .unwrap();

        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &txo_version,
            &script,
            &output_features,
            &covenant,
            &encrypted_data,
            minimum_value_promise,
        );
        let partial_metadata_signature = key_manager
            .get_receiver_partial_metadata_signature(
                &spending_key_id,
                &value.into(),
                &sender_offset_public_key,
                &ephemeral_pubkey,
                &txo_version,
                &metadata_message,
                output_features.range_proof_type,
            )
            .await
            .unwrap();

        let mut output = TransactionOutput::new_current_version(
            output_features,
            commitment,
            Some(proof),
            script.clone(),
            sender_offset_public_key,
            partial_metadata_signature.clone(),
            covenant.clone(),
            encrypted_data,
            minimum_value_promise,
        );
        assert!(output.verify_metadata_signature().is_err());

        // Sender finalize transaction output
        let partial_sender_metadata_signature = key_manager
            .get_sender_partial_metadata_signature(
                &ephemeral_pubkey_id,
                &sender_offset_key_id,
                &output.commitment,
                partial_metadata_signature.ephemeral_commitment(),
                &txo_version,
                &metadata_message,
            )
            .await
            .unwrap();
        output.metadata_signature = &partial_metadata_signature + &partial_sender_metadata_signature;
        assert!(output.verify_metadata_signature().is_ok());
    }

    #[tokio::test]
    async fn zero_recipients() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let p1 = TestParams::new(&key_manager).await;
        let p2 = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroTari(1200), 0, &key_manager).await;
        let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());
        let script = TariScript::default();
        let output_features = OutputFeatures::default();
        let change = TestParams::new(&key_manager).await;
        let script_key = key_manager
            .get_public_key_at_key_id(&change.script_key_id)
            .await
            .unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(2))
            .with_change_data(
                script!(Nop),
                inputs!(script_key),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_input(input)
            .await
            .unwrap()
            .with_output(
                create_key_manager_output_with_data(
                    script.clone(),
                    output_features.clone(),
                    &p1,
                    MicroTari(500),
                    &key_manager,
                )
                .await
                .unwrap(),
                p1.sender_offset_key_id.clone(),
            )
            .await
            .unwrap()
            .with_output(
                create_key_manager_output_with_data(script, output_features, &p2, MicroTari(400), &key_manager)
                    .await
                    .unwrap(),
                p2.sender_offset_key_id.clone(),
            )
            .await
            .unwrap();
        let mut sender = builder.build().await.unwrap();
        assert!(!sender.is_failed());
        assert!(sender.is_finalizing());
        match sender.finalize(&key_manager).await {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        }
        let tx = sender.get_transaction().unwrap();
        // let change_offset = key_manager.getoff
        // assert_eq!(tx.offset, p1.offset + p2.offset);
        let rules = create_consensus_rules();
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn single_recipient_no_change() {
        let rules = create_consensus_rules();
        let factories = CryptoFactories::default();
        // Alice's parameters
        let key_manager = create_test_core_key_manager_with_memory_db();
        let a_change_key = TestParams::new(&key_manager).await;
        // Bob's parameters
        let bob_key = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroTari(1200), 0, &key_manager).await;
        let utxo = input.as_transaction_input(&key_manager).await.unwrap();
        let script = script!(Nop);
        let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());
        let fee_per_gram = MicroTari(4);
        let fee = builder.fee().calculate(fee_per_gram, 1, 1, 1, 0);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_input(input)
            .await
            .unwrap()
            .with_recipient_data(
                script.clone(),
                OutputFeatures::default(),
                Covenant::default(),
                0.into(),
                MicroTari(1200) - fee - MicroTari(10),
            )
            .await
            .unwrap()
            .with_change_data(
                script.clone(),
                ExecutionStack::default(),
                a_change_key.script_key_id,
                a_change_key.spend_key_id,
                Covenant::default(),
            );
        let mut alice = builder.build().await.unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message(&key_manager).await.unwrap();
        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());
        let bob_public_key = msg.sender_offset_public_key.clone();
        let mut bob_output = WalletOutput::new_current_version(
            MicroTari(1200) - fee - MicroTari(10),
            bob_key.spend_key_id,
            OutputFeatures::default(),
            script.clone(),
            ExecutionStack::default(),
            bob_key.script_key_id,
            bob_public_key,
            CommitmentAndPublicKeySignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
        );

        let metadata_message = TransactionOutput::metadata_signature_message(&bob_output);
        bob_output.metadata_signature = key_manager
            .get_receiver_partial_metadata_signature(
                &bob_output.spending_key_id,
                &bob_output.value.into(),
                &bob_output.sender_offset_public_key,
                &msg.ephemeral_public_nonce,
                &bob_output.version,
                &metadata_message,
                bob_output.features.range_proof_type,
            )
            .await
            .unwrap();

        // Receiver gets message, deserializes it etc, and creates his response
        let mut bob_info = SingleReceiverTransactionProtocol::create(&msg, bob_output, &key_manager)
            .await
            .unwrap(); // Alice gets message back, deserializes it, etc
        alice
            .add_single_recipient_info(bob_info.clone(), &key_manager)
            .await
            .unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize(&key_manager).await {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        };
        assert!(alice.is_finalized());

        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.body.kernels()[0].fee, fee + MicroTari(10)); // Check the twist above
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.inputs()[0].commitment(), utxo.commitment());
        assert_eq!(tx.body.outputs().len(), 1);
        // Bob still needs to add the finalized metadata signature to his output after he receives the final transaction
        // from Alice
        bob_info.output.metadata_signature = tx.body.outputs()[0].metadata_signature.clone();
        assert!(bob_info.output.verify_metadata_signature().is_ok());
        assert_eq!(tx.body.outputs()[0], bob_info.output);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn single_recipient_with_change() {
        let rules = create_consensus_rules();
        let key_manager = create_test_core_key_manager_with_memory_db();
        let factories = CryptoFactories::default();
        // Alice's parameters
        let alice_key = TestParams::new(&key_manager).await;
        // Bob's parameters
        let bob_key = TestParams::new(&key_manager).await;
        let input = create_test_input(MicroTari(25000), 0, &key_manager).await;
        let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());
        let script = script!(Nop);
        let expected_fee = builder.fee().calculate(
            MicroTari(20),
            1,
            1,
            2,
            alice_key.get_size_for_default_features_and_scripts(2),
        );
        let change = TestParams::new(&key_manager).await;
        let script_key = key_manager
            .get_public_key_at_key_id(&change.script_key_id)
            .await
            .unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_change_data(
                script!(Nop),
                inputs!(script_key),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_input(input)
            .await
            .unwrap()
            .with_recipient_data(
                script.clone(),
                OutputFeatures::default(),
                Covenant::default(),
                0.into(),
                MicroTari(5000),
            )
            .await
            .unwrap();
        let mut alice = builder.build().await.unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message(&key_manager).await.unwrap();
        println!(
            "amount: {}, fee: {},  Public Excess: {}, Nonce: {}",
            msg.amount,
            msg.metadata.fee,
            msg.public_excess.to_hex(),
            msg.public_nonce.to_hex()
        );

        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());
        let bob_public_key = msg.sender_offset_public_key.clone();
        let mut bob_output = WalletOutput::new_current_version(
            MicroTari(5000),
            bob_key.spend_key_id,
            OutputFeatures::default(),
            script.clone(),
            ExecutionStack::default(),
            bob_key.script_key_id,
            bob_public_key,
            CommitmentAndPublicKeySignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
        );

        let metadata_message = TransactionOutput::metadata_signature_message(&bob_output);
        bob_output.metadata_signature = key_manager
            .get_receiver_partial_metadata_signature(
                &bob_output.spending_key_id,
                &bob_output.value.into(),
                &bob_output.sender_offset_public_key,
                &msg.ephemeral_public_nonce,
                &bob_output.version,
                &metadata_message,
                bob_output.features.range_proof_type,
            )
            .await
            .unwrap();

        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(&msg, bob_output, &key_manager)
            .await
            .unwrap();
        println!(
            "Bob's key: {}, Nonce: {}, Signature: {}, Commitment: {}",
            bob_info.public_spend_key.to_hex(),
            bob_info.partial_signature.get_public_nonce().to_hex(),
            bob_info.partial_signature.get_signature().to_hex(),
            bob_info.output.commitment.as_public_key().to_hex()
        );
        // Alice gets message back, deserializes it, etc
        alice.add_single_recipient_info(bob_info, &key_manager).await.unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize(&key_manager).await {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        };

        assert!(alice.is_finalized());
        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.body.kernels()[0].fee, expected_fee);
        assert_eq!(tx.body.inputs().len(), 1);
        // assert_eq!(tx.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(tx, None, None, u64::MAX).is_ok());
    }

    #[tokio::test]
    async fn single_recipient_range_proof_fail() {
        // Alice's parameters
        let key_manager = create_test_core_key_manager_with_memory_db_with_range_proof_size(32);
        // Bob's parameters
        let bob_key = TestParams::new(&key_manager).await;
        let input = create_test_input((2u64.pow(32) + 2001).into(), 0, &key_manager).await;
        let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());
        let script = script!(Nop);
        let change = TestParams::new(&key_manager).await;
        let script_key = key_manager
            .get_public_key_at_key_id(&change.script_key_id)
            .await
            .unwrap();
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_change_data(
                script!(Nop),
                inputs!(script_key),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_input(input)
            .await
            .unwrap()
            .with_recipient_data(
                script.clone(),
                OutputFeatures::default(),
                Covenant::default(),
                0.into(),
                (2u64.pow(32) + 1).into(),
            )
            .await
            .unwrap();
        let mut alice = builder.build().await.unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message(&key_manager).await.unwrap();
        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());
        // Receiver gets message, deserializes it etc, and creates his response
        let bob_public_key = msg.sender_offset_public_key.clone();
        let bob_output = WalletOutput::new_current_version(
            (2u64.pow(32) + 1).into(),
            bob_key.spend_key_id,
            OutputFeatures::default(),
            script.clone(),
            ExecutionStack::default(),
            bob_key.script_key_id,
            bob_public_key,
            CommitmentAndPublicKeySignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
        );

        let bob_info = SingleReceiverTransactionProtocol::create(&msg, bob_output, &key_manager).await; // Alice gets message back, deserializes it, etc
        match bob_info {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                TransactionProtocolError::TransactionBuildError(TransactionError::ValidationError(
                    "Value provided is outside the range allowed by the range proof".to_string()
                ))
            ),
        }
    }

    #[tokio::test]
    async fn disallow_fee_larger_than_amount() {
        // Alice's parameters
        let key_manager = create_test_core_key_manager_with_memory_db();
        let (utxo_amount, fee_per_gram, amount) = (MicroTari(2500), MicroTari(10), MicroTari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager).await;
        let script = script!(Nop);
        let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());
        let change = TestParams::new(&key_manager).await;
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_change_data(
                script!(Nop),
                inputs!(change.script_public_key),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_input(input)
            .await
            .unwrap()
            .with_recipient_data(
                script.clone(),
                OutputFeatures::default(),
                Covenant::default(),
                0.into(),
                amount,
            )
            .await
            .unwrap();
        // Verify that the initial 'fee greater than amount' check rejects the transaction when it is constructed
        match builder.build().await {
            Ok(_) => panic!("'BuildError(\"Fee is greater than amount\")' not caught"),
            Err(e) => assert_eq!(e.message, "Fee is greater than amount".to_string()),
        };
    }

    #[tokio::test]
    async fn allow_fee_larger_than_amount() {
        // Alice's parameters
        let key_manager = create_test_core_key_manager_with_memory_db();
        let (utxo_amount, fee_per_gram, amount) = (MicroTari(2500), MicroTari(10), MicroTari(500));
        let input = create_test_input(utxo_amount, 0, &key_manager).await;
        let script = script!(Nop);
        let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());
        let change = TestParams::new(&key_manager).await;
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_change_data(
                script!(Nop),
                inputs!(change.script_public_key),
                change.script_key_id.clone(),
                change.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_input(input)
            .await
            .unwrap()
            .with_prevent_fee_gt_amount(false)
            .with_recipient_data(
                script.clone(),
                OutputFeatures::default(),
                Covenant::default(),
                0.into(),
                amount,
            )
            .await
            .unwrap();
        // Test if the transaction passes the initial 'fee greater than amount' check when it is constructed
        match builder.build().await {
            Ok(_) => {},
            Err(e) => panic!("Unexpected error: {:?}", e),
        };
    }

    #[tokio::test]
    async fn single_recipient_with_rewindable_change_and_receiver_outputs_bulletproofs() {
        // Alice's parameters
        let key_manager_alice = create_test_core_key_manager_with_memory_db();
        let key_manager_bob = create_test_core_key_manager_with_memory_db();
        // Bob's parameters
        let bob_test_params = TestParams::new(&key_manager_bob).await;
        let alice_value = MicroTari(25000);
        let input = create_test_input(alice_value, 0, &key_manager_alice).await;

        let script = script!(Nop);

        let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager_alice.clone());
        let change_params = TestParams::new(&key_manager_alice).await;
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_change_data(
                script!(Nop),
                inputs!(change_params.script_public_key),
                change_params.script_key_id.clone(),
                change_params.spend_key_id.clone(),
                Covenant::default(),
            )
            .with_input(input)
            .await
            .unwrap()
            .with_recipient_data(
                script.clone(),
                OutputFeatures::default(),
                Covenant::default(),
                0.into(),
                MicroTari(5000),
            )
            .await
            .unwrap();
        let mut alice = builder.build().await.unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message(&key_manager_alice).await.unwrap();

        let change = alice_value - msg.amount - msg.metadata.fee;

        println!(
            "amount: {}, fee: {},  Public Excess: {}, Nonce: {}, Change: {}",
            msg.amount,
            msg.metadata.fee,
            msg.public_excess.to_hex(),
            msg.public_nonce.to_hex(),
            change
        );

        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());

        let bob_public_key = msg.sender_offset_public_key.clone();
        let bob_output = WalletOutput::new_current_version(
            MicroTari(5000),
            bob_test_params.spend_key_id,
            OutputFeatures::default(),
            script.clone(),
            ExecutionStack::default(),
            bob_test_params.script_key_id,
            bob_public_key,
            CommitmentAndPublicKeySignature::default(),
            0,
            Covenant::default(),
            EncryptedData::default(),
            0.into(),
        );

        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(&msg, bob_output, &key_manager_bob)
            .await
            .unwrap();

        // Alice gets message back, deserializes it, etc
        alice
            .add_single_recipient_info(bob_info, &key_manager_alice)
            .await
            .unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize(&key_manager_alice).await {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        };

        assert!(alice.is_finalized());
        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.body.outputs().len(), 2);

        // If the first output isn't alice's then the second must be
        // TODO: Fix this logic when 'encrypted_data.todo_decrypt()' is fixed only one of these will be possible
        let output_0 = &tx.body.outputs()[0];
        let output_1 = &tx.body.outputs()[1];

        if let Ok((key, _value)) = key_manager_alice
            .try_commitment_key_recovery(&output_0.commitment, &output_0.encrypted_data, None)
            .await
        {
            assert_eq!(key, change_params.spend_key_id);
        } else if let Ok((key, _value)) = key_manager_alice
            .try_commitment_key_recovery(&output_1.commitment, &output_1.encrypted_data, None)
            .await
        {
            assert_eq!(key, change_params.spend_key_id);
        } else {
            panic!("Could not recover Alice's output");
        }
    }
}
