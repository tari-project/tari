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

use derivative::Derivative;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    transaction::TxId,
    types::{ComAndPubSignature, HashOutput, PrivateKey, PublicKey, Signature},
};
use tari_crypto::{
    keys::PublicKey as PublicKeyTrait,
    ristretto::pedersen::PedersenCommitment,
    tari_utilities::ByteArray,
};
use tari_script::TariScript;

use super::CalculateTxIdTransactionProtocolHasherBlake256;
use crate::{
    consensus::ConsensusConstants,
    covenants::Covenant,
    transactions::{
        fee::Fee,
        tari_amount::*,
        transaction_components::{
            KernelBuilder,
            OutputFeatures,
            Transaction,
            TransactionBuilder,
            TransactionInput,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            UnblindedOutput,
            MAX_TRANSACTION_INPUTS,
            MAX_TRANSACTION_OUTPUTS,
        },
        transaction_protocol::{
            recipient::{RecipientInfo, RecipientSignedMessage},
            transaction_initializer::SenderTransactionInitializer,
            TransactionMetadata,
            TransactionProtocolError as TPE,
        },
    },
};

//----------------------------------------   Local Data types     ----------------------------------------------------//

/// This struct contains all the information that a transaction initiator (the sender) will manage throughout the
/// Transaction construction process.
// TODO: Investigate necessity to use the 'Serialize' and 'Deserialize' traits here; this could potentially leak
// TODO:   information when least expected. #LOGGED
#[derive(Clone, Derivative, Serialize, Deserialize, PartialEq)]
#[derivative(Debug)]
pub(super) struct RawTransactionInfo {
    pub num_recipients: usize,
    // The sum of self-created outputs plus change
    pub amount_to_self: MicroTari,
    pub tx_id: TxId,
    pub amounts: Vec<MicroTari>,
    pub recipient_scripts: Vec<TariScript>,
    pub recipient_output_features: Vec<OutputFeatures>,
    #[derivative(Debug = "ignore")]
    pub recipient_sender_offset_private_keys: Vec<PrivateKey>,
    pub recipient_covenants: Vec<Covenant>,
    pub recipient_minimum_value_promise: Vec<MicroTari>,
    // The sender's portion of the public commitment nonce
    #[derivative(Debug = "ignore")]
    pub private_commitment_nonces: Vec<PrivateKey>,
    pub change: MicroTari,
    pub change_output_metadata_signature: Option<ComAndPubSignature>,
    pub change_sender_offset_public_key: Option<PublicKey>,
    pub unblinded_change_output: Option<UnblindedOutput>,
    pub metadata: TransactionMetadata,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub offset: PrivateKey,
    // The sender's blinding factor shifted by the sender-selected offset
    pub offset_blinding_factor: PrivateKey,
    #[derivative(Debug = "ignore")]
    pub gamma: PrivateKey,
    pub public_excess: PublicKey,
    // The sender's private nonce
    #[derivative(Debug = "ignore")]
    pub private_nonce: PrivateKey,
    // The sender's public nonce
    pub public_nonce: PublicKey,
    // The sum of all public nonces
    pub public_nonce_sum: PublicKey,
    #[serde(skip)]
    pub recipient_info: RecipientInfo,
    pub signatures: Vec<Signature>,
    pub message: String,
    pub height: u64,
    pub prev_header: Option<HashOutput>,
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
    /// Begin constructing a new transaction. All the up-front data is collected via the
    /// `SenderTransactionInitializer` builder function
    pub fn builder(num_recipients: usize, consensus_constants: ConsensusConstants) -> SenderTransactionInitializer {
        SenderTransactionInitializer::new(num_recipients, &consensus_constants)
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

    pub fn get_total_amount(&self) -> Result<MicroTari, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.amounts.iter().sum()),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the total value of outputs being sent to yourself in the transaction
    pub fn get_amount_to_self(&self) -> Result<MicroTari, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.amount_to_self),
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
            SenderState::CollectingSingleSignature(info) => Ok(info.change),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the change output
    pub fn get_change_unblinded_output(&self) -> Result<Option<UnblindedOutput>, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.unblinded_change_output.clone()),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the metadata signature of the change output
    pub fn get_change_output_metadata_signature(&self) -> Result<Option<ComAndPubSignature>, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.change_output_metadata_signature.clone()),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the the script offset public key of the change transaction
    pub fn get_change_sender_offset_public_key(&self) -> Result<Option<PublicKey>, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.change_sender_offset_public_key.clone()),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This function will return the script offset private keys for a single recipient
    pub fn get_recipient_sender_offset_private_key(&self, recipient_index: usize) -> Result<PrivateKey, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok({
                info.recipient_sender_offset_private_keys
                    .get(recipient_index)
                    .ok_or(TPE::ScriptOffsetPrivateKeyNotFound)?
                    .clone()
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

    pub fn get_minimum_value_promise(&self, recipient_index: usize) -> Result<MicroTari, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(*info
                .recipient_minimum_value_promise
                .get(recipient_index)
                .ok_or(TPE::MinimumValuePromiseNotFound)?),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// Build the sender's message for the single-round protocol (one recipient) and move to next State
    pub fn build_single_round_message(&mut self) -> Result<SingleRoundSenderData, TPE> {
        match &self.state {
            SenderState::SingleRoundMessageReady(info) => {
                let result = self.get_single_round_message()?;
                self.state = SenderState::CollectingSingleSignature(info.clone());
                Ok(result)
            },
            _ => Err(TPE::InvalidStateError),
        }
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
    pub fn get_single_round_message(&self) -> Result<SingleRoundSenderData, TPE> {
        match &self.state {
            SenderState::SingleRoundMessageReady(info) | SenderState::CollectingSingleSignature(info) => {
                let recipient_output_features = info.recipient_output_features.first().cloned().ok_or_else(|| {
                    TPE::IncompleteStateError("The recipient output features should be available".to_string())
                })?;
                let recipient_script =
                    info.recipient_scripts.first().cloned().ok_or_else(|| {
                        TPE::IncompleteStateError("The recipient script should be available".to_string())
                    })?;
                let recipient_script_offset_secret_key =
                    info.recipient_sender_offset_private_keys.first().ok_or_else(|| {
                        TPE::IncompleteStateError("The recipient script offset should be available".to_string())
                    })?;
                let private_commitment_nonce = info.private_commitment_nonces.first().ok_or_else(|| {
                    TPE::IncompleteStateError("The sender's private commitment nonce should be available".to_string())
                })?;
                let recipient_covenant = info.recipient_covenants.first().cloned().ok_or_else(|| {
                    TPE::IncompleteStateError("The recipient covenant should be available".to_string())
                })?;
                let recipient_minimum_value_promise =
                    info.recipient_minimum_value_promise.first().copied().ok_or_else(|| {
                        TPE::IncompleteStateError("The recipient minimum value promise should be available".to_string())
                    })?;

                Ok(SingleRoundSenderData {
                    tx_id: info.tx_id,
                    amount: self.get_total_amount()?,
                    public_nonce: info.public_nonce.clone(),
                    public_excess: info.public_excess.clone(),
                    metadata: info.metadata.clone(),
                    message: info.message.clone(),
                    features: recipient_output_features,
                    script: recipient_script,
                    sender_offset_public_key: PublicKey::from_secret_key(recipient_script_offset_secret_key),
                    ephemeral_public_nonce: PublicKey::from_secret_key(private_commitment_nonce),
                    covenant: recipient_covenant,
                    minimum_value_promise: recipient_minimum_value_promise,
                })
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Add the signed transaction from the recipient and move to the next state
    pub fn add_single_recipient_info(&mut self, rec: RecipientSignedMessage) -> Result<(), TPE> {
        match &mut self.state {
            SenderState::CollectingSingleSignature(info) => {
                // Consolidate transaction info
                info.outputs.push(rec.output.clone());

                // Update Gamma with this output
                let recipient_sender_offset_private_key =
                    info.recipient_sender_offset_private_keys.first().ok_or_else(|| {
                        TPE::IncompleteStateError(
                            "For single recipient there should be one recipient script offset".to_string(),
                        )
                    })?;
                info.gamma = info.gamma.clone() - recipient_sender_offset_private_key.clone();

                // Finalize the combined metadata signature by adding the receiver signature portion
                let private_commitment_nonce = info.private_commitment_nonces.first().ok_or_else(|| {
                    TPE::IncompleteStateError("The sender's private commitment nonce should be available".to_string())
                })?;
                let index = info.outputs.len() - 1;
                if info.outputs[index].verify_metadata_signature().is_err() {
                    info.outputs[index].metadata_signature = SenderTransactionProtocol::finalize_metadata_signature(
                        private_commitment_nonce,
                        recipient_sender_offset_private_key,
                        &info.outputs[index].clone(),
                    )?;
                }

                // Nonce is in the signature, so we'll add those together later
                info.public_excess = &info.public_excess + &rec.public_spend_key;
                info.public_nonce_sum = &info.public_nonce_sum + rec.partial_signature.get_public_nonce();
                info.signatures.push(rec.partial_signature);
                info.metadata = rec.tx_metadata;
                self.state = SenderState::Finalizing(info.clone());
                Ok(())
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    fn finalize_metadata_signature(
        private_commitment_nonce: &PrivateKey,
        sender_offset_private_key: &PrivateKey,
        output: &TransactionOutput,
    ) -> Result<ComAndPubSignature, TPE> {
        // Create sender signature
        let sender_signature = TransactionOutput::create_sender_partial_metadata_signature(
            output.version,
            &output.commitment,
            output.metadata_signature.ephemeral_commitment(),
            &output.script,
            &output.features,
            sender_offset_private_key,
            Some(private_commitment_nonce),
            &output.covenant,
            &output.encrypted_data,
            output.minimum_value_promise,
        )?;
        // Create aggregated metadata signature
        let aggregated_metadata_signature = &sender_signature + &output.metadata_signature;

        Ok(aggregated_metadata_signature)
    }

    /// Attempts to build the final transaction.
    fn build_transaction(info: &RawTransactionInfo) -> Result<Transaction, TPE> {
        let mut tx_builder = TransactionBuilder::new();
        for i in &info.inputs {
            tx_builder.add_input(i.clone());
        }

        for o in &info.outputs {
            tx_builder.add_output(o.clone());
        }
        tx_builder.add_offset(info.offset.clone());
        tx_builder.add_script_offset(info.gamma.clone());
        let mut s_agg = info.signatures[0].clone();
        info.signatures.iter().skip(1).for_each(|s| s_agg = &s_agg + s);
        let excess = PedersenCommitment::from_public_key(&info.public_excess);

        let kernel = KernelBuilder::new()
            .with_fee(info.metadata.fee)
            .with_features(info.metadata.kernel_features)
            .with_lock_height(info.metadata.lock_height)
            .with_burn_commitment(info.metadata.burn_commitment.clone())
            .with_excess(&excess)
            .with_signature(&s_agg)
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
            if info.signatures.len() != 1 + info.num_recipients {
                return Err(TPE::ValidationError(format!(
                    "Incorrect number of signatures ({})",
                    info.signatures.len()
                )));
            }
            Ok(())
        } else {
            Err(TPE::InvalidStateError)
        }
    }

    /// Produce the sender's partial signature
    fn sign(&mut self) -> Result<(), TPE> {
        match &mut self.state {
            SenderState::Finalizing(info) => {
                let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
                    &TransactionKernelVersion::get_current_version(),
                    &info.public_nonce_sum,
                    &info.public_excess,
                    &info.metadata,
                );
                // let e = build_challenge(&info.public_nonce_sum, &info.metadata);

                let k = info.offset_blinding_factor.clone();
                let r = info.private_nonce.clone();
                let s = Signature::sign_raw(&k, r, &e).map_err(TPE::SigningError)?;
                info.signatures.push(s);
                Ok(())
            },
            _ => Err(TPE::InvalidStateError),
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
    pub fn finalize(&mut self) -> Result<(), TPE> {
        // Create the final aggregated signature, moving to the Failed state if anything goes wrong
        match &mut self.state {
            SenderState::Finalizing(_) => {
                if let Err(e) = self.sign() {
                    self.state = SenderState::Failed(e.clone());
                    return Err(e);
                }
            },
            _ => return Err(TPE::InvalidStateError),
        }
        // Validate the inputs we have, and then construct the final transaction
        match &self.state {
            SenderState::Finalizing(info) => {
                let result = self.validate().and_then(|_| Self::build_transaction(info));
                match result {
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

    /// This method is used to store a pending transaction to be sent which should be in the CollectionSingleSignature
    /// state, This state will be serialized and returned as a string.
    pub fn save_pending_transaction_to_be_sent(&self) -> Result<String, TPE> {
        match &self.state {
            SenderState::Initializing(_) => Err(TPE::InvalidStateError),
            SenderState::SingleRoundMessageReady(_) => Err(TPE::InvalidStateError),
            SenderState::CollectingSingleSignature(s) => {
                let data = serde_json::to_string(s).map_err(|_| TPE::SerializationError)?;
                Ok(data)
            },
            SenderState::Finalizing(_) => Err(TPE::InvalidStateError),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// This method takes the serialized data from the previous method, deserializes it and recreates the pending Sender
    /// Transaction from it.
    pub fn load_pending_transaction_to_be_sent(data: &str) -> Result<Self, TPE> {
        let raw_data: RawTransactionInfo = serde_json::from_str(data).map_err(|_| TPE::SerializationError)?;
        Ok(Self {
            state: SenderState::CollectingSingleSignature(Box::new(raw_data)),
        })
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
            SenderState::Initializing(info) => match info.num_recipients {
                0 => Ok(SenderState::Finalizing(info)),
                1 => Ok(SenderState::SingleRoundMessageReady(info)),
                _ => Ok(SenderState::Failed(TPE::UnsupportedError(
                    "Multiple recipients are not supported yet".into(),
                ))),
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
    use rand::rngs::OsRng;
    use tari_common_types::types::{CommitmentFactory, PrivateKey, PublicKey, RangeProof};
    use tari_crypto::{
        commitment::HomomorphicCommitmentFactory,
        errors::RangeProofError::ProofConstructionError,
        keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
        range_proof::RangeProofService,
        tari_utilities::{hex::Hex, ByteArray},
    };
    use tari_script::{script, ExecutionStack, TariScript};

    use super::SenderState;
    use crate::{
        covenants::Covenant,
        test_helpers::{create_consensus_constants, create_consensus_rules},
        transactions::{
            crypto_factories::CryptoFactories,
            tari_amount::*,
            test_helpers::{create_non_recoverable_unblinded_output, create_test_input, TestParams},
            transaction_components::{EncryptedData, OutputFeatures, TransactionOutput, TransactionOutputVersion},
            transaction_protocol::{
                sender::{SenderTransactionProtocol, TransactionSenderMessage},
                single_receiver::SingleReceiverTransactionProtocol,
                TransactionProtocolError,
                TransactionProtocolError::RangeProofError,
            },
        },
        validation::transaction::TransactionInternalConsistencyValidator,
    };

    #[test]
    fn test_not_single() {
        assert_eq!(TransactionSenderMessage::None.single(), None);
        assert_eq!(TransactionSenderMessage::Multiple.single(), None);
    }

    #[test]
    fn test_errors() {
        let mut stp = SenderTransactionProtocol {
            state: SenderState::Failed(TransactionProtocolError::InvalidStateError),
        };
        assert_eq!(stp.get_transaction(), Err(TransactionProtocolError::InvalidStateError));
        assert_eq!(
            stp.clone().take_transaction(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert!(!stp.check_tx_id(0u64.into()));
        assert_eq!(stp.get_tx_id(), Err(TransactionProtocolError::InvalidStateError));
        assert_eq!(stp.get_total_amount(), Err(TransactionProtocolError::InvalidStateError));
        assert_eq!(
            stp.get_amount_to_self(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_change_amount(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_change_unblinded_output(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_change_output_metadata_signature(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_change_sender_offset_public_key(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.get_recipient_sender_offset_private_key(0),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(stp.get_fee_amount(), Err(TransactionProtocolError::InvalidStateError));
        assert_eq!(
            stp.clone().build_single_round_message(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.clone().revert_sender_state_to_single_round_message_ready(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(
            stp.clone().get_single_round_message(),
            Err(TransactionProtocolError::InvalidStateError)
        );
        assert_eq!(stp.sign(), Err(TransactionProtocolError::InvalidStateError));
    }

    #[test]
    fn test_metadata_signature_finalize() {
        // Defaults
        let commitment_factory = CommitmentFactory::default();
        let crypto_factory = CryptoFactories::default();

        // Sender data
        let ephemeral_private_nonce = PrivateKey::random(&mut OsRng);
        let sender_ephemeral_public_nonce = PublicKey::from_secret_key(&ephemeral_private_nonce);
        let value = 1000u64;
        let sender_offset_private_key = PrivateKey::random(&mut OsRng);
        let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);

        // Shared contract data
        let script = Default::default();
        let output_features = Default::default();

        // Receiver data
        let spending_key = PrivateKey::random(&mut OsRng);
        let commitment = commitment_factory.commit(&spending_key, &PrivateKey::from(value));
        let proof = RangeProof::from_bytes(
            &crypto_factory
                .range_proof
                .construct_proof(&spending_key, value)
                .unwrap(),
        )
        .unwrap();
        let covenant = Covenant::default();

        // Encrypted value
        let encryption_key = PrivateKey::random(&mut OsRng);
        let encrypted_data =
            EncryptedData::encrypt_data(&encryption_key, &commitment, value.into(), &spending_key).unwrap();

        let minimum_value_promise = MicroTari::zero();

        let partial_metadata_signature = TransactionOutput::create_receiver_partial_metadata_signature(
            TransactionOutputVersion::get_current_version(),
            value.into(),
            &spending_key,
            &script,
            &output_features,
            &sender_offset_public_key,
            &sender_ephemeral_public_nonce,
            &covenant,
            &encrypted_data,
            minimum_value_promise,
        )
        .unwrap();

        let mut output = TransactionOutput::new_current_version(
            Default::default(),
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
        let partial_sender_metadata_signature = TransactionOutput::create_sender_partial_metadata_signature(
            TransactionOutputVersion::get_current_version(),
            &output.commitment,
            partial_metadata_signature.ephemeral_commitment(),
            &script,
            &output_features,
            &sender_offset_private_key,
            Some(&ephemeral_private_nonce),
            &covenant,
            &encrypted_data,
            minimum_value_promise,
        )
        .unwrap();
        output.metadata_signature = &partial_metadata_signature + &partial_sender_metadata_signature;
        assert!(output.verify_metadata_signature().is_ok());
    }

    #[test]
    fn zero_recipients() {
        let factories = CryptoFactories::default();
        let p1 = TestParams::new();
        let p2 = TestParams::new();
        let (utxo, input) = create_test_input(MicroTari(1200), 0, &factories.commitment);
        let mut builder = SenderTransactionProtocol::builder(0, create_consensus_constants(0));
        let script = TariScript::default();
        let output_features = OutputFeatures::default();

        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(2))
            .with_offset(p1.offset.clone() + p2.offset.clone())
            .with_private_nonce(p1.nonce.clone())
            .with_change_secret(p1.change_spend_key.clone())
            .with_input(utxo, input)
            .with_output(
                create_non_recoverable_unblinded_output(script.clone(), output_features.clone(), &p1, MicroTari(500))
                    .unwrap(),
                p1.sender_offset_private_key.clone(),
            )
            .unwrap()
            .with_output(
                create_non_recoverable_unblinded_output(script, output_features, &p2, MicroTari(400)).unwrap(),
                p2.sender_offset_private_key.clone(),
            )
            .unwrap();
        let mut sender = builder.build(&factories, None, u64::MAX).unwrap();
        assert!(!sender.is_failed());
        assert!(sender.is_finalizing());
        match sender.finalize() {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        }
        let tx = sender.get_transaction().unwrap();
        assert_eq!(tx.offset, p1.offset + p2.offset);
    }

    #[test]
    fn single_recipient_no_change() {
        let factories = CryptoFactories::default();
        // Alice's parameters
        let a = TestParams::new();
        // Bob's parameters
        let b = TestParams::new();
        let (utxo, input) = create_test_input(MicroTari(1200), 0, &factories.commitment);
        let script = script!(Nop);
        let mut builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
        let fee_per_gram = MicroTari(4);
        let fee = builder.fee().calculate(fee_per_gram, 1, 1, 1, 0);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_input(utxo.clone(), input)
            .with_recipient_data(0, script.clone(), PrivateKey::random(&mut OsRng), OutputFeatures::default(), PrivateKey::random(&mut OsRng), Covenant::default(), MicroTari::zero())
            .with_change_script(script, ExecutionStack::default(), PrivateKey::default())
            // A little twist: Check the case where the change is less than the cost of another output
            .with_amount(0, MicroTari(1200) - fee - MicroTari(10));
        let mut alice = builder.build(&factories, None, u64::MAX).unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message().unwrap();
        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());

        // Test serializing the current state to be sent and resuming from that serialized data
        let ser = alice.save_pending_transaction_to_be_sent().unwrap();
        let mut alice = SenderTransactionProtocol::load_pending_transaction_to_be_sent(&ser).unwrap();

        // Receiver gets message, deserializes it etc, and creates his response
        let mut bob_info = SingleReceiverTransactionProtocol::create(
            &msg,
            b.nonce,
            b.spend_key,
            &factories,
            &EncryptedData::default(),
        )
        .unwrap(); // Alice gets message back, deserializes it, etc
        alice.add_single_recipient_info(bob_info.clone()).unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize() {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        };
        assert!(alice.is_finalized());

        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.offset, a.offset);
        assert_eq!(tx.body.kernels()[0].fee, fee + MicroTari(10)); // Check the twist above
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.inputs()[0], utxo);
        assert_eq!(tx.body.outputs().len(), 1);
        // Bob still needs to add the finalized metadata signature to his output after he receives the final transaction
        // from Alice
        bob_info.output.metadata_signature = tx.body.outputs()[0].metadata_signature.clone();
        assert!(bob_info.output.verify_metadata_signature().is_ok());
        assert_eq!(tx.body.outputs()[0], bob_info.output);
    }

    #[test]
    fn single_recipient_with_change() {
        let rules = create_consensus_rules();
        let factories = CryptoFactories::default();
        // Alice's parameters
        let a = TestParams::new();
        // Bob's parameters
        let b = TestParams::new();
        let (utxo, input) = create_test_input(MicroTari(25000), 0, &factories.commitment);
        let mut builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
        let script = script!(Nop);
        let expected_fee =
            builder
                .fee()
                .calculate(MicroTari(20), 1, 1, 2, a.get_size_for_default_features_and_scripts(2));
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_spend_key.clone())
            .with_input(utxo.clone(), input)
            .with_recipient_data(
                0,
                script.clone(),
                PrivateKey::random(&mut OsRng),
                OutputFeatures::default(),
                PrivateKey::random(&mut OsRng),
                Covenant::default(),
                MicroTari::zero(),
            )
            .with_change_script(script, ExecutionStack::default(), PrivateKey::default())
            .with_amount(0, MicroTari(5000));
        let mut alice = builder.build(&factories, None, u64::MAX).unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message().unwrap();
        println!(
            "amount: {}, fee: {},  Public Excess: {}, Nonce: {}",
            msg.amount,
            msg.metadata.fee,
            msg.public_excess.to_hex(),
            msg.public_nonce.to_hex()
        );

        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());

        // Test serializing the current state to be sent and resuming from that serialized data
        let ser = alice.save_pending_transaction_to_be_sent().unwrap();
        let mut alice = SenderTransactionProtocol::load_pending_transaction_to_be_sent(&ser).unwrap();

        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(
            &msg,
            b.nonce,
            b.spend_key,
            &factories,
            &EncryptedData::default(),
        )
        .unwrap();
        println!(
            "Bob's key: {}, Nonce: {}, Signature: {}, Commitment: {}",
            bob_info.public_spend_key.to_hex(),
            bob_info.partial_signature.get_public_nonce().to_hex(),
            bob_info.partial_signature.get_signature().to_hex(),
            bob_info.output.commitment.as_public_key().to_hex()
        );
        // Alice gets message back, deserializes it, etc
        alice.add_single_recipient_info(bob_info).unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize() {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        };

        assert!(alice.is_finalized());
        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.offset, a.offset);
        assert_eq!(tx.body.kernels()[0].fee, expected_fee);
        assert_eq!(tx.body.inputs().len(), 1);
        assert_eq!(tx.body.inputs()[0], utxo);
        assert_eq!(tx.body.outputs().len(), 2);
        let validator = TransactionInternalConsistencyValidator::new(false, rules, factories);
        assert!(validator.validate(tx, None, None, u64::MAX).is_ok());
    }

    #[test]
    fn single_recipient_range_proof_fail() {
        let factories = CryptoFactories::new(32);
        // Alice's parameters
        let a = TestParams::new();
        // Bob's parameters
        let b = TestParams::new();
        let (utxo, input) = create_test_input((2u64.pow(32) + 2001).into(), 0, &factories.commitment);
        let mut builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
        let script = script!(Nop);

        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_spend_key)
            .with_input(utxo, input)
            .with_recipient_data(
                0,
                script.clone(),
                PrivateKey::random(&mut OsRng),
                OutputFeatures::default(),
                PrivateKey::random(&mut OsRng),
                Covenant::default(),
                MicroTari::zero(),
            )
            .with_change_script(script, ExecutionStack::default(), PrivateKey::default())
            .with_amount(0, (2u64.pow(32) + 1).into());
        let mut alice = builder.build(&factories, None, u64::MAX).unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message().unwrap();
        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());
        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(
            &msg,
            b.nonce,
            b.spend_key,
            &factories,
            &EncryptedData::default(),
        ); // Alice gets message back, deserializes it, etc
        match bob_info {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                RangeProofError(ProofConstructionError(
                    "Invalid array/vector length error: `Value too large, bit vector capacity will be exceeded`"
                        .to_string()
                ))
            ),
        }
    }

    #[test]
    fn disallow_fee_larger_than_amount() {
        let factories = CryptoFactories::default();
        // Alice's parameters
        let alice = TestParams::new();
        let (utxo_amount, fee_per_gram, amount) = (MicroTari(2500), MicroTari(10), MicroTari(500));
        let (utxo, input) = create_test_input(utxo_amount, 0, &factories.commitment);
        let script = script!(Nop);
        let mut builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(alice.offset.clone())
            .with_private_nonce(alice.nonce.clone())
            .with_change_secret(alice.change_spend_key)
            .with_input(utxo, input)
            .with_amount(0, amount)
            .with_recipient_data(
                0,
                script.clone(),
                PrivateKey::random(&mut OsRng),
                Default::default(),
                PrivateKey::random(&mut OsRng),
                Covenant::default(),
                MicroTari::zero(),
            )
            .with_change_script(script, ExecutionStack::default(), PrivateKey::default());
        // Verify that the initial 'fee greater than amount' check rejects the transaction when it is constructed
        match builder.build(&factories, None, u64::MAX) {
            Ok(_) => panic!("'BuildError(\"Fee is greater than amount\")' not caught"),
            Err(e) => assert_eq!(e.message, "Fee is greater than amount".to_string()),
        };
    }

    #[test]
    fn allow_fee_larger_than_amount() {
        let factories = CryptoFactories::default();
        // Alice's parameters
        let alice = TestParams::new();
        let (utxo_amount, fee_per_gram, amount) = (MicroTari(2500), MicroTari(10), MicroTari(500));
        let (utxo, input) = create_test_input(utxo_amount, 0, &factories.commitment);
        let script = script!(Nop);
        let mut builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(alice.offset.clone())
            .with_private_nonce(alice.nonce.clone())
            .with_change_secret(alice.change_spend_key)
            .with_input(utxo, input)
            .with_amount(0, amount)
            .with_prevent_fee_gt_amount(false)
            .with_recipient_data(
                0,
                script.clone(),
                PrivateKey::random(&mut OsRng),
                Default::default(),
                PrivateKey::random(&mut OsRng),
                Covenant::default(),
                MicroTari::zero(),
            )
            .with_change_script(script, ExecutionStack::default(), PrivateKey::default());
        // Test if the transaction passes the initial 'fee greater than amount' check when it is constructed
        match builder.build(&factories, None, u64::MAX) {
            Ok(_) => {},
            Err(e) => panic!("Unexpected error: {:?}", e),
        };
    }

    #[test]
    fn single_recipient_with_rewindable_change_and_receiver_outputs_bulletproofs() {
        let factories = CryptoFactories::default();
        // Alice's parameters
        let alice_test_params = TestParams::new();
        // Bob's parameters
        let bob_test_params = TestParams::new();
        let alice_value = MicroTari(25000);
        let (utxo, input) = create_test_input(alice_value, 0, &factories.commitment);

        let script = script!(Nop);

        let mut builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_offset(alice_test_params.offset.clone())
            .with_private_nonce(alice_test_params.nonce.clone())
            .with_change_secret(alice_test_params.change_spend_key.clone())
            .with_recoverable_outputs(alice_test_params.recovery_data.clone())
            .with_input(utxo, input)
            .with_amount(0, MicroTari(5000))
            .with_recipient_data(
                0,
                script.clone(),
                PrivateKey::random(&mut OsRng),
                OutputFeatures::default(),
                PrivateKey::random(&mut OsRng),
                Covenant::default(),
                MicroTari::zero(),
            )
            .with_change_script(script, ExecutionStack::default(), PrivateKey::default());
        let mut alice = builder.build(&factories, None, u64::MAX).unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message().unwrap();

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

        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(
            &msg,
            bob_test_params.nonce,
            bob_test_params.spend_key,
            &factories,
            &EncryptedData::default(),
        )
        .unwrap();

        // Alice gets message back, deserializes it, etc
        alice.add_single_recipient_info(bob_info).unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize() {
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

        if let Ok((committed_value, blinding_factor)) = EncryptedData::decrypt_data(
            &alice_test_params.recovery_data.encryption_key,
            &output_0.commitment,
            &output_0.encrypted_data,
        ) {
            assert_eq!(
                factories
                    .commitment
                    .commit_value(&blinding_factor, committed_value.as_u64()),
                output_0.commitment
            );
        } else if let Ok((committed_value, blinding_factor)) = EncryptedData::decrypt_data(
            &alice_test_params.recovery_data.encryption_key,
            &output_1.commitment,
            &output_1.encrypted_data,
        ) {
            assert_eq!(
                factories
                    .commitment
                    .commit_value(&blinding_factor, committed_value.as_u64()),
                output_1.commitment
            );
        } else {
            panic!("Could not recover Alice's output");
        }
    }
}
