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
        TransactionProtocolError,
    },
};
use digest::Digest;
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_utilities::ByteArray;

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
    Failed(TransactionProtocolError),
}

impl SenderState {
    pub fn initialize(self) -> Result<SenderState, TransactionProtocolError> {
        match self {
            SenderState::Initializing(info) => match info.num_recipients {
                0 => Ok(SenderState::Finalizing(info)),
                1 => Ok(SenderState::SingleRoundMessageReady(info)),
                _ => Ok(SenderState::Failed(TransactionProtocolError::UnsupportedError(
                    "Multiple recipients are not supported yet".into(),
                ))),
            },
            _ => Err(TransactionProtocolError::InvalidTransitionError),
        }
    }
}

//----------------------------------------         Tests          ----------------------------------------------------//
