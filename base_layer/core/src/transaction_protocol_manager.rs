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

//! Transaction Protocol Manager facilitates the process of constructing a Mimblewimble transaction between two parties.
//!
//! The Transaction Protocol Manager implements a protocol to construct a Mimwblewimble transaction between two parties
//! , a Sender and a Receiver. In this transaction the Sender is paying the Receiver from their inputs and also paying
//! to as many change outputs as they like. The Receiver will receive a single output from this transaction.
//! The module consists of three main components:
//! - A Builder for the initial Sender state data
//! - A SenderTransactionProtocolManager which manages the Sender's state machine
//! - A ReceiverTransactionProtocolManager which manages the Receiver's state machine.
//!
//! The two state machines run in parallel and will be managed by each respective party. Each state machine has methods
//! to construct and accept the public data messages that needs to be transmitted between the parties. The diagram below
//! illustrates the progression of the two state machines and shows where the public data messages are constructed and
//! accepted in each state machine
//!
//! ```plaintext
//!                   Sender's State Machine                    Receiver's State Machine
//!                   ----------------------                    ------------------------
//!
//!                   +----------------+                        +------------------+
//!                   |                |                        |                  |
//!                   |  Sender        |                        | Receiver         |
//!                   |  Init          |                        | Init             |
//!                   |                |                        |                  |
//!                   +----------------+                        +------------------+
//!                          |                                         |
//!                          | AcceptSenderInitalState   ------------->| AcceptSenderPublicData
//!                          v                           |             v
//!                   +----------------+                 |      +------------------+
//!                   |                |                 |      |                  |
//!                   | Waiting for    |                 |      | Receiver         |
//!                   | Receiver       |-----------------+      | Output           |
//!                   | Output         | ConstructSender        | Setup            |
//!                   |                | PublicData             |                  |
//!                   +----------------+                        +------------------+
//!                          |                                         |
//! AcceptReceiverPublicData |<----------------                        | SetReceiverOutput
//!                          v                |                        v
//!                   +----------------+      |                 +------------------+
//!                   |                |      |                 |                  |
//!                   | Sender Partial |      |                 | Receiver Partial |
//!                   | Signature      |      |                 | Signature        |
//!                   | Creation       |      |                 | Creation         |
//!                   |                |      |                 |                  |
//!                   +----------------+      |                 +------------------+
//!                          |                |                        |
//!                          | SenderFinalize |                        | SetReceiverNonce
//!                          v                |                        v
//!                   +----------------+      |                 +------------------+
//!                   |                |      |                 |                  |
//!                   | Sender         |      |                 | Receiver         |
//!                   | Finalized      |      +-----------------| Completed        |
//!                   | Transaction    |      ConstructReceiver |                  |
//!                   |                |      PublicData        +------------------+
//!                   +----------------+
//!                           |
//!                           |
//!                           v
//!                      +-----------+
//!                      |Completed  |
//!                      |TX! \o/    |
//!                      +-----------+
//! ```

use crate::{
    transaction::{TransactionInput, TransactionOutput},
    types::{BlindingFactor, SecretKey, Signature},
};

use crate::{
    transaction::{KernelFeatures, Transaction, TransactionBuilder, TransactionError, TransactionKernel},
    types::{CommitmentFactory, PublicKey, SignatureHash},
};
use crypto::{
    challenge::Challenge,
    commitment::HomomorphicCommitmentFactory,
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
    SigningError(SchnorrSignatureError),
    // A signature verification failed
    InvalidSignatureError,
    // An error occurred while building the final transaction
    TransactionBuildError(TransactionError),
}

/// SenderTransactionProtocolManager contains the state of the Sender side of the transaction negotiation protocol
/// between two parties a Sender and Receiver.
struct SenderTransactionProtocolManager {
    state: SenderState,
}

impl SenderTransactionProtocolManager {
    pub fn new() -> SenderTransactionProtocolManager {
        SenderTransactionProtocolManager {
            state: SenderState::SenderInit(SenderInit::new()),
        }
    }

    // SenderState::SenderInit state methods
    // -------------------------------------
    pub fn accept_sender_initial_state(
        &mut self,
        sender_state: SenderStateData,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(SenderEvent::AcceptSenderInitialState(sender_state))
    }

    // SenderState::WaitingForReceiverOutput state methods
    // ---------------------------------------------------

    pub fn construct_sender_public_data(&self) -> Result<SenderPublicData, TransactionProtocolError> {
        match &self.state {
            SenderState::WaitingForReceiverOutput(s) => s.construct_sender_public_data(),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    pub fn accept_receiver_public_data(
        &mut self,
        receiver_data: ReceiverPublicData,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(SenderEvent::AcceptReceiverPublicData(receiver_data))
    }

    // SenderState::SenderPartialSignatureCreation state methods
    // ---------------------------------------------------------

    pub fn finalize_signature(&mut self) -> Result<(), TransactionProtocolError> {
        self.handle_event(SenderEvent::SenderFinalize)
    }

    // SenderState::Finalized state methods
    // ------------------------------------

    pub fn build_final_transaction(&mut self) -> Result<Transaction, TransactionProtocolError> {
        match &self.state {
            SenderState::FinalizedTransaction(s) => s.build_transaction(),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    // SenderState state query methods
    // --------------------------------------------------------------------

    /// Method to determine if we are in the SenderState::SenderInit state
    pub fn is_sender_init(&self) -> bool {
        match self.state {
            SenderState::SenderInit(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the SenderState::WaitingForReceiverOutput state
    pub fn is_sender_waiting_for_receiver_output(&self) -> bool {
        match self.state {
            SenderState::WaitingForReceiverOutput(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the SenderState::SenderPartialSignatureCreation state
    pub fn is_sender_partial_signature_creation(&self) -> bool {
        match self.state {
            SenderState::SenderPartialSignatureCreation(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the SenderState::FinalizedTransaction state
    pub fn is_finalized(&self) -> bool {
        match self.state {
            SenderState::FinalizedTransaction(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the SenderState::Failed state
    pub fn is_failed(&self) -> bool {
        match self.state {
            SenderState::Failed(_) => true,
            _ => false,
        }
    }

    /// Method to return the error behind a failure, if one has occurred
    pub fn failure_reason(&self) -> Option<TransactionProtocolError> {
        match &self.state {
            SenderState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// This function implements the state machine for the Sender part of the Transaction Protocol. Every combination
    /// of State and Event are handled here. The previous state is consumed and dependant on the outcome of
    /// processing the event a new state is returned.
    fn handle_event(&mut self, event: SenderEvent) -> Result<(), TransactionProtocolError> {
        self.state = match self.state.clone() {
            SenderState::SenderInit(s) => match event {
                SenderEvent::AcceptSenderInitialState(d) => s.accept_initial_state(d)?,
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            SenderState::WaitingForReceiverOutput(s) => match event {
                SenderEvent::AcceptReceiverPublicData(d) => s.accept_receiver_public_data(d),
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            SenderState::SenderPartialSignatureCreation(s) => match event {
                SenderEvent::SenderFinalize => s.finalize_signature()?,
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

/// ReceiverTransactionProtocolManager contains the state of the Receiver side of the transaction negotiation protocol
/// between two parties a Sender and Receiver.
struct ReceiverTransactionProtocolManager {
    state: ReceiverState,
}

impl ReceiverTransactionProtocolManager {
    pub fn new() -> ReceiverTransactionProtocolManager {
        ReceiverTransactionProtocolManager {
            state: ReceiverState::ReceiverInit(ReceiverInit::new()),
        }
    }

    // ReceiverState::ReceiverInit state methods
    // -----------------------------------------
    pub fn accept_sender_public_data(&mut self, sender_data: SenderPublicData) -> Result<(), TransactionProtocolError> {
        self.handle_event(ReceiverEvent::AcceptSenderPublicData(sender_data))
    }

    // ReceiverState::ReceiverOutputSetup state methods
    // ------------------------------------------------

    pub fn set_receiver_output(
        &mut self,
        output: TransactionOutput,
        blinding_factor: BlindingFactor,
    ) -> Result<(), TransactionProtocolError>
    {
        self.handle_event(ReceiverEvent::SetReceiverOutput(output, blinding_factor))
    }

    // ReceiverState::ReceiverPartialSignatureCreation state methods
    // -------------------------------------------------------------

    pub fn set_receiver_nonce(&mut self, nonce: SecretKey) -> Result<(), TransactionProtocolError> {
        self.handle_event(ReceiverEvent::SetReceiverNonce(nonce))
    }

    // ReceiverState::ReceiverCompleted state methods
    // ----------------------------------------------
    pub fn construct_receiver_public_data(&self) -> Result<ReceiverPublicData, TransactionProtocolError> {
        match &self.state {
            ReceiverState::ReceiverCompleted(s) => s.construct_receiver_public_data(),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    // ReceiverState state query methods
    // ---------------------------------

    /// Method to determine if we are in the ReceiverState::ReceiverInit state
    pub fn is_receiver_init(&self) -> bool {
        match self.state {
            ReceiverState::ReceiverInit(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the ReceiverState::ReceiverOutputSetup state
    pub fn is_receiver_output_setup(&self) -> bool {
        match self.state {
            ReceiverState::ReceiverOutputSetup(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the ReceiverState::ReceiverPartialSignatureCreation state
    pub fn is_receiver_partial_signature_creation(&self) -> bool {
        match self.state {
            ReceiverState::ReceiverPartialSignatureCreation(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the ReceiverState::ReceiverCompleted state
    pub fn is_completed(&self) -> bool {
        match self.state {
            ReceiverState::ReceiverCompleted(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the ReceiverState::Failed state
    pub fn is_failed(&self) -> bool {
        match self.state {
            ReceiverState::Failed(_) => true,
            _ => false,
        }
    }

    /// Method to return the error behind a failure, if one has occured
    pub fn failure_reason(&self) -> Option<TransactionProtocolError> {
        match &self.state {
            ReceiverState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// This function implements the state machine for the Receiver side of the Transaction Protocol. Every combination
    /// of State and Event are handled here. The previous state is consumed and dependant on the outcome of processing
    /// the event a new state is returned.
    fn handle_event(&mut self, event: ReceiverEvent) -> Result<(), TransactionProtocolError> {
        self.state = match self.state.clone() {
            ReceiverState::ReceiverInit(s) => match event {
                ReceiverEvent::AcceptSenderPublicData(d) => s.accept_initial_sender_public_data(d),
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            ReceiverState::ReceiverOutputSetup(s) => match event {
                ReceiverEvent::SetReceiverOutput(o, bf) => s.set_receiver_output(o, bf)?,
                _ => {
                    return Err(TransactionProtocolError::InvalidTransitionError);
                },
            },
            ReceiverState::ReceiverPartialSignatureCreation(s) => match event {
                ReceiverEvent::SetReceiverNonce(n) => s.set_private_nonce(n)?,
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

/// This enum contains all the possible events that can occur for the Sender state machine
enum SenderEvent {
    AcceptSenderInitialState(SenderStateData),
    AcceptReceiverPublicData(ReceiverPublicData),
    SenderFinalize,
}

/// This enum contains all the possible events that can occur for the Receiver state machine
enum ReceiverEvent {
    AcceptSenderPublicData(SenderPublicData),
    SetReceiverOutput(TransactionOutput, BlindingFactor),
    SetReceiverNonce(SecretKey),
}

/// This enum contains all the states of the Sender state machine
#[derive(Clone, Debug)]
enum SenderState {
    SenderInit(SenderInit),
    WaitingForReceiverOutput(WaitingForReceiverOutput),
    SenderPartialSignatureCreation(SenderPartialSignatureCreation),
    FinalizedTransaction(FinalizedTransaction),
    Failed(TransactionProtocolError),
}

/// This enum contains all the states of the Receiver state machine
#[derive(Clone, Debug)]
enum ReceiverState {
    ReceiverInit(ReceiverInit),
    ReceiverOutputSetup(ReceiverOutputSetup),
    ReceiverPartialSignatureCreation(ReceiverPartialSignatureCreation),
    ReceiverCompleted(ReceiverCompleted),
    Failed(TransactionProtocolError),
}

/// This struct contains all the working data that is required during the protocol for the Sender.
#[derive(Clone, Debug, PartialEq)]
pub struct SenderStateData {
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
    receiver_output_public_key: Option<PublicKey>,
    receiver_public_nonce: Option<PublicKey>,
    receiver_partial_signature: Option<Signature>,
    sender_partial_signature: Option<Signature>,
    final_signature: Option<Signature>,
}

impl SenderStateData {
    /// This function validates the contents of the SenderStateData structure. It can validate at two stages of
    /// completion Firstly, if only the Sender's data is completed it will validate by creating a output for the
    /// specified amount Secondly, if the Receiver data is present it will use the provided Receiver output and
    /// validate the Receiver's Partial Signature
    pub fn validate(&self) -> Result<(), TransactionProtocolError> {
        if self.amount.is_none() ||
            self.lock_height.is_none() ||
            self.fee.is_none() ||
            self.offset.is_none() ||
            self.inputs.len() == 0 ||
            self.sender_private_nonce.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        // Validate that inputs, outputs, fees and amount balance
        let mut sum = CommitmentFactory::create(&SecretKey::default(), &SecretKey::from(self.fee.unwrap()));
        if self.receiver_output_public_key.is_none() {
            sum = &sum + &CommitmentFactory::create(&SecretKey::default(), &SecretKey::from(self.amount.unwrap()));
        } else {
            // If the receiver output has been added we validate that instead of constructing a commitment from the
            // stated amount
            sum = &sum - &CommitmentFactory::from_public_key(&self.receiver_output_public_key.unwrap());
        }

        for o in &self.outputs {
            sum = &sum + &o.commitment;
        }
        for i in &self.inputs {
            sum = &sum - &i.commitment;
        }

        sum = &sum -
            &CommitmentFactory::create(
                &self.sender_excess_blinding_factor.unwrap().into(),
                &SecretKey::default(),
            );
        sum = &sum - &CommitmentFactory::create(&self.offset.unwrap().into(), &SecretKey::default());

        if sum != CommitmentFactory::create(&SecretKey::default(), &SecretKey::default()) {
            return Err(TransactionProtocolError::ValidationError);
        }

        // If the receiver partial signature is present it can be validated
        if self.receiver_partial_signature.is_some() &&
            self.sender_public_nonce.is_some() &&
            self.sender_excess.is_some() &&
            self.receiver_public_nonce.is_some() &&
            self.receiver_output_public_key.is_some()
        {
            let challenge = calculate_challenge(
                &self.sender_public_nonce.unwrap(),
                &self.receiver_public_nonce.unwrap(),
                &self.sender_excess.unwrap(),
                &self.receiver_output_public_key.unwrap(),
                self.fee.unwrap(),
                self.lock_height.unwrap(),
            );

            if !self
                .receiver_partial_signature
                .unwrap()
                .verify_challenge(&self.receiver_output_public_key.unwrap(), challenge)
            {
                return Err(TransactionProtocolError::InvalidSignatureError);
            }
        }

        Ok(())
    }
}

/// This struct contains all the working data that is required by a Receiver during the protocol.
#[derive(Clone, Debug)]
struct ReceiverStateData {
    receiver_output_blinding_factor: Option<BlindingFactor>,
    receiver_output_public_key: Option<PublicKey>,
    receiver_output: Option<TransactionOutput>,
    receiver_private_nonce: Option<SecretKey>,
    receiver_public_nonce: Option<PublicKey>,
    receiver_partial_signature: Option<Signature>,
}

impl ReceiverStateData {
    fn new() -> ReceiverStateData {
        ReceiverStateData {
            receiver_output_blinding_factor: None,
            receiver_output_public_key: None,
            receiver_output: None,
            receiver_private_nonce: None,
            receiver_public_nonce: None,
            receiver_partial_signature: None,
        }
    }
}

/// This is the message containing the public data that the Sender will send to the Receiver
#[derive(Clone, Debug)]
pub struct SenderPublicData {
    pub fee: u64,
    pub lock_height: u64,
    pub amount: u64,
    pub sender_excess: PublicKey,
    pub sender_public_nonce: PublicKey,
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone)]
pub struct ReceiverPublicData {
    pub receiver_output: TransactionOutput,
    pub receiver_output_public_key: PublicKey,
    pub receiver_public_nonce: PublicKey,
    pub receiver_partial_signature: Signature,
}

// -------------------------------------- Sender States --------------------------------------------
/// This is the starting state for the Sender, this state waits until it receives a completely constructed
/// SenderStateData (constructed using the Builder) and then moves on to the next state.
#[derive(Clone, Debug)]
struct SenderInit {}

impl SenderInit {
    fn new() -> Self {
        SenderInit {}
    }

    fn accept_initial_state(self, sender_state: SenderStateData) -> Result<SenderState, TransactionProtocolError> {
        sender_state.validate()?;

        Ok(SenderState::WaitingForReceiverOutput(WaitingForReceiverOutput::new(
            sender_state,
        )))
    }
}

/// In this state the Sender is able to construct the message containing the public data to be sent to the Receiver,
/// the state waits in this state until it receives the Receiver's public data in return before moving to the next
/// state.
#[derive(Clone, Debug)]
struct WaitingForReceiverOutput {
    state_data: SenderStateData,
}

impl WaitingForReceiverOutput {
    fn new(state_data: SenderStateData) -> WaitingForReceiverOutput {
        WaitingForReceiverOutput { state_data }
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
        })
    }

    fn accept_receiver_public_data(mut self, receiver_data: ReceiverPublicData) -> SenderState {
        self.state_data.receiver_output_public_key = Some(receiver_data.receiver_output_public_key);
        self.state_data.receiver_public_nonce = Some(receiver_data.receiver_public_nonce);
        self.state_data.receiver_partial_signature = Some(receiver_data.receiver_partial_signature);
        self.state_data.outputs.push(receiver_data.receiver_output);

        match self.state_data.validate() {
            Ok(()) => SenderState::SenderPartialSignatureCreation(SenderPartialSignatureCreation::new(self.state_data)),
            Err(err) => SenderState::Failed(err),
        }
    }
}

/// In this state the Sender can now construct their partial signature and the final aggregated signature.
#[derive(Clone, Debug)]
struct SenderPartialSignatureCreation {
    state_data: SenderStateData,
}

impl SenderPartialSignatureCreation {
    fn new(previous_state_data: SenderStateData) -> SenderPartialSignatureCreation {
        SenderPartialSignatureCreation {
            state_data: previous_state_data,
        }
    }

    fn finalize_signature(mut self) -> Result<SenderState, TransactionProtocolError> {
        // Validate that all the required state is present.
        if self.state_data.sender_public_nonce.is_none() ||
            self.state_data.sender_private_nonce.is_none() ||
            self.state_data.sender_excess.is_none() ||
            self.state_data.sender_excess_blinding_factor.is_none() ||
            self.state_data.receiver_output_public_key.is_none() ||
            self.state_data.fee.is_none() ||
            self.state_data.lock_height.is_none() ||
            self.state_data.receiver_public_nonce.is_none() ||
            self.state_data.receiver_partial_signature.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        self.state_data.validate()?;

        let challenge = calculate_challenge(
            &self.state_data.sender_public_nonce.unwrap(),
            &self.state_data.receiver_public_nonce.unwrap(),
            &self.state_data.sender_excess.unwrap(),
            &self.state_data.receiver_output_public_key.unwrap(),
            self.state_data.fee.unwrap(),
            self.state_data.lock_height.unwrap(),
        );

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
            challenge,
        ) {
            return Err(TransactionProtocolError::InvalidSignatureError);
        }

        Ok(SenderState::FinalizedTransaction(FinalizedTransaction::new(
            self.state_data,
        )))
    }
}

/// In this state the transaction has been finalized and validated. The final transaction can now be built.
#[derive(Clone, Debug)]
struct FinalizedTransaction {
    state_data: SenderStateData,
}

impl FinalizedTransaction {
    fn new(state_data: SenderStateData) -> FinalizedTransaction {
        FinalizedTransaction { state_data }
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

// -------------------------------------- Receiver States ------------------------------------------
/// This is the starting state for the Receiver state machine. This state waits until it receives the message from the
/// Sender contains the Sender's public data at which point it moves on to the next state.
#[derive(Clone, Debug)]
struct ReceiverInit {}

impl ReceiverInit {
    fn new() -> Self {
        ReceiverInit {}
    }

    fn accept_initial_sender_public_data(self, sender_state_data: SenderPublicData) -> ReceiverState {
        ReceiverState::ReceiverOutputSetup(ReceiverOutputSetup::new(sender_state_data))
    }
}

/// In this state the Receiver will provide the data for their receiving output which will transition them to the next
/// state.
#[derive(Clone, Debug)]
struct ReceiverOutputSetup {
    sender_state_data: SenderPublicData,
    receiver_state_data: ReceiverStateData,
}

impl ReceiverOutputSetup {
    fn new(sender_state_data: SenderPublicData) -> ReceiverOutputSetup {
        ReceiverOutputSetup {
            sender_state_data,
            receiver_state_data: ReceiverStateData::new(),
        }
    }

    fn set_receiver_output(
        mut self,
        output: TransactionOutput,
        blinding_factor: BlindingFactor,
    ) -> Result<ReceiverState, TransactionProtocolError>
    {
        self.receiver_state_data.receiver_output = Some(output);
        self.receiver_state_data.receiver_output_blinding_factor = Some(blinding_factor);
        self.receiver_state_data.receiver_output_public_key = Some(PublicKey::from_secret_key(&blinding_factor));

        Ok(ReceiverState::ReceiverPartialSignatureCreation(
            ReceiverPartialSignatureCreation::new(self.sender_state_data, self.receiver_state_data),
        ))
    }
}

/// In this state the Receiver is ready to provide the data required for them to construct their partial signature
#[derive(Clone, Debug)]
struct ReceiverPartialSignatureCreation {
    sender_state_data: SenderPublicData,
    receiver_state_data: ReceiverStateData,
}

impl ReceiverPartialSignatureCreation {
    fn new(
        sender_state_data: SenderPublicData,
        receiver_state_data: ReceiverStateData,
    ) -> ReceiverPartialSignatureCreation
    {
        ReceiverPartialSignatureCreation {
            sender_state_data,
            receiver_state_data,
        }
    }

    fn set_private_nonce(mut self, nonce: SecretKey) -> Result<ReceiverState, TransactionProtocolError> {
        // Validate that all the required state is present.
        if self.receiver_state_data.receiver_output_public_key.is_none() ||
            self.receiver_state_data.receiver_output_blinding_factor.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        self.receiver_state_data.receiver_public_nonce = Some(PublicKey::from_secret_key(&nonce));
        self.receiver_state_data.receiver_private_nonce = Some(nonce);

        let challenge = calculate_challenge(
            &self.sender_state_data.sender_public_nonce,
            &self.receiver_state_data.receiver_public_nonce.unwrap(),
            &self.sender_state_data.sender_excess,
            &self.receiver_state_data.receiver_output_public_key.unwrap(),
            self.sender_state_data.fee,
            self.sender_state_data.lock_height,
        );

        self.receiver_state_data.receiver_partial_signature = Some(Signature::sign(
            self.receiver_state_data.receiver_output_blinding_factor.unwrap(),
            self.receiver_state_data.receiver_private_nonce.unwrap(),
            challenge,
        )?);

        Ok(ReceiverState::ReceiverCompleted(ReceiverCompleted::new(
            self.receiver_state_data,
        )))
    }
}

/// In this state the Receiver's state machine is complete and it can now construct the message with
/// the Receiver's public data which can be transmitted to the Sender.
#[derive(Clone, Debug)]
struct ReceiverCompleted {
    receiver_state_data: ReceiverStateData,
}

impl ReceiverCompleted {
    fn new(receiver_state_data: ReceiverStateData) -> ReceiverCompleted {
        ReceiverCompleted { receiver_state_data }
    }

    fn construct_receiver_public_data(&self) -> Result<ReceiverPublicData, TransactionProtocolError> {
        // Validate the current state to check we have the data we need
        if self.receiver_state_data.receiver_output_public_key.is_none() ||
            self.receiver_state_data.receiver_public_nonce.is_none() ||
            self.receiver_state_data.receiver_partial_signature.is_none() ||
            self.receiver_state_data.receiver_output.is_none()
        {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        Ok(ReceiverPublicData {
            receiver_output: self.receiver_state_data.receiver_output.unwrap(),
            receiver_output_public_key: self.receiver_state_data.receiver_output_public_key.unwrap(),
            receiver_public_nonce: self.receiver_state_data.receiver_public_nonce.unwrap(),
            receiver_partial_signature: self.receiver_state_data.receiver_partial_signature.unwrap(),
        })
    }
}

// -------------------------------- Sender Starting State Builder ----------------------------------
/// The SenderStateBuilder is a Builder to facilitate the construction of the Sender's initial state.
#[derive(Clone, Debug)]
pub struct SenderStateBuilder {
    amount: Option<u64>,
    lock_height: Option<u64>,
    fee: Option<u64>,
    inputs: Vec<TransactionInput>,
    outputs: Vec<TransactionOutput>,
    offset: Option<BlindingFactor>,
    sender_excess_blinding_factor: Option<BlindingFactor>,
    sender_private_nonce: Option<SecretKey>,
    sender_public_nonce: Option<PublicKey>,
}

impl SenderStateBuilder {
    pub fn new() -> Self {
        Self {
            amount: None,
            lock_height: None,
            fee: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            offset: None,
            sender_private_nonce: None,
            sender_public_nonce: None,
            sender_excess_blinding_factor: None,
        }
    }

    pub fn with_fee(mut self, fee: u64) -> Self {
        self.fee = Some(fee);
        self
    }

    pub fn with_amount(mut self, amount: u64) -> Self {
        self.amount = Some(amount);
        self
    }

    pub fn with_lock_height(mut self, lock_height: u64) -> Self {
        self.lock_height = Some(lock_height);
        self
    }

    pub fn with_offset(mut self, offset: BlindingFactor) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn with_input(mut self, input: TransactionInput, blinding_factor: BlindingFactor) -> Self {
        self.inputs.push(input);
        self.sender_excess_blinding_factor =
            Some(self.sender_excess_blinding_factor.unwrap_or(BlindingFactor::default()) - blinding_factor);
        self
    }

    pub fn with_output(mut self, output: TransactionOutput, blinding_factor: BlindingFactor) -> Self {
        self.outputs.push(output);
        self.sender_excess_blinding_factor =
            Some(self.sender_excess_blinding_factor.unwrap_or(BlindingFactor::default()) + blinding_factor);
        self
    }

    pub fn with_private_nonce(mut self, nonce: SecretKey) -> Self {
        self.sender_public_nonce = Some(PublicKey::from_secret_key(&nonce));
        self.sender_private_nonce = Some(nonce);
        self
    }

    pub fn finish(&self) -> Result<SenderStateData, TransactionProtocolError> {
        // The following needs to be set to attempt validation
        if self.offset.is_none() || self.sender_excess_blinding_factor.is_none() {
            return Err(TransactionProtocolError::IncompleteStateError);
        }

        let result = SenderStateData {
            amount: self.amount,
            lock_height: self.lock_height,
            fee: self.fee,
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            offset: self.offset,
            sender_excess_blinding_factor: Some(self.sender_excess_blinding_factor.unwrap() - self.offset.unwrap()),
            sender_excess: Some(PublicKey::from_secret_key(
                &(self.sender_excess_blinding_factor.unwrap() - self.offset.unwrap()),
            )),
            sender_private_nonce: self.sender_private_nonce,
            sender_public_nonce: self.sender_public_nonce,
            receiver_output_public_key: None,
            receiver_public_nonce: None,
            receiver_partial_signature: None,
            sender_partial_signature: None,
            final_signature: None,
        };

        result.validate()?;
        Ok(result)
    }
}

/// Convenience function that calculates the challenge for the Schnorr signatures
pub fn calculate_challenge(
    public_nonce1: &PublicKey,
    public_nonce2: &PublicKey,
    public_key1: &PublicKey,
    public_key2: &PublicKey,
    fee: u64,
    lock_height: u64,
) -> Challenge<SignatureHash>
{
    Challenge::<SignatureHash>::new()
        .concat((public_nonce1 + public_nonce2).as_bytes())
        .concat((public_key1 + public_key2).as_bytes())
        .concat(&fee.to_le_bytes())
        .concat(&lock_height.to_le_bytes())
}

#[cfg(test)]
mod test {
    use crate::{
        range_proof::RangeProof,
        transaction::{OutputFeatures, TransactionInput, TransactionOutput},
        transaction_protocol_manager::{
            ReceiverTransactionProtocolManager,
            SenderStateBuilder,
            SenderTransactionProtocolManager,
            TransactionProtocolError,
        },
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

        // Sender gets started building the initial state
        let mut initial_state = SenderStateBuilder::new()
            .with_amount(amount)
            .with_fee(fee)
            .with_lock_height(0u64)
            .with_input(
                TransactionInput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&input_secret_key.into(), &SecretKey::from(input1_val)),
                ),
                input_secret_key,
            )
            .with_input(
                TransactionInput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&input2_secret_key.into(), &SecretKey::from(input2_val)),
                ),
                input2_secret_key,
            )
            .with_output(
                TransactionOutput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&change_secret_key.into(), &SecretKey::from(change1_val)),
                    RangeProof([0; 1]),
                ),
                change_secret_key,
            )
            .with_private_nonce(sender_private_nonce);

        // Attempt to build initial state without setting offset
        assert_eq!(
            initial_state.finish(),
            Err(TransactionProtocolError::IncompleteStateError)
        );

        initial_state = initial_state.with_offset(offset);

        // Attempt to build initial state while the commitments don't balance
        assert_eq!(initial_state.finish(), Err(TransactionProtocolError::ValidationError));

        let initial_state = initial_state
            .with_output(
                TransactionOutput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&change2_secret_key.into(), &SecretKey::from(change2_val)),
                    RangeProof([0; 1]),
                ),
                change2_secret_key,
            )
            .finish()
            .unwrap();

        let mut sender_protocol_manager = SenderTransactionProtocolManager::new();

        assert!(sender_protocol_manager.is_sender_init());

        sender_protocol_manager
            .accept_sender_initial_state(initial_state.clone())
            .unwrap();

        assert!(sender_protocol_manager.is_sender_waiting_for_receiver_output());
        assert_eq!(
            sender_protocol_manager.accept_sender_initial_state(initial_state.clone()),
            Err(TransactionProtocolError::InvalidTransitionError)
        );

        let sender_public_data = sender_protocol_manager.construct_sender_public_data().unwrap();

        // Start Receiver state machine
        let mut receiver_protocol_manager = ReceiverTransactionProtocolManager::new();
        assert!(receiver_protocol_manager.is_receiver_init());

        receiver_protocol_manager
            .accept_sender_public_data(sender_public_data)
            .unwrap();
        assert!(receiver_protocol_manager.is_receiver_output_setup());

        receiver_protocol_manager
            .set_receiver_output(
                TransactionOutput::new(
                    OutputFeatures::empty(),
                    CommitmentFactory::create(&receiver_secret_key.into(), &SecretKey::from(amount)),
                    RangeProof([0; 1]),
                ),
                receiver_secret_key,
            )
            .unwrap();
        assert!(receiver_protocol_manager.is_receiver_partial_signature_creation());

        receiver_protocol_manager
            .set_receiver_nonce(receiver_private_nonce)
            .unwrap();
        assert!(receiver_protocol_manager.is_completed());

        // The receiver now constructs their public data message to send back to the sender
        let receiver_public_data = receiver_protocol_manager.construct_receiver_public_data().unwrap();

        // Lets try finalize the signature without accepting the receiver's public data
        assert_eq!(
            sender_protocol_manager.finalize_signature(),
            Err(TransactionProtocolError::InvalidTransitionError)
        );

        // Lets try accept receiver data with an incorrect output amount (same secret key)
        let mut incorrect_receiver_public_data = receiver_public_data.clone();
        incorrect_receiver_public_data.receiver_output = TransactionOutput::new(
            OutputFeatures::empty(),
            CommitmentFactory::create(&receiver_secret_key.into(), &SecretKey::from(amount + 10)),
            RangeProof([0; 1]),
        );
        sender_protocol_manager
            .accept_receiver_public_data(incorrect_receiver_public_data)
            .unwrap();
        assert!(sender_protocol_manager.is_failed());

        // Redo the sender side of the protocol as the previous test put the original sender_protocol_manager into the
        // Failed state
        let mut sender_protocol_manager = SenderTransactionProtocolManager::new();
        sender_protocol_manager
            .accept_sender_initial_state(initial_state.clone())
            .unwrap();
        sender_protocol_manager
            .accept_receiver_public_data(receiver_public_data)
            .unwrap();
        assert!(!sender_protocol_manager.is_failed());
        assert!(sender_protocol_manager.is_sender_partial_signature_creation());

        sender_protocol_manager.finalize_signature().unwrap();
        assert!(sender_protocol_manager.is_finalized());

        let final_tx = sender_protocol_manager.build_final_transaction().unwrap();
        final_tx.validate().unwrap();
    }
}
