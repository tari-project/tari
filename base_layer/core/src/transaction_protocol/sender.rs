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
    tari_amount::*,
    transaction::{KernelFeatures, Transaction, TransactionBuilder, TransactionInput, TransactionOutput},
    types::{BlindingFactor, CommitmentFactory, PrivateKey, PublicKey, RangeProofService, Signature},
};

use crate::{
    transaction::{KernelBuilder, MAX_TRANSACTION_INPUTS, MAX_TRANSACTION_OUTPUTS, MINIMUM_TRANSACTION_FEE},
    transaction_protocol::{
        build_challenge,
        recipient::{RecipientInfo, RecipientSignedTransactionData},
        transaction_initializer::SenderTransactionInitializer,
        TransactionMetadata,
        TransactionProtocolError as TPE,
    },
};
use digest::Digest;
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::pedersen::PedersenCommitment;
use tari_utilities::ByteArray;

//----------------------------------------   Local Data types     ----------------------------------------------------//

/// This struct contains all the information that a transaction initiator (the sender) will manage throughout the
/// Transaction construction process.
#[derive(Clone, Debug)]
pub(super) struct RawTransactionInfo {
    pub num_recipients: usize,
    // The sum of self-created outputs plus change
    pub amount_to_self: MicroTari,
    pub ids: Vec<u64>,
    pub amounts: Vec<MicroTari>,
    pub metadata: TransactionMetadata,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub offset: BlindingFactor,
    // The sender's blinding factor shifted by the sender-selected offset
    pub offset_blinding_factor: BlindingFactor,
    pub public_excess: PublicKey,
    // The sender's private nonce
    pub private_nonce: PrivateKey,
    // The sender's public nonce
    pub public_nonce: PublicKey,
    // The sum of all public nonces
    pub public_nonce_sum: PublicKey,
    pub recipient_info: RecipientInfo,
    pub signatures: Vec<Signature>,
}

impl RawTransactionInfo {
    pub fn calculate_total_amount(&self) -> MicroTari {
        let to_others: MicroTari = self.amounts.iter().sum();
        to_others + self.amount_to_self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SingleRoundSenderData {
    /// The transaction id for the recipient
    pub tx_id: u64,
    /// The amount, in µT, being sent to the recipient
    pub amount: MicroTari,
    /// The offset public excess for this transaction
    pub public_excess: PublicKey,
    /// The sender's public nonce
    pub public_nonce: PublicKey,
    /// The transaction metadata
    pub metadata: TransactionMetadata,
}

#[derive(Clone, Serialize, Deserialize)]
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
    pub fn builder(num_recipients: usize) -> SenderTransactionInitializer {
        SenderTransactionInitializer::new(num_recipients)
    }

    /// Convenience method to check whether we're receiving recipient data
    pub fn is_collecting_single_signature(&self) -> bool {
        match &self.state {
            SenderState::CollectingSingleSignature(_) => true,
            _ => false,
        }
    }

    /// Convenience method to check whether we're ready to send a message to a single recipient
    pub fn is_single_round_message_ready(&self) -> bool {
        match &self.state {
            SenderState::SingleRoundMessageReady(_) => true,
            _ => false,
        }
    }

    /// Method to determine if we are in the SenderState::Finalizing state
    pub fn is_finalizing(&self) -> bool {
        match &self.state {
            SenderState::Finalizing(_) => true,
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

    pub fn get_transaction(&self) -> Result<&Transaction, TPE> {
        match &self.state {
            SenderState::FinalizedTransaction(tx) => Ok(tx),
            _ => Err(TPE::InvalidStateError),
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

    /// Method to check if the provided tx_id matches this transaction
    pub fn check_tx_id(&self, tx_id: u64) -> bool {
        match &self.state {
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => info.ids[0] == tx_id,
            _ => false,
        }
    }

    pub fn get_tx_id(&self) -> Result<u64, TPE> {
        match &self.state {
            SenderState::Finalizing(info) |
            SenderState::SingleRoundMessageReady(info) |
            SenderState::CollectingSingleSignature(info) => Ok(info.ids[0]),
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

    /// Build the sender's message for the single-round protocol (one recipient) and move to next State
    pub fn build_single_round_message(&mut self) -> Result<SingleRoundSenderData, TPE> {
        match &self.state {
            SenderState::SingleRoundMessageReady(info) => {
                let result = SingleRoundSenderData {
                    tx_id: info.ids[0],
                    amount: self.get_total_amount().unwrap(),
                    public_nonce: info.public_nonce.clone(),
                    public_excess: info.public_excess.clone(),
                    metadata: info.metadata.clone(),
                };
                self.state = SenderState::CollectingSingleSignature(info.clone());
                Ok(result)
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Add the signed transaction from the recipient and move to the next state
    pub fn add_single_recipient_info(
        &mut self,
        rec: RecipientSignedTransactionData,
        prover: &RangeProofService,
    ) -> Result<(), TPE>
    {
        match &mut self.state {
            SenderState::CollectingSingleSignature(info) => {
                if !rec.output.verify_range_proof(prover)? {
                    return Err(TPE::ValidationError(
                        "Recipient output range proof failed to verify".into(),
                    ));
                }
                // Consolidate transaction info
                info.outputs.push(rec.output);
                // nonce is in the signature, so we'll add those together later
                info.public_excess = &info.public_excess + &rec.public_spend_key;
                info.public_nonce_sum = &info.public_nonce_sum + rec.partial_signature.get_public_nonce();
                info.signatures.push(rec.partial_signature);
                self.state = SenderState::Finalizing(info.clone());
                Ok(())
            },
            _ => Err(TPE::InvalidStateError),
        }
    }

    /// Attempts to build the final transaction.
    fn build_transaction(
        info: &RawTransactionInfo,
        features: KernelFeatures,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> Result<Transaction, TPE>
    {
        let mut tx_builder = TransactionBuilder::new();
        for i in &info.inputs {
            tx_builder.add_input(i.clone());
        }

        for o in &info.outputs {
            tx_builder.add_output(o.clone());
        }
        tx_builder.add_offset(info.offset.clone());
        let mut s_agg = info.signatures[0].clone();
        info.signatures.iter().skip(1).for_each(|s| s_agg = &s_agg + s);
        let excess = PedersenCommitment::from_public_key(&info.public_excess);
        let kernel = KernelBuilder::new()
            .with_fee(info.metadata.fee)
            .with_features(features)
            .with_lock_height(info.metadata.lock_height)
            .with_excess(&excess)
            .with_signature(&s_agg)
            .build()?;
        tx_builder.with_kernel(kernel);
        tx_builder.build(prover, factory).map_err(TPE::from)
    }

    /// Performs sanitary checks on the collected transaction pieces prior to building the final Transaction instance
    fn validate(&self) -> Result<(), TPE> {
        if let SenderState::Finalizing(info) = &self.state {
            let total_amount = info.calculate_total_amount();
            let fee = info.metadata.fee;
            // The fee should be less than the amount. This isn't a protocol requirement, but it's what you want 99.999%
            // of the time, and our users will thank us if we reject a tx where they put the amount in the fee field by
            // mistake!
            if fee > total_amount {
                return Err(TPE::ValidationError("Fee is greater than amount".into()));
            }
            // The fee must be greater than MIN_FEE to prevent spam attacks
            if fee < MINIMUM_TRANSACTION_FEE {
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
                let e = build_challenge(&info.public_nonce_sum, &info.metadata);
                let k = info.offset_blinding_factor.clone();
                let r = info.private_nonce.clone();
                let s = Signature::sign(k, r, &e).map_err(TPE::SigningError)?;
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
    pub fn finalize(
        &mut self,
        features: KernelFeatures,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> Result<bool, TPE>
    {
        // Create the final aggregated signature, moving to the Failed state if anything goes wrong
        match &mut self.state {
            SenderState::Finalizing(_) => {
                if let Err(e) = self.sign() {
                    self.state = SenderState::Failed(e);
                    return Ok(false);
                }
            },
            _ => return Err(TPE::InvalidStateError),
        }
        // Validate the inputs we have, and then construct the final transaction
        match &self.state {
            SenderState::Finalizing(info) => {
                let result = self
                    .validate()
                    .and_then(|_| Self::build_transaction(info, features, prover, factory));
                if let Err(e) = result {
                    self.state = SenderState::Failed(e);
                    return Ok(false);
                }
                let mut transaction = result.unwrap();
                let result = transaction
                    .validate_internal_consistency(prover, factory)
                    .map_err(TPE::TransactionBuildError);
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

//----------------------------------------         Tests          ----------------------------------------------------//

#[cfg(test)]
mod test {
    use crate::{
        fee::Fee,
        tari_amount::*,
        transaction::{KernelFeatures, OutputFeatures, UnblindedOutput},
        transaction_protocol::{
            sender::SenderTransactionProtocol,
            single_receiver::SingleReceiverTransactionProtocol,
            test_common::{make_input, TestParams},
            TransactionProtocolError,
        },
        types::{COMMITMENT_FACTORY, PROVER},
    };
    use rand::OsRng;
    use tari_crypto::common::Blake256;
    use tari_utilities::hex::Hex;

    #[test]
    fn zero_recipients() {
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, MicroTari(1200));
        let mut builder = SenderTransactionProtocol::builder(0);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(10))
            .with_offset(p.offset.clone())
            .with_private_nonce(p.nonce.clone())
            .with_change_secret(p.change_key.clone())
            .with_input(utxo, input)
            .with_output(UnblindedOutput::new(MicroTari(500), p.spend_key.clone(), None))
            .with_output(UnblindedOutput::new(MicroTari(400), p.spend_key.clone(), None));
        let mut sender = builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();
        assert_eq!(sender.is_failed(), false);
        assert!(sender.is_finalizing());
        match sender.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
            Ok(true) => (),
            Ok(false) => panic!("{:?}", sender.failure_reason()),
            Err(e) => panic!("{:?}", e),
        }
        let tx = sender.get_transaction().unwrap();
        assert_eq!(tx.offset, p.offset);
    }

    #[test]
    fn single_recipient_no_change() {
        let mut rng = OsRng::new().unwrap();
        // Alice's parameters
        let a = TestParams::new(&mut rng);
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, MicroTari(1200));
        let mut builder = SenderTransactionProtocol::builder(1);
        let fee = Fee::calculate(MicroTari(20), 1, 1);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_input(utxo.clone(), input)
            // A little twist: Check the case where the change is less than the cost of another output
            .with_amount(0, MicroTari(1200) - fee - MicroTari(10));
        let mut alice = builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message().unwrap();
        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());
        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(
            &msg,
            b.nonce,
            b.spend_key,
            OutputFeatures::empty(),
            &PROVER,
            &COMMITMENT_FACTORY,
        )
        .unwrap();
        // Alice gets message back, deserializes it, etc
        alice.add_single_recipient_info(bob_info.clone(), &PROVER).unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
            Ok(true) => (),
            Ok(false) => panic!("{:?}", alice.failure_reason()),
            Err(e) => panic!("{:?}", e),
        };
        assert!(alice.is_finalized());
        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.offset, a.offset);
        assert_eq!(tx.body.kernels[0].fee, fee + MicroTari(10)); // Check the twist above
        assert_eq!(tx.body.inputs.len(), 1);
        assert_eq!(tx.body.inputs[0], utxo);
        assert_eq!(tx.body.outputs.len(), 1);
        assert_eq!(tx.body.outputs[0], bob_info.output);
    }

    #[test]
    fn single_recipient_with_change() {
        let mut rng = OsRng::new().unwrap();
        // Alice's parameters
        let a = TestParams::new(&mut rng);
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, MicroTari(2500));
        let mut builder = SenderTransactionProtocol::builder(1);
        let fee = Fee::calculate(MicroTari(20), 1, 2);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_key.clone())
            .with_input(utxo.clone(), input)
            .with_amount(0, MicroTari(500));
        let mut alice = builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();
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
        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(
            &msg,
            b.nonce,
            b.spend_key,
            OutputFeatures::empty(),
            &PROVER,
            &COMMITMENT_FACTORY,
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
        alice.add_single_recipient_info(bob_info.clone(), &PROVER).unwrap();
        // Transaction should be complete
        assert!(alice.is_finalizing());
        match alice.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
            Ok(true) => (),
            Ok(false) => panic!("{:?}", alice.failure_reason()),
            Err(e) => panic!("{:?}", e),
        };

        assert!(alice.is_finalized());
        let tx = alice.get_transaction().unwrap();
        assert_eq!(tx.offset, a.offset);
        assert_eq!(tx.body.kernels[0].fee, fee);
        assert_eq!(tx.body.inputs.len(), 1);
        assert_eq!(tx.body.inputs[0], utxo);
        assert_eq!(tx.body.outputs.len(), 2);
        assert!(tx
            .clone()
            .validate_internal_consistency(&PROVER, &COMMITMENT_FACTORY)
            .is_ok());
    }

    #[test]
    fn single_recipient_range_proof_fail() {
        let mut rng = OsRng::new().unwrap();
        // Alice's parameters
        let a = TestParams::new(&mut rng);
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, (2u64.pow(32) + 2001).into());
        let mut builder = SenderTransactionProtocol::builder(1);

        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_key.clone())
            .with_input(utxo.clone(), input)
            .with_amount(0, (2u64.pow(32) + 1).into());
        let mut alice = builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();
        assert!(alice.is_single_round_message_ready());
        let msg = alice.build_single_round_message().unwrap();
        // Send message down the wire....and wait for response
        assert!(alice.is_collecting_single_signature());
        // Receiver gets message, deserializes it etc, and creates his response
        let bob_info = SingleReceiverTransactionProtocol::create(
            &msg,
            b.nonce,
            b.spend_key,
            OutputFeatures::empty(),
            &PROVER,
            &COMMITMENT_FACTORY,
        )
        .unwrap();
        // Alice gets message back, deserializes it, etc
        match alice.add_single_recipient_info(bob_info.clone(), &PROVER) {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                TransactionProtocolError::ValidationError("Recipient output range proof failed to verify".into())
            ),
        }
    }
}
