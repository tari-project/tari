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

use crate::{
    transaction::{TransactionInput, TransactionOutput},
    types::{BlindingFactor, SecretKey, Signature},
};

use crate::{
    transaction::{KernelFeatures, Transaction, TransactionBuilder, TransactionKernel},
    types::{CommitmentFactory, PublicKey},
};

use crate::{
    transaction::{MAX_TRANSACTION_INPUTS, MAX_TRANSACTION_OUTPUTS, MINIMUM_TRANSACTION_FEE},
    transaction_protocol::{
        receiver::{RecipientInfo, RecipientSignedTransactionData},
        transaction_initializer::SenderTransactionInitializer,
        TransactionMetadata,
        TransactionProtocolError as TPE,
    },
};
use digest::Digest;
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_utilities::ByteArray;
use crate::transaction::KernelBuilder;

//----------------------------------------   Local Data types     ----------------------------------------------------//

/// This struct contains all the information that a transaction initiator (the sender) will manage throughout the
/// Transaction construction process.
#[derive(Clone, Debug)]
pub(super) struct RawTransactionInfo {
    pub num_recipients: usize,
    pub ids: Vec<u64>,
    pub amounts: Vec<u64>,
    pub metadata: TransactionMetadata,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub offset: BlindingFactor,
    // The sender's blinding factor shifted by the sender-selected offset
    pub offset_blinding_factor: BlindingFactor,
    pub public_excess: PublicKey,
    pub private_nonce: SecretKey,
    pub public_nonce: PublicKey,
    pub recipient_info: RecipientInfo,
    pub signatures: Vec<Signature>,
}

impl RawTransactionInfo {
    pub fn calculate_total_amount(&self) -> u64 {
        self.amounts.iter().fold(0u64, |sum, v| sum + v)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SingleRoundSenderData {
    /// The transaction id for the recipient
    pub tx_id: u64,
    /// The amount, in µT, being sent to the recipient
    pub amount: u64,
    /// The offset public excess for this transaction
    pub public_excess: PublicKey,
    /// The sender's public nonce
    pub public_nonce: PublicKey,
    /// The transaction metadata
    pub metadata: TransactionMetadata,
}

pub enum SenderMessage {
    None,
    Single(Box<SingleRoundSenderData>),
    // TODO: Three round types
    Multiple,
}

//----------------------------------------  Sender State Protocol ----------------------------------------------------//
#[derive(Debug)]
pub struct SenderTransactionProtocol {
    pub(super) state: SenderState,
}

impl SenderTransactionProtocol {
    /// Begin constructing a new transaction. All the up-front data is collected via the `SenderTransactionInitializer`
    /// builder function
    pub fn new(num_recipients: usize) -> SenderTransactionInitializer {
        SenderTransactionInitializer::new(num_recipients)
    }

    /// Convenience method to check whether we're receiving recipient data
    pub fn is_collecting_single_signature(&self) -> bool {
        match &self.state {
            SenderState::CollectingSingleSignature(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the SenderState::FinalizedTransaction state
    pub fn is_finalized(&self) -> bool {
        match &self.state {
            SenderState::FinalizedTransaction(_) => true,
            _ => false,
        }
    }

    /// Method to determine if the transaction protocol has failed
    pub fn is_failed(&self) -> bool {
        match &self.state {
            SenderState::Failed(_) => true,
            _ => false,
        }
    }

    /// Method to return the error behind a failure, if one has occurred
    pub fn failure_reason(&self) -> Option<TPE> {
        match &self.state {
            SenderState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    pub fn get_total_amount(&self) -> Result<u64, TPE> {
        match &self.state {
            SenderState::Initializing(info) |
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.amounts.iter().sum::<u64>()),
            SenderState::FinalizedTransaction(_) => Err(TPE::InvalidStateError),
            SenderState::Failed(_) => Err(TPE::InvalidStateError),
        }
    }

    /// Build the sender's message for the single-round protocol (one recipient) and move to next State
    pub fn build_single_round_message(&mut self) -> Result<SingleRoundSenderData, TPE> {
        match &self.state {
            SenderState::SingleRoundMessageReady(info) => {
                let result = SingleRoundSenderData {
                    tx_id: info.ids[0],
                    amount: self.get_total_amount().unwrap(),
                    public_nonce: info.public_nonce.clone(),
                    public_excess: info.public_nonce.clone(),
                    metadata: info.metadata.clone(),
                };
                self.state = SenderState::CollectingSingleSignature(info.clone());
                Ok(result)
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Add the signed transaction from the recipient and move to the next state
    pub fn add_single_recipient_info(&mut self, rec: RecipientSignedTransactionData) -> Result<(), TPE> {
        match &mut self.state {
            SenderState::CollectingSingleSignature(info) => {
                // Consolidate transaction info
                info.outputs.push(rec.output);
                info.signatures.push(rec.partial_signature);
                self.state = SenderState::Finalizing(info.clone());
                Ok(())
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Attempts to build the final transaction.
    fn build_transaction(info: &RawTransactionInfo, features: KernelFeatures) -> Result<Transaction, TPE> {
        let mut tx_builder = TransactionBuilder::new();
        for i in &info.inputs {
            tx_builder.add_input(i.clone());
        }

        for o in &info.outputs {
            tx_builder.add_output(o.clone());
        }
        tx_builder.add_offset(info.offset.clone());
        let mut s_agg = info.signatures[0].clone();
        info.signatures.iter().take(1).for_each(|s| s_agg = &s_agg + s);
        let excess = CommitmentFactory::from_public_key(&info.public_excess);
        let kernel = KernelBuilder::new()
            .with_fee(info.metadata.fee)
            .with_features(features)
            .with_lock_height(info.metadata.lock_height)
            .with_excess(&excess)
            .with_signature(&s_agg)
            .build()?;
        tx_builder.with_kernel(kernel);
        let tx = tx_builder.build()?;
        Ok(tx)
    }

    /// Performs sanitary checks on the collected transaction pieces prior to building the final Transaction instance
    fn validate(&self) -> Result<(), TPE> {
        if let SenderState::Finalizing(info) = &self.state {
            // The fee should be less than the amount. This isn't a protocol requirement, but it's what you want 99.999%
            // of the time, and our users will thank us if we reject a tx where they put the amount in the fee field by
            // mistake!
            let total_amount = info.calculate_total_amount();
            let fee = info.metadata.fee;
            if fee > total_amount {
                return Err(TPE::ValidationError(
                    "Fee is greater than amount".into(),
                ));
            }
            // The fee must be greater than MIN_FEE to prevent spam attacks
            if fee < MINIMUM_TRANSACTION_FEE {
                return Err(TPE::ValidationError(
                    "Fee is less than the minimum".into(),
                ));
            }
            // Prevent overflow attacks by imposing sane limits on some key parameters
            if info.inputs.len() > MAX_TRANSACTION_INPUTS {
                return Err(TPE::ValidationError(
                    "Too many inputs in transaction".into(),
                ));
            }
            if info.outputs.len() > MAX_TRANSACTION_OUTPUTS {
                return Err(TPE::ValidationError(
                    "Too many outputs in transaction".into(),
                ));
            }
            if info.inputs.len() == 0 {
                return Err(TPE::ValidationError(
                    "A transaction cannot have zero inputs".into(),
                ));
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

    /// Try and finalise the transaction. If the current state is Finalizing, the result will be whether the
    /// transaction was valid or not. If the result is false, the transaction will be in a Failed state. Calling
    /// finalize while in any other state will result in an error.
    ///
    /// First we validate against internal sanity checks, then try build the transaction, and then
    /// formally validate the transaction terms (no inflation, signature matches etc). If any step fails,
    /// the transaction protocol moves to Failed state and we are done; you can't rescue the situation. The function
    /// returns `Ok(false)` in this instance.
    pub fn finalize(&mut self, features: KernelFeatures) -> Result<bool, TPE> {
        match &self.state {
            SenderState::Finalizing(info) => {
                let result = self.validate().and_then(|_| Self::build_transaction(info, features));
                if let Err(e) = result {
                    self.state = SenderState::Failed(e);
                    return Ok(false);
                }
                let mut transaction = result.unwrap();
                let result = transaction
                    .validate_internal_consistency()
                    .map_err(|e| TPE::TransactionBuildError(e));
                if let Err(e) = result {
                    self.state = SenderState::Failed(e);
                    return Ok(false);
                }
                self.state = SenderState::FinalizedTransaction(transaction);
                Ok(true)
            },
            _ => Err(TPE::InvalidStateError),
        }
    }
}

pub fn calculate_tx_id<D: Digest>(pub_nonce: &PublicKey, index: usize) -> u64 {
    let hash = D::new().chain(pub_nonce.as_bytes()).chain(index.to_le_bytes()).result();
    let mut bytes: [u8; 8] = [0u8; 8];
    bytes.copy_from_slice(&hash[..8]);
    u64::from_le_bytes(bytes)
}

//----------------------------------------      Sender State      ----------------------------------------------------//

/// This enum contains all the states of the Sender state machine
#[derive(Debug)]
pub(super) enum SenderState {
    /// Transitional state that kicks of the relevant transaction protocol
    Initializing(RawTransactionInfo),
    /// The message for the recipient in a single-round scheme is ready
    SingleRoundMessageReady(RawTransactionInfo),
    /// Waiting for the signed transaction data in the single-round protocol
    CollectingSingleSignature(RawTransactionInfo),
    /// The final transaction state is being validated - it will automatically transition to Failed or Finalized from
    /// here
    Finalizing(RawTransactionInfo),
    /// The final transaction is ready to be broadcast
    FinalizedTransaction(Transaction),
    /// An unrecoverable failure has occurred and the transaction must be abandoned
    Failed(TPE),
}

impl SenderState {
    pub fn initialize(self) -> Result<SenderState, TPE> {
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

//----------------------------------------         Tests          ----------------------------------------------------//
