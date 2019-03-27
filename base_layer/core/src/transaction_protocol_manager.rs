// Copyright 2019 The Tari Project
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

//! The Transaction Protocol Manager implements a protocol to construct a Mimwblewimble transaction between two parties
//! , a Sender and a Receiver. In this transaction the Sender is paying the Receiver from their inputs and also paying
//! to as many change outputs as they like. The Receiver will receive a single output from this transaction.
//! The module is written to allow for a single entity to construct a transaction by moving through the state machine
//! and providing all the required data OR for two remote parties to collaborate to build the transaction by having the
//! Transaction Protocol Manager construct and accept messages with the public information required from the two
//! parties. Wallets are an example of an application that will often be constructing transactions to pay themselves as
//! part of UTXO management. Peer-to-Peer payments will make use of the public messages for the remote parties to
//! collaborate.
//!
//! Below is an illustration of the single user state machine flow that this module implements, the transitions in the
//! diagram represent the various events that the state machine can handle.
//!
//! +------------+
//! |            +-------+
//! |   Sender   |       | SetFee
//! |   Setup    +<------+
//! |            |
//! |            +-------+
//! |            |       | SetLockHeight
//! |            +<------+
//! |            |
//! |            +-------+
//! |            |       | SetAmount
//! |            +<------+
//! |            |
//! |            +-------+
//! |            |       | AddInput                                   +---------+
//! |            +<------+                                            | \o/ Fin |
//! |            |                                                    +----+----+
//! |            +-------+                                                 ^
//! |            |       | AddChangeOutput                                 |
//! |            +<------+                                                 |
//! |            |                                                   +-----+------+
//! |            +-------+                                           |            |
//! |            |       | SetSenderNonce                            | Finalized  |
//! |            +<------+                                           | Transaction|
//! |            |                                                   |            |
//! +-----+------+                                                   +-----+------+
//!       |                                                                ^
//!       | SetOffset                                      SenderFinalize  |
//!       v                                                                |
//! +-----+------+                    +-----------+                  +-----+-----+
//! |            |                    |           |                  |           |
//! | Receiver   |                    | Receiver  |                  | Sender    |
//! | Output     +------------------->+ Partial   +----------------->+ Partial   |
//! | Setup      | SetReceiverOutput  | Signature | SetReceiverNonce | Signing   |
//! |            |                    |           |                  |           |
//! +------------+                    +-----------+                  +-----------+
//!
//! When two remote parties are collaborating the state machine flow will be as follows:
//!
//! Sender's side of the protocol                      Receiver's side of the protocol
//! -----------------------------                      -------------------------------
//!
//! +-------------+                                     +-----------+
//! |             +-------+                             |           |
//! |   Sender    |       | SetFee                      | Sender    |
//! |   Setup     +<------+                             | Setup     |
//! |             |                                     |           |
//! |             +-------+                             +----+------+
//! |             |       | SetLockHeight                    |
//! |             +<------+                    +------------>+ AcceptSenderPublicData
//! |             |                            |             v
//! |             +-------+                    |        +----+------+
//! |             |       | SetAmount          |        |           |
//! |             +<------+                    |        | Receiver  |
//! |             |                            |        | Output    |
//! |             +-------+                    |        | Setup     |
//! |             |       | AddInput           |        |           |
//! |             +<------+                    |        +----+------+
//! |             |                            |             |
//! |             +-------+                    |             | SetReceiverOutput
//! |             |       | AddChangeOutput    |             v
//! |             +<------+                    |        +----+------+
//! |             |                            |        |           |
//! |             +-------+                    |        | Receiver  |
//! |             |       | SetSenderNonce     |        | Partial   |
//! |             +<------+                    |        | Signature |
//! |             |                            |        |           |
//! +-----+-------+                            |        +----+------+
//!       |                                    |             |
//!       | SetOffset                          |             | SetReceiverNonce
//!       v                                    |             v
//! +-----+-------+                            |        +----+------+
//! |             |                            |        |           |
//! | Receiver    |                            |        | Sender    |
//! | Output      +----------------------------+        | Partial   |
//! | Setup       | ConstructSenderPublicData           | Signing   |
//! |             |                                     |           |
//! +-----+-------+                                     +----+------+
//!       |                                                 |
//!       | AcceptReceiverPublicData<-----------------------+ ConstructReceiverPublicData
//!       v
//! +-----+-------+
//! |             |
//! | Sender      |
//! | Partial     |
//! | Signing     |
//! |             |
//! +-----+-------+
//!       |
//!       | SenderFinalize
//!       v
//! +-----+-------+
//! |             |
//! | Finalized   |       +---------+
//! | Transaction +------>+ \o/ Fin |
//! |             |       +---------+
//! +-------------+

use crate::{
    transaction::{TransactionInput, TransactionOutput},
    types::{BlindingFactor, SecretKey, Signature},
};

// use crate::types::PublicKey;
use crate::{
    transaction::{KernelFeatures, Transaction, TransactionBuilder, TransactionError, TransactionKernel},
    types::{CommitmentFactory, PublicKey},
};
use crypto::{
    challenge::Challenge,
    commitment::HomomorphicCommitmentFactory,
    common::Blake256,
    keys::PublicKey as PublicKeyTrait,
    signatures::SchnorrSignatureError,
};
use derive_error::Error;
use tari_utilities::ByteArray;

#[derive(Clone, Debug, PartialEq, Error)]
pub enum TransactionProtocolError {
    // The current state is not yet completed, cannot transition to next state
    IncompleteStateError,
    // Validation of the transaction being built failed, rectify issue and try again
    ValidationError,
    // Invalid state transition
    InvalidTransitionError,
    // Invalid state
    InvalidStateError,
    // An error occurred while performing a signature
    #[error(no_from)]
    SignatureError(SchnorrSignatureError),
    // A signature verification failed
    InvalidSignatureError,
    // An error occurred while building the final transaction
    #[error(no_from)]
    TransactionBuildError(TransactionError),
}

impl From<SchnorrSignatureError> for TransactionProtocolError {
    fn from(e: SchnorrSignatureError) -> TransactionProtocolError {
        TransactionProtocolError::SignatureError(e)
    }
}

impl From<TransactionError> for TransactionProtocolError {
    fn from(e: TransactionError) -> TransactionProtocolError {
        TransactionProtocolError::TransactionBuildError(e)
    }
}

/// TransactionProtocolManager contains the state of the transaction negotiation protocol between two parties
/// a Sender and Receiver. It also implements the public interface for parties to engage in the negotiation protocol
struct TransactionProtocolManager {
    state: TransactionProtocolState,
}

impl TransactionProtocolManager {
    pub fn new() -> TransactionProtocolManager {
        TransactionProtocolManager { state: TransactionProtocolState::SenderSetup(SenderSetup::new()) }
    }

    // TransactionProtocolState::SenderSetup state methods
    // ---------------------------------------------------

    pub fn set_amount(&mut self, amount: u64) -> Result<(), TransactionProtocolError> {
        self.handle_event(TransactionProtocolEvent::SetAmount(amount))
    }

    pub fn set_lock_height(&mut self, lock_height: u64) -> Result<(), TransactionProtocolError> {
        self.handle_event(TransactionProtocolEvent::SetLockHeight(lock_height))
    }

    pub fn set_fee(&mut self, fee: u64) -> Result<(), TransactionProtocolError> {
        self.handle_event(TransactionProtocolEvent::SetFee(fee))
    }

    pub fn add_input(
        &mut self,
        input: &TransactionInput,
        blinding_factor: &BlindingFactor,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(TransactionProtocolEvent::AddInput(input.clone(), blinding_factor.clone()))
    }

    pub fn add_change_output(
        &mut self,
        output: &TransactionOutput,
        blinding_factor: &BlindingFactor,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(TransactionProtocolEvent::AddChangeOutput(output.clone(), blinding_factor.clone()))
    }

    pub fn set_sender_nonce(&mut self, nonce: &SecretKey) -> Result<(), TransactionProtocolError> {
        self.handle_event(TransactionProtocolEvent::SetSenderNonce(nonce.clone()))
    }

    pub fn set_offset(&mut self, offset: &BlindingFactor) -> Result<(), TransactionProtocolError> {
        self.handle_event(TransactionProtocolEvent::SetOffset(offset.clone()))
    }

    // TransactionProtocolState::ReceiverOutputSetup state methods
    // -----------------------------------------------------------

    pub fn set_receiver_output(
        &mut self,
        output: &TransactionOutput,
        blinding_factor: &BlindingFactor,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(TransactionProtocolEvent::SetReceiverOutput(output.clone(), blinding_factor.clone()))
    }

    // TransactionProtocolState::ReceiverPartialSignature state methods
    // ----------------------------------------------------------------

    pub fn set_receiver_nonce(&mut self, nonce: &SecretKey) -> Result<(), TransactionProtocolError> {
        self.handle_event(TransactionProtocolEvent::SetReceiverNonce(nonce.clone()))
    }

    // TransactionProtocolState::SenderPartialSignature state methods
    // --------------------------------------------------------------

    pub fn finalize_signature(&mut self) -> Result<(), TransactionProtocolError> {
        self.handle_event(TransactionProtocolEvent::SenderFinalize)
    }

    // TransactionProtocolState::FinalizedTransaction state methods
    // ------------------------------------------------------------

    pub fn build_final_transaction(&mut self) -> Result<Transaction, TransactionProtocolError> {
        match &self.state {
            TransactionProtocolState::FinalizedTransaction(s) => s.build_transaction(),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    // Inter-party communications handlers to send and receive public data
    // ---------------------------------------------------------------------
    /// The Sender can construct this packet once they have transitioned to the ReceiverOutputSetup state
    pub fn construct_sender_public_data(&self) -> Result<SenderPublicData, TransactionProtocolError> {
        match &self.state {
            TransactionProtocolState::ReceiverOutputSetup(s) => s.construct_sender_public_data(),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    /// The Receiver and accept this data which they are in the initial state and accepting the data will transition
    /// them to the ReceiverOutputSetup state
    pub fn accept_sender_public_data(
        &mut self,
        incoming_data: SenderPublicData,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(TransactionProtocolEvent::AcceptSenderPublicData(incoming_data))
    }

    /// The Receiver can construct this packet once they have transitioned to the SenderPartialSignature state
    pub fn construct_receiver_public_data(&self) -> Result<ReceiverPublicData, TransactionProtocolError> {
        match &self.state {
            TransactionProtocolState::SenderPartialSignature(s) => s.construct_receiver_public_data(),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    /// The Receiver can receive this public data if they are in the ReceiverOutputSetup state and it will transition
    /// them to the SenderPartialSignature state
    pub fn accept_receiver_public_data(
        &mut self,
        incoming_data: ReceiverPublicData,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(TransactionProtocolEvent::AcceptReceiverPublicData(incoming_data))
    }

    // TransactionProtocolState::SenderPartialSignature state query methods
    // --------------------------------------------------------------------

    /// Method to determine if we are in the SenderSetup state
    pub fn is_sender_setup(&self) -> bool {
        match self.state {
            TransactionProtocolState::SenderSetup(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the ReceiverOutputSetup state
    pub fn is_receiver_output_setup(&self) -> bool {
        match self.state {
            TransactionProtocolState::ReceiverOutputSetup(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the ReceiverPartialSignature state
    pub fn is_receiver_partial_signature(&self) -> bool {
        match self.state {
            TransactionProtocolState::ReceiverPartialSignature(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the SenderPartialSignature state
    pub fn is_sender_partial_signature(&self) -> bool {
        match self.state {
            TransactionProtocolState::SenderPartialSignature(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the FinalizedTransaction state
    pub fn is_finalized(&self) -> bool {
        match self.state {
            TransactionProtocolState::FinalizedTransaction(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the Failed state
    pub fn is_failed(&self) -> bool {
        match self.state {
            TransactionProtocolState::Failed(_) => true,
            _ => false,
        }
    }

    /// Method to return the error behind a failure, if one has occured
    pub fn failure_reason(&self) -> Option<TransactionProtocolError> {
        match &self.state {
            TransactionProtocolState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// This function implements the state machine for the Transaction Protocol. Every combination of State and Event
    /// are handled here. The previous state is consumed and dependant on the outcome of processing the event a new
    /// state is returned.
    fn handle_event(&mut self, event: TransactionProtocolEvent) -> Result<(), TransactionProtocolError> {
        self.state = match self.state.clone() {
            // The first state allows the sender to assemble all initial components of the transaction in any order,
            // except for the offset which is the final step. When the offset is set the sender portion of
            // the transaction is validated, if it is not then the sender needs correct the data before
            // setting the offset again. If the transaction is valid the protocol moves to next state.
            TransactionProtocolState::SenderSetup(s) => match event {
                TransactionProtocolEvent::SetAmount(a) => s.set_amount(a),
                TransactionProtocolEvent::SetLockHeight(a) => s.set_lock_height(a),
                TransactionProtocolEvent::SetFee(a) => s.set_fee(a),
                TransactionProtocolEvent::AddInput(i, bf) => s.add_input(i, bf),
                TransactionProtocolEvent::AddChangeOutput(o, bf) => s.add_output(o, bf),
                TransactionProtocolEvent::SetSenderNonce(n) => s.set_nonce(n),
                TransactionProtocolEvent::SetOffset(o) => s.set_offset(o)?,
                TransactionProtocolEvent::AcceptSenderPublicData(d) => s.accept_sender_public_data(d)?,
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            // This state is completed by the receiver. The receiver must add a single output for the amount specified
            // by the sender and choosing their own blinding factor. When this is set the transaction is
            // validated again, if it is valid the protocol moves on to the next state, otherwise the
            // receiver can attempt to set a valid output again.
            TransactionProtocolState::ReceiverOutputSetup(s) => match event {
                TransactionProtocolEvent::SetReceiverOutput(o, bf) => s.set_output(o, bf)?,
                TransactionProtocolEvent::AcceptReceiverPublicData(d) => s.accept_receiver_public_data(d)?,
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            // This state is completed by the Receiver. The Receiver must set a nonce for their signature. If this is
            // done successfully we move on to the next state.
            TransactionProtocolState::ReceiverPartialSignature(s) => match event {
                TransactionProtocolEvent::SetReceiverNonce(n) => s.set_private_nonce(n)?,
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            // This state is completed by the Sender
            TransactionProtocolState::SenderPartialSignature(s) => match event {
                TransactionProtocolEvent::SenderFinalize => s.finalize_signature()?,
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            _ => {
                return Err(TransactionProtocolError::InvalidStateError);
            },
        };

        Ok(())
    }
}

/// This enum contains all the possible events that can occur for this state machine
enum TransactionProtocolEvent {
    SetAmount(u64),
    SetLockHeight(u64),
    SetFee(u64),
    AddInput(TransactionInput, BlindingFactor),
    AddChangeOutput(TransactionOutput, BlindingFactor),
    SetSenderNonce(SecretKey),
    SetOffset(BlindingFactor),
    AcceptSenderPublicData(SenderPublicData),
    SetReceiverOutput(TransactionOutput, BlindingFactor),
    SetReceiverNonce(SecretKey),
    AcceptReceiverPublicData(ReceiverPublicData),
    SenderFinalize,
}

/// This enum contains all the states of the state machine
#[derive(Clone, Debug)]
enum TransactionProtocolState {
    SenderSetup(SenderSetup),
    ReceiverOutputSetup(ReceiverOutputSetup),
    ReceiverPartialSignature(ReceiverPartialSignature),
    SenderPartialSignature(SenderPartialSignature),
    FinalizedTransaction(FinalizedTransaction),
    Failed(TransactionProtocolError),
}

/// This struct contains all the working data is required during the protocol. All fields are
/// options as they are not all required by both parties.
#[derive(Clone, Debug)]
struct TransactionProtocolStateData {
    amount: Option<u64>,
    lock_height: Option<u64>,
    fee: Option<u64>,
    inputs: Vec<TransactionInput>,
    outputs: Vec<TransactionOutput>,
    offset: Option<BlindingFactor>,
    sender_excess_blinding_factor: Option<BlindingFactor>,
    sender_excess: Option<PublicKey>,
    sender_private_nonce: Option<SecretKey>,
    sender_public_nonce: Option<PublicKey>,
    receiver_output_blinding_factor: Option<BlindingFactor>,
    receiver_output_public_key: Option<PublicKey>,
    receiver_output: Option<TransactionOutput>,
    receiver_private_nonce: Option<SecretKey>,
    receiver_public_nonce: Option<PublicKey>,
    receiver_partial_signature: Option<Signature>,
    sender_partial_signature: Option<Signature>,
    final_signature: Option<Signature>,
}

/// This is the message containing the public data that the Sender will send to the Receiver
pub struct SenderPublicData {
    pub fee: u64,
    pub lock_height: u64,
    pub amount: u64,
    pub sender_excess: PublicKey,
    pub sender_public_nonce: PublicKey,
    pub offset: BlindingFactor,
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone)]
pub struct ReceiverPublicData {
    pub receiver_output: TransactionOutput,
    pub receiver_output_public_key: PublicKey,
    pub receiver_public_nonce: PublicKey,
    pub receiver_partial_signature: Signature,
}

/// In this state the Sender will start the protocol by supplying all of the data required from them
/// Once all the data is supplied the final step is for the Sender to select their offset and the state
/// machine will transition to the next state.
/// The Receiver will also start in this state and will accept the public data from the Sender in order to
/// transition to the next state.
#[derive(Clone, Debug)]
struct SenderSetup {
    state_data: TransactionProtocolStateData,
}

impl SenderSetup {
    fn new() -> SenderSetup {
        let state_data = TransactionProtocolStateData {
            amount: None,
            lock_height: None,
            fee: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            offset: None,
            sender_excess_blinding_factor: None,
            sender_excess: None,
            sender_private_nonce: None,
            sender_public_nonce: None,
            receiver_output_blinding_factor: None,
            receiver_output_public_key: None,
            receiver_output: None,
            receiver_private_nonce: None,
            receiver_public_nonce: None,
            receiver_partial_signature: None,
            sender_partial_signature: None,
            final_signature: None,
        };

        SenderSetup { state_data }
    }

    /// This method accepts data from a Sender and transitions into the next state
    /// TODO Check that this is the Receiver and has no Sender state set?
    fn accept_sender_public_data(
        mut self,
        incoming_data: SenderPublicData,
    ) -> Result<TransactionProtocolState, TransactionProtocolError>
    {
        self.state_data.fee = Some(incoming_data.fee);
        self.state_data.lock_height = Some(incoming_data.lock_height);
        self.state_data.amount = Some(incoming_data.amount);
        self.state_data.sender_excess = Some(incoming_data.sender_excess);
        self.state_data.sender_public_nonce = Some(incoming_data.sender_public_nonce);
        self.state_data.offset = Some(incoming_data.offset);

        Ok(TransactionProtocolState::ReceiverOutputSetup(ReceiverOutputSetup::new(self.state_data)))
    }

    fn set_amount(mut self, amount: u64) -> TransactionProtocolState {
        self.state_data.amount = Some(amount);
        TransactionProtocolState::SenderSetup(self)
    }

    fn set_lock_height(mut self, lockheight: u64) -> TransactionProtocolState {
        self.state_data.lock_height = Some(lockheight);
        TransactionProtocolState::SenderSetup(self)
    }

    fn set_fee(mut self, fee: u64) -> TransactionProtocolState {
        self.state_data.fee = Some(fee);
        TransactionProtocolState::SenderSetup(self)
    }

    fn add_input(mut self, input: TransactionInput, blinding_factor: BlindingFactor) -> TransactionProtocolState {
        self.state_data.inputs.push(input);
        self.state_data.sender_excess_blinding_factor =
            Some(self.state_data.sender_excess_blinding_factor.unwrap_or(BlindingFactor::default()) - blinding_factor);
        TransactionProtocolState::SenderSetup(self)
    }

    fn add_output(mut self, output: TransactionOutput, blinding_factor: BlindingFactor) -> TransactionProtocolState {
        self.state_data.outputs.push(output);
        self.state_data.sender_excess_blinding_factor =
            Some(self.state_data.sender_excess_blinding_factor.unwrap_or(BlindingFactor::default()) + blinding_factor);
        TransactionProtocolState::SenderSetup(self)
    }

    fn set_nonce(mut self, nonce: SecretKey) -> TransactionProtocolState {
        self.state_data.sender_private_nonce = Some(nonce.clone());
        self.state_data.sender_public_nonce = Some(PublicKey::from_secret_key(&nonce));
        TransactionProtocolState::SenderSetup(self)
    }

    /// This is the final call you make in this state that will transition you to the next state
    fn set_offset(mut self, offset: BlindingFactor) -> Result<TransactionProtocolState, TransactionProtocolError> {
        // Validate the current state to check we can proceed to the next state
        if self.state_data.amount.is_none() ||
            self.state_data.lock_height.is_none() ||
            self.state_data.fee.is_none() ||
            self.state_data.offset.is_some() ||
            self.state_data.inputs.len() == 0 ||
            self.state_data.sender_private_nonce.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        // Validate that inputs, outputs, fees and amount balance
        let mut sum = &CommitmentFactory::create(&SecretKey::default(), &SecretKey::from(self.state_data.fee.unwrap())) +
            &CommitmentFactory::create(&SecretKey::default(), &SecretKey::from(self.state_data.amount.unwrap()));
        for o in self.state_data.outputs.clone() {
            sum = &sum + &o.commitment;
        }
        for i in self.state_data.inputs.clone() {
            sum = &sum - &i.commitment;
        }
        sum = &sum -
            &CommitmentFactory::create(
                &self.state_data.sender_excess_blinding_factor.unwrap().into(),
                &SecretKey::default(),
            );

        if sum != CommitmentFactory::create(&SecretKey::default(), &SecretKey::default()) {
            return Err(TransactionProtocolError::ValidationError);
        }

        // If validation passes, select offset and move on to next state
        self.state_data.sender_excess_blinding_factor =
            Some(&self.state_data.sender_excess_blinding_factor.unwrap() - offset);
        self.state_data.offset = Some(offset);
        self.state_data.sender_excess =
            Some(PublicKey::from_secret_key(&self.state_data.sender_excess_blinding_factor.unwrap()));

        Ok(TransactionProtocolState::ReceiverOutputSetup(ReceiverOutputSetup::new(self.state_data)))
    }
}

/// In this state the Receiver will provide the data for their receiving output which will transition them to the next
/// state.
/// In this state the Sender can construct their public data message to be sent to the Receiver AND the Sender will wait
/// in this state to accept the public data from the Receiver to advance to the next state.
#[derive(Clone, Debug)]
struct ReceiverOutputSetup {
    state_data: TransactionProtocolStateData,
}

impl ReceiverOutputSetup {
    fn new(previous_state_data: TransactionProtocolStateData) -> ReceiverOutputSetup {
        ReceiverOutputSetup { state_data: previous_state_data }
    }

    /// The Sender will accept the public data from the Receiver in order to transition to the next state.
    fn accept_receiver_public_data(
        mut self,
        incoming_data: ReceiverPublicData,
    ) -> Result<TransactionProtocolState, TransactionProtocolError>
    {
        self.state_data.receiver_output_public_key = Some(incoming_data.receiver_output_public_key);
        self.state_data.receiver_public_nonce = Some(incoming_data.receiver_public_nonce);
        self.state_data.receiver_partial_signature = Some(incoming_data.receiver_partial_signature);
        self.state_data.receiver_output = Some(incoming_data.receiver_output);

        // Validate that inputs, outputs, fees and amount balance
        let mut sum = CommitmentFactory::create(&SecretKey::default(), &SecretKey::from(self.state_data.fee.unwrap()));

        for o in self.state_data.outputs.clone() {
            sum = &sum + &o.commitment;
        }
        sum = &sum + &incoming_data.receiver_output.commitment;

        for i in self.state_data.inputs.clone() {
            sum = &sum - &i.commitment;
        }
        sum = CommitmentFactory::from_public_key(&(sum.as_public_key() - &self.state_data.sender_excess.unwrap()));
        sum = &sum - &CommitmentFactory::create(&self.state_data.offset.unwrap().into(), &SecretKey::default());
        sum = &sum - &CommitmentFactory::from_public_key(&incoming_data.receiver_output_public_key);

        if sum != CommitmentFactory::create(&SecretKey::default(), &SecretKey::default()) {
            return Err(TransactionProtocolError::ValidationError);
        }
        // If it all checks out then add the receiver outputs to the other outputs.
        self.state_data.outputs.push(incoming_data.receiver_output);

        Ok(TransactionProtocolState::SenderPartialSignature(SenderPartialSignature::new(self.state_data)))
    }

    /// The Sender will use this method to construct the public data message they will send to the Receiver
    fn construct_sender_public_data(&self) -> Result<SenderPublicData, TransactionProtocolError> {
        // Validate the current state to check we have the data we need
        if self.state_data.amount.is_none() ||
            self.state_data.lock_height.is_none() ||
            self.state_data.fee.is_none() ||
            self.state_data.offset.is_none() ||
            self.state_data.sender_public_nonce.is_none() ||
            self.state_data.sender_excess.is_none() ||
            self.state_data.inputs.len() == 0
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        Ok(SenderPublicData {
            fee: self.state_data.fee.unwrap(),
            lock_height: self.state_data.lock_height.unwrap(),
            amount: self.state_data.amount.unwrap(),
            sender_excess: self.state_data.sender_excess.unwrap(),
            sender_public_nonce: self.state_data.sender_public_nonce.unwrap(),
            offset: self.state_data.offset.unwrap(),
        })
    }

    fn set_output(
        mut self,
        output: TransactionOutput,
        blinding_factor: BlindingFactor,
    ) -> Result<TransactionProtocolState, TransactionProtocolError>
    {
        self.state_data.receiver_output = Some(output);
        self.state_data.receiver_output_blinding_factor = Some(blinding_factor);
        self.state_data.receiver_output_public_key = Some(PublicKey::from_secret_key(&blinding_factor));

        Ok(TransactionProtocolState::ReceiverPartialSignature(ReceiverPartialSignature::new(self.state_data)))
    }
}

/// In this state the Receiver is ready to provide the data required for them to construct their partial signature
#[derive(Clone, Debug)]
struct ReceiverPartialSignature {
    state_data: TransactionProtocolStateData,
}

impl ReceiverPartialSignature {
    fn new(previous_state_data: TransactionProtocolStateData) -> ReceiverPartialSignature {
        ReceiverPartialSignature { state_data: previous_state_data }
    }

    fn set_private_nonce(mut self, nonce: SecretKey) -> Result<TransactionProtocolState, TransactionProtocolError> {
        // Validate that all the required state is present.
        if self.state_data.sender_public_nonce.is_none() ||
            self.state_data.sender_excess.is_none() ||
            self.state_data.receiver_output_public_key.is_none() ||
            self.state_data.fee.is_none() ||
            self.state_data.lock_height.is_none() ||
            self.state_data.receiver_output_blinding_factor.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        self.state_data.receiver_private_nonce = Some(nonce.clone());
        self.state_data.receiver_public_nonce = Some(PublicKey::from_secret_key(&nonce));

        let challenge = Challenge::<Blake256>::new()
            .concat(
                (&self.state_data.sender_public_nonce.unwrap() + &self.state_data.receiver_public_nonce.unwrap())
                    .as_bytes(),
            )
            .concat(
                (&self.state_data.sender_excess.unwrap() + &self.state_data.receiver_output_public_key.unwrap())
                    .as_bytes(),
            )
            .concat(&self.state_data.fee.unwrap().to_le_bytes())
            .concat(&self.state_data.lock_height.unwrap().to_le_bytes());

        self.state_data.receiver_partial_signature = Some(Signature::sign(
            self.state_data.receiver_output_blinding_factor.unwrap(),
            self.state_data.receiver_private_nonce.unwrap(),
            challenge.clone(),
        )?);

        Ok(TransactionProtocolState::SenderPartialSignature(SenderPartialSignature::new(self.state_data)))
    }
}

/// In this state the Sender can now construct their partial signature and the final aggregated signature.
/// Also in this state the Receiver can construct the message containing their public data to be sent back to the Sender
#[derive(Clone, Debug)]
struct SenderPartialSignature {
    state_data: TransactionProtocolStateData,
}

impl SenderPartialSignature {
    fn new(previous_state_data: TransactionProtocolStateData) -> SenderPartialSignature {
        SenderPartialSignature { state_data: previous_state_data }
    }

    fn construct_receiver_public_data(&self) -> Result<ReceiverPublicData, TransactionProtocolError> {
        // Validate the current state to check we have the data we need
        if self.state_data.receiver_output_public_key.is_none() ||
            self.state_data.receiver_public_nonce.is_none() ||
            self.state_data.receiver_partial_signature.is_none() ||
            self.state_data.receiver_output.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        Ok(ReceiverPublicData {
            receiver_output: self.state_data.receiver_output.unwrap(),
            receiver_output_public_key: self.state_data.receiver_output_public_key.unwrap(),
            receiver_public_nonce: self.state_data.receiver_public_nonce.unwrap(),
            receiver_partial_signature: self.state_data.receiver_partial_signature.unwrap(),
        })
    }

    fn finalize_signature(mut self) -> Result<TransactionProtocolState, TransactionProtocolError> {
        // Validate that all the required state is present.
        if self.state_data.sender_public_nonce.is_none() ||
            self.state_data.sender_private_nonce.is_none() ||
            self.state_data.sender_excess.is_none() ||
            self.state_data.receiver_output_public_key.is_none() ||
            self.state_data.fee.is_none() ||
            self.state_data.lock_height.is_none() ||
            self.state_data.receiver_public_nonce.is_none() ||
            self.state_data.receiver_partial_signature.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        let challenge = Challenge::<Blake256>::new()
            .concat(
                (&self.state_data.sender_public_nonce.unwrap() + &self.state_data.receiver_public_nonce.unwrap())
                    .as_bytes(),
            )
            .concat(
                (&self.state_data.sender_excess.unwrap() + &self.state_data.receiver_output_public_key.unwrap())
                    .as_bytes(),
            )
            .concat(&self.state_data.fee.unwrap().to_le_bytes())
            .concat(&self.state_data.lock_height.unwrap().to_le_bytes());

        // Verify the receivers partial signature
        if !self
            .state_data
            .receiver_partial_signature
            .unwrap()
            .verify_challenge(&self.state_data.receiver_output_public_key.unwrap(), challenge.clone())
        {
            return Err(TransactionProtocolError::InvalidSignatureError);
        }

        self.state_data.sender_partial_signature = Some(Signature::sign(
            self.state_data.sender_excess_blinding_factor.unwrap(),
            self.state_data.sender_private_nonce.unwrap(),
            challenge.clone(),
        )?);

        self.state_data.final_signature = Some(
            &self.state_data.sender_partial_signature.unwrap() + &self.state_data.receiver_partial_signature.unwrap(),
        );

        // Validate final signature
        if !self.state_data.final_signature.unwrap().verify_challenge(
            &(&self.state_data.receiver_output_public_key.unwrap() + &self.state_data.sender_excess.unwrap()),
            challenge.clone(),
        ) {
            return Err(TransactionProtocolError::InvalidSignatureError);
        }

        Ok(TransactionProtocolState::FinalizedTransaction(FinalizedTransaction::new(self.state_data)))
    }
}

/// In this state the transaction has been finalized and validated. The final transaction can now be built.
#[derive(Clone, Debug)]
struct FinalizedTransaction {
    state_data: TransactionProtocolStateData,
}

impl FinalizedTransaction {
    fn new(previous_state_data: TransactionProtocolStateData) -> FinalizedTransaction {
        FinalizedTransaction { state_data: previous_state_data }
    }

    fn build_transaction(&self) -> Result<Transaction, TransactionProtocolError> {
        // Validate that all the data that is required is present
        if self.state_data.amount.is_none() ||
            self.state_data.lock_height.is_none() ||
            self.state_data.fee.is_none() ||
            self.state_data.inputs.len() == 0 ||
            self.state_data.outputs.len() == 0 ||
            self.state_data.offset.is_none() ||
            self.state_data.receiver_public_nonce.is_none() ||
            self.state_data.sender_public_nonce.is_none() ||
            self.state_data.receiver_output_public_key.is_none() ||
            self.state_data.sender_excess.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        let mut tx_builder = TransactionBuilder::new();

        for i in &self.state_data.inputs {
            tx_builder.add_input(i.clone());
        }

        for o in &self.state_data.outputs {
            tx_builder.add_output(o.clone());
        }

        tx_builder.add_offset(self.state_data.offset.unwrap());
        tx_builder.with_kernel(TransactionKernel {
            features: KernelFeatures::empty(),
            fee: self.state_data.fee.unwrap(),
            lock_height: self.state_data.lock_height.unwrap(),
            excess: Some(CommitmentFactory::from_public_key(
                &(&self.state_data.receiver_output_public_key.unwrap() + &self.state_data.sender_excess.unwrap()),
            )),
            excess_sig: self.state_data.final_signature,
        });

        let tx = tx_builder.build()?;

        Ok(tx)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        range_proof::RangeProof,
        transaction::{OutputFeatures, TransactionInput, TransactionOutput},
        transaction_protocol_manager::{TransactionProtocolError, TransactionProtocolManager},
        types::{BlindingFactor, CommitmentFactory, SecretKey},
    };
    use crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey as SecretKeyTrait};

    #[test]
    fn transaction_protocol_test() {
        let mut rng = rand::OsRng::new().unwrap();

        let input_secret_key = SecretKey::random(&mut rng);
        let input2_secret_key = SecretKey::random(&mut rng);
        let change_secret_key = SecretKey::random(&mut rng);
        let change2_secret_key = SecretKey::random(&mut rng);
        let receiver_secret_key = SecretKey::random(&mut rng);
        let sender_private_nonce = SecretKey::random(&mut rng);
        let receiver_private_nonce = SecretKey::random(&mut rng);
        let offset: BlindingFactor = BlindingFactor::random(&mut rng).into();

        let input1_val = 16u64;
        let input2_val = 9u64;
        let change1_val = 2u64;
        let change2_val = 3u64;
        let fee = 1u64;
        let amount = input1_val + input2_val - change1_val - change2_val - fee;

        let mut sender_tx_protocol_manager = TransactionProtocolManager::new();
        sender_tx_protocol_manager.set_amount(amount).unwrap();
        sender_tx_protocol_manager.set_fee(fee).unwrap();
        sender_tx_protocol_manager.set_lock_height(0u64).unwrap();
        sender_tx_protocol_manager
            .add_input(
                &TransactionInput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&input_secret_key.into(), &SecretKey::from(input1_val)),
                ),
                &input_secret_key,
            )
            .unwrap();
        sender_tx_protocol_manager
            .add_input(
                &TransactionInput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&input2_secret_key.into(), &SecretKey::from(input2_val)),
                ),
                &input2_secret_key,
            )
            .unwrap();
        sender_tx_protocol_manager
            .add_change_output(
                &TransactionOutput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&change_secret_key.into(), &SecretKey::from(change1_val)),
                    RangeProof([0; 1]),
                ),
                &change_secret_key,
            )
            .unwrap();

        // Attempt to set the offset before setting the nonce
        assert_eq!(sender_tx_protocol_manager.set_offset(&offset), Err(TransactionProtocolError::IncompleteStateError));

        sender_tx_protocol_manager.set_sender_nonce(&sender_private_nonce).unwrap();

        // Attempt to set the offset while the commitments don't balance
        assert_eq!(sender_tx_protocol_manager.set_offset(&offset), Err(TransactionProtocolError::ValidationError));

        sender_tx_protocol_manager
            .add_change_output(
                &TransactionOutput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&change2_secret_key.into(), &SecretKey::from(change2_val)),
                    RangeProof([0; 1]),
                ),
                &change2_secret_key,
            )
            .unwrap();

        sender_tx_protocol_manager.set_offset(&offset).unwrap();
        assert!(sender_tx_protocol_manager.is_receiver_output_setup());

        // We are now in TransactionProtocolState::ReceiverOutputSetup, lets try call a
        // TransactionProtocolState::SenderSetup event
        assert_eq!(sender_tx_protocol_manager.set_fee(4u64), Err(TransactionProtocolError::InvalidTransitionError));

        // The Sender now constructs a SenderPublicData message to send to the Receiver and we continue in their
        // protocol manager
        let sender_public_data = sender_tx_protocol_manager.construct_sender_public_data().unwrap();

        // Creating a receiver protocol manager
        let mut receiver_tx_protocol_manager = TransactionProtocolManager::new();
        receiver_tx_protocol_manager.accept_sender_public_data(sender_public_data).unwrap();
        assert!(receiver_tx_protocol_manager.is_receiver_output_setup());
        receiver_tx_protocol_manager
            .set_receiver_output(
                &TransactionOutput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&receiver_secret_key.into(), &SecretKey::from(amount)),
                    RangeProof([0; 1]),
                ),
                &receiver_secret_key,
            )
            .unwrap();

        receiver_tx_protocol_manager.set_receiver_nonce(&receiver_private_nonce).unwrap();
        assert!(receiver_tx_protocol_manager.is_sender_partial_signature());

        // The receiver now constructs their public data message to send back to the sender
        let receiver_public_data = receiver_tx_protocol_manager.construct_receiver_public_data().unwrap();

        // Lets try finalize the signature without accepting the receiver's public data
        assert_eq!(
            sender_tx_protocol_manager.finalize_signature(),
            Err(TransactionProtocolError::InvalidTransitionError)
        );

        // Lets try accept receiver data with an incorrect output amount (same secret key)
        let mut incorrect_receiver_public_data = receiver_public_data.clone();
        incorrect_receiver_public_data.receiver_output = TransactionOutput::new(
            OutputFeatures::empty(),
            CommitmentFactory::create(&receiver_secret_key.into(), &SecretKey::from(amount + 1)),
            RangeProof([0; 1]),
        );
        assert_eq!(
            sender_tx_protocol_manager.accept_receiver_public_data(incorrect_receiver_public_data),
            Err(TransactionProtocolError::ValidationError)
        );
        assert!(!sender_tx_protocol_manager.is_sender_partial_signature());

        sender_tx_protocol_manager.accept_receiver_public_data(receiver_public_data).unwrap();
        assert!(sender_tx_protocol_manager.is_sender_partial_signature());
        sender_tx_protocol_manager.finalize_signature().unwrap();

        let final_tx = sender_tx_protocol_manager.build_final_transaction().unwrap();
        final_tx.validate().unwrap();
    }
}
