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

use derive_error::Error;
use std::collections::HashMap;
use tari_core::{
    transaction::{KernelFeatures, OutputFeatures, Transaction},
    transaction_protocol::{
        recipient::RecipientSignedTransactionData,
        sender::SenderMessage,
        TransactionProtocolError,
    },
    types::SecretKey,
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};

#[derive(Debug, Error, PartialEq)]
pub enum TransactionManagerError {
    // Transaction protocol is not in the correct state for this operation
    InvalidStateError,
    // Transaction Protocol Error
    TransactionProtocolError(TransactionProtocolError),
    // The message being process is not recognized by the Transaction Manager
    InvalidMessageTypeError,
    // A message for a specific tx_id has been repeated
    RepeatedMessageError,
    // A recipient reply was received for a non-existent tx_id
    TransactionDoesNotExistError,
}

/// TransactionManager allows for the management of multiple inbound and outbound transaction protocols
/// which are uniquely identified by a tx_id. The TransactionManager generates and accepts the various protocol
/// messages and applies them to the appropriate protocol instances based on the tx_id.
/// The TransactionManager allows for the sending of transactions to single receivers, when the appropriate recipient
/// response is handled the transaction is completed and moved to the completed_transaction buffer.
/// The TransactionManager will accept inbound transactions and generate a reply. Received transactions will remain
/// in the pending_inbound_transactions buffer.
/// TODO Allow for inbound transactions that are detected on the blockchain to be marked as complete.
///
/// # Fields
/// 'pending_outbound_transactions' - List of transaction protocols sent by this client and waiting response from the
/// recipient 'pending_inbound_transactions' - List of transaction protocols that have been received and responded to.
/// 'completed_transaction' - List of sent transactions that have been responded to and are completed.

pub struct TransactionManager {
    pending_outbound_transactions: HashMap<u64, SenderTransactionProtocol>,
    pending_inbound_transactions: HashMap<u64, ReceiverTransactionProtocol>,
    completed_transactions: HashMap<u64, Transaction>,
}

impl TransactionManager {
    pub fn new() -> TransactionManager {
        TransactionManager {
            pending_outbound_transactions: HashMap::new(),
            pending_inbound_transactions: HashMap::new(),
            completed_transactions: HashMap::new(),
        }
    }

    /// Start to send a new transaction.
    /// # Arguments
    /// 'sender_transaction_protocol' - A well formed SenderTransactionProtocol ready to generate the SenderMessage.
    ///
    /// # Returns
    /// Public SenderMessage to be transmitted to the recipient.
    pub fn start_send_transaction(
        &mut self,
        mut sender_transaction_protocol: SenderTransactionProtocol,
    ) -> Result<SenderMessage, TransactionManagerError>
    {
        if !sender_transaction_protocol.is_single_round_message_ready() {
            return Err(TransactionManagerError::InvalidStateError);
        }

        let msg = sender_transaction_protocol.build_single_round_message()?;

        self.pending_outbound_transactions
            .insert(msg.tx_id.clone(), sender_transaction_protocol);

        Ok(SenderMessage::Single(Box::new(msg)))
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub fn accept_recipient_reply(
        &mut self,
        recipient_reply: RecipientSignedTransactionData,
    ) -> Result<(), TransactionManagerError>
    {
        let mut marked_for_removal = None;

        for (tx_id, stp) in self.pending_outbound_transactions.iter_mut() {
            let recp_tx_id = recipient_reply.tx_id.clone();
            if stp.check_tx_id(recp_tx_id) && stp.is_collecting_single_signature() {
                stp.add_single_recipient_info(recipient_reply)?;
                stp.finalize(KernelFeatures::empty())?;
                let tx = stp.get_transaction()?;
                self.completed_transactions.insert(recp_tx_id, tx.clone());
                marked_for_removal = Some(tx_id.clone());
                break;
            }
        }

        if marked_for_removal.is_none() {
            return Err(TransactionManagerError::TransactionDoesNotExistError);
        }

        if let Some(tx_id) = marked_for_removal {
            self.pending_outbound_transactions.remove(&tx_id);
        }

        Ok(())
    }

    /// Accept a new transaction from a sender by handling a public SenderMessage
    /// # Arguments
    /// 'sender_message' - Message from a sender containing the setup of the transaction being sent to you
    /// 'nonce' - Your chosen nonce for your signature of this transaction
    /// 'spending_key' - Your chosen secret_key for this transaction
    /// # Returns
    /// Public reply message to be sent back to the sender.
    pub fn accept_transaction(
        &mut self,
        sender_message: SenderMessage,
        nonce: SecretKey,
        spending_key: SecretKey,
    ) -> Result<RecipientSignedTransactionData, TransactionManagerError>
    {
        let rtp = ReceiverTransactionProtocol::new(sender_message, nonce, spending_key, OutputFeatures::empty());
        let recipient_reply = rtp.get_signed_data()?.clone();

        // Check this is not a repeat message i.e. tx_id doesn't already exist in our pending or completed transactions
        if self.pending_outbound_transactions.contains_key(&recipient_reply.tx_id) {
            return Err(TransactionManagerError::RepeatedMessageError);
        }

        if self.pending_inbound_transactions.contains_key(&recipient_reply.tx_id) {
            return Err(TransactionManagerError::RepeatedMessageError);
        }

        if self.completed_transactions.contains_key(&recipient_reply.tx_id) {
            return Err(TransactionManagerError::RepeatedMessageError);
        }

        // Otherwise add it to our pending transaction list and return reply
        self.pending_inbound_transactions
            .insert(recipient_reply.tx_id.clone(), rtp);

        Ok(recipient_reply)
    }

    /// Return a copy of the completed transactions
    fn get_completed_transactions(&self) -> HashMap<u64, Transaction> {
        return self.completed_transactions.clone();
    }

    fn num_pending_inbound_transactions(&self) -> usize {
        return self.pending_inbound_transactions.len();
    }

    fn num_pending_outbound_transactions(&self) -> usize {
        return self.pending_outbound_transactions.len();
    }
}

#[cfg(test)]
mod test {
    use crate::transaction_manager::{TransactionManager, TransactionManagerError};
    use rand::{CryptoRng, OsRng, Rng};
    use tari_core::{
        transaction::{OutputFeatures, TransactionInput, UnblindedOutput},
        transaction_protocol::{sender::SenderMessage, TransactionProtocolError},
        types::{CommitmentFactory, PublicKey, SecretKey},
        SenderTransactionProtocol,
    };
    use tari_crypto::{
        commitment::HomomorphicCommitmentFactory,
        common::Blake256,
        keys::{PublicKey as PK, SecretKey as SK},
    };

    pub struct TestParams {
        pub spend_key: SecretKey,
        pub change_key: SecretKey,
        pub offset: SecretKey,
        pub nonce: SecretKey,
        pub public_nonce: PublicKey,
    }

    impl TestParams {
        pub fn new<R: Rng + CryptoRng>(rng: &mut R) -> TestParams {
            let r = SecretKey::random(rng);
            TestParams {
                spend_key: SecretKey::random(rng),
                change_key: SecretKey::random(rng),
                offset: SecretKey::random(rng),
                public_nonce: PublicKey::from_secret_key(&r),
                nonce: r,
            }
        }
    }

    pub fn make_input<R: Rng + CryptoRng>(rng: &mut R, val: u64) -> (TransactionInput, UnblindedOutput) {
        let key = SecretKey::random(rng);
        let v = SecretKey::from(val);
        let commitment = CommitmentFactory::create(&key, &v);
        let input = TransactionInput::new(OutputFeatures::empty(), commitment);
        (input, UnblindedOutput::new(val, key, None))
    }

    #[test]
    fn manage_single_transaction() {
        let mut rng = OsRng::new().unwrap();
        // Alice's parameters
        let a = TestParams::new(&mut rng);
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 2500);
        let mut builder = SenderTransactionProtocol::new(1);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_key.clone())
            .with_input(utxo.clone(), input)
            .with_amount(0, 500);
        let alice_stp = builder.build::<Blake256>().unwrap();

        let mut alice_tx_manager = TransactionManager::new();
        let mut bob_tx_manager = TransactionManager::new();

        let send_msg = alice_tx_manager.start_send_transaction(alice_stp).unwrap();
        let mut tx_id = 0;
        if let SenderMessage::Single(single_round_sender_data) = send_msg.clone() {
            tx_id = single_round_sender_data.tx_id;
        }

        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 1);

        let receive_msg = bob_tx_manager
            .accept_transaction(send_msg, b.nonce, b.spend_key)
            .unwrap();

        assert_eq!(bob_tx_manager.num_pending_inbound_transactions(), 1);

        alice_tx_manager.accept_recipient_reply(receive_msg).unwrap();

        let txs = alice_tx_manager.get_completed_transactions();

        assert_eq!(txs.len(), 1);
        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 0);
        assert!(txs.contains_key(&tx_id));
    }

    #[test]
    fn manage_multiple_transactions() {
        let mut rng = OsRng::new().unwrap();
        // Alice, Bob and Carols various parameters
        let a_send1 = TestParams::new(&mut rng);
        let a_send2 = TestParams::new(&mut rng);
        let a_send3 = TestParams::new(&mut rng);
        let a_recv1 = TestParams::new(&mut rng);
        let b_send1 = TestParams::new(&mut rng);
        let b_recv1 = TestParams::new(&mut rng);
        let b_recv2 = TestParams::new(&mut rng);
        let c_recv1 = TestParams::new(&mut rng);

        // Initializing all the sending transaction protocols
        // Alice
        let (utxo_a1, input_a1) = make_input(&mut rng, 2500);
        let mut builder_a1 = SenderTransactionProtocol::new(1);
        builder_a1
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(a_send1.offset.clone())
            .with_private_nonce(a_send1.nonce.clone())
            .with_change_secret(a_send1.change_key.clone())
            .with_input(utxo_a1.clone(), input_a1)
            .with_amount(0, 500);
        let alice_stp1 = builder_a1.build::<Blake256>().unwrap();

        let (utxo_a2, input_a2) = make_input(&mut rng, 2500);
        let mut builder_a2 = SenderTransactionProtocol::new(1);
        builder_a2
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(a_send2.offset.clone())
            .with_private_nonce(a_send2.nonce.clone())
            .with_change_secret(a_send2.change_key.clone())
            .with_input(utxo_a2.clone(), input_a2)
            .with_amount(0, 500);
        let alice_stp2 = builder_a2.build::<Blake256>().unwrap();

        let (utxo_a3, input_a3) = make_input(&mut rng, 2500);
        let mut builder_a3 = SenderTransactionProtocol::new(1);
        builder_a3
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(a_send3.offset.clone())
            .with_private_nonce(a_send3.nonce.clone())
            .with_change_secret(a_send3.change_key.clone())
            .with_input(utxo_a3.clone(), input_a3)
            .with_amount(0, 500);
        let alice_stp3 = builder_a3.build::<Blake256>().unwrap();

        // Bob
        let (utxo_b1, input_b1) = make_input(&mut rng, 2500);
        let mut builder_b1 = SenderTransactionProtocol::new(1);
        builder_b1
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(b_send1.offset.clone())
            .with_private_nonce(b_send1.nonce.clone())
            .with_change_secret(b_send1.change_key.clone())
            .with_input(utxo_b1.clone(), input_b1)
            .with_amount(0, 500);
        let bob_stp1 = builder_b1.build::<Blake256>().unwrap();

        let mut alice_tx_manager = TransactionManager::new();
        let mut bob_tx_manager = TransactionManager::new();
        let mut carol_tx_manager = TransactionManager::new();

        // Now a series of interleaved sending and receiving of transactions
        let send_msg_a1 = alice_tx_manager.start_send_transaction(alice_stp1).unwrap();
        let mut alice_tx_ids = Vec::new();
        if let SenderMessage::Single(single_round_sender_data) = send_msg_a1.clone() {
            alice_tx_ids.push(single_round_sender_data.tx_id);
        }
        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 1);

        let send_msg_a2 = alice_tx_manager.start_send_transaction(alice_stp2).unwrap();
        if let SenderMessage::Single(single_round_sender_data) = send_msg_a2.clone() {
            alice_tx_ids.push(single_round_sender_data.tx_id);
        }
        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 2);

        let receive_msg_b1 = bob_tx_manager
            .accept_transaction(send_msg_a1, b_recv1.nonce, b_recv1.spend_key)
            .unwrap();
        assert_eq!(bob_tx_manager.num_pending_inbound_transactions(), 1);

        let receive_msg_c1 = carol_tx_manager
            .accept_transaction(send_msg_a2, c_recv1.nonce, c_recv1.spend_key)
            .unwrap();
        assert_eq!(carol_tx_manager.num_pending_inbound_transactions(), 1);

        let send_msg_b1 = bob_tx_manager.start_send_transaction(bob_stp1).unwrap();
        let mut bob_tx_ids = Vec::new();
        if let SenderMessage::Single(single_round_sender_data) = send_msg_b1.clone() {
            bob_tx_ids.push(single_round_sender_data.tx_id);
        }
        assert_eq!(bob_tx_manager.num_pending_outbound_transactions(), 1);

        let receive_msg_a1 = alice_tx_manager
            .accept_transaction(send_msg_b1, a_recv1.nonce, a_recv1.spend_key)
            .unwrap();
        assert_eq!(alice_tx_manager.num_pending_inbound_transactions(), 1);

        alice_tx_manager.accept_recipient_reply(receive_msg_c1).unwrap();
        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 1);
        assert_eq!(alice_tx_manager.get_completed_transactions().len(), 1);

        let send_msg_a3 = alice_tx_manager.start_send_transaction(alice_stp3).unwrap();
        if let SenderMessage::Single(single_round_sender_data) = send_msg_a3.clone() {
            alice_tx_ids.push(single_round_sender_data.tx_id);
        }
        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 2);

        let receive_msg_b2 = bob_tx_manager
            .accept_transaction(send_msg_a3, b_recv2.nonce, b_recv2.spend_key)
            .unwrap();
        assert_eq!(bob_tx_manager.num_pending_inbound_transactions(), 2);

        alice_tx_manager.accept_recipient_reply(receive_msg_b2).unwrap();
        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 1);
        assert_eq!(alice_tx_manager.get_completed_transactions().len(), 2);

        bob_tx_manager.accept_recipient_reply(receive_msg_a1).unwrap();
        assert_eq!(bob_tx_manager.num_pending_outbound_transactions(), 0);
        assert_eq!(bob_tx_manager.get_completed_transactions().len(), 1);

        alice_tx_manager.accept_recipient_reply(receive_msg_b1).unwrap();
        assert_eq!(alice_tx_manager.num_pending_outbound_transactions(), 0);
        assert_eq!(alice_tx_manager.get_completed_transactions().len(), 3);

        for tx_id in alice_tx_ids {
            assert!(alice_tx_manager.get_completed_transactions().contains_key(&tx_id));
        }

        for tx_id in bob_tx_ids {
            assert!(bob_tx_manager.get_completed_transactions().contains_key(&tx_id));
        }
    }

    #[test]
    fn accept_repeated_tx_id() {
        let mut rng = OsRng::new().unwrap();
        // Alice's parameters
        let a = TestParams::new(&mut rng);
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 2500);
        let mut builder = SenderTransactionProtocol::new(1);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_key.clone())
            .with_input(utxo.clone(), input)
            .with_amount(0, 500);
        let alice_stp = builder.build::<Blake256>().unwrap();

        let mut alice_tx_manager = TransactionManager::new();
        let mut bob_tx_manager = TransactionManager::new();

        let send_msg = alice_tx_manager.start_send_transaction(alice_stp).unwrap();

        let _receive_msg = bob_tx_manager
            .accept_transaction(send_msg.clone(), b.nonce.clone(), b.spend_key.clone())
            .unwrap();

        let receive_msg2 = bob_tx_manager.accept_transaction(send_msg, b.nonce, b.spend_key);

        assert_eq!(receive_msg2, Err(TransactionManagerError::RepeatedMessageError));
    }

    #[test]
    fn accept_malformed_sender_message() {
        let mut rng = OsRng::new().unwrap();
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let send_msg = SenderMessage::None;
        let mut bob_tx_manager = TransactionManager::new();
        let receive_msg = bob_tx_manager.accept_transaction(send_msg, b.nonce, b.spend_key);

        assert_eq!(
            receive_msg,
            Err(TransactionManagerError::TransactionProtocolError(
                TransactionProtocolError::InvalidStateError
            ))
        );
    }

    #[test]
    fn accept_malformed_recipient_reply() {
        let mut rng = OsRng::new().unwrap();
        // Alice's parameters
        let a = TestParams::new(&mut rng);
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 2500);
        let mut builder = SenderTransactionProtocol::new(1);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_key.clone())
            .with_input(utxo.clone(), input)
            .with_amount(0, 500);
        let alice_stp = builder.build::<Blake256>().unwrap();

        let mut alice_tx_manager = TransactionManager::new();
        let mut bob_tx_manager = TransactionManager::new();

        let send_msg = alice_tx_manager.start_send_transaction(alice_stp).unwrap();

        let mut receive_msg = bob_tx_manager
            .accept_transaction(send_msg.clone(), b.nonce.clone(), b.spend_key.clone())
            .unwrap();

        // Monkey with the range proof
        receive_msg.output.proof = [0u8; 32].to_vec();

        assert_eq!(
            alice_tx_manager.accept_recipient_reply(receive_msg),
            Err(TransactionManagerError::TransactionProtocolError(
                TransactionProtocolError::ValidationError("Recipient output range proof failed to verify".to_string())
            ))
        );
    }

    #[test]
    fn accept_recipient_reply_for_unknown_tx_id() {
        let mut rng = OsRng::new().unwrap();
        // Alice's parameters
        let a = TestParams::new(&mut rng);
        // Bob's parameters
        let b = TestParams::new(&mut rng);
        let (utxo, input) = make_input(&mut rng, 2500);
        let mut builder = SenderTransactionProtocol::new(1);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(20)
            .with_offset(a.offset.clone())
            .with_private_nonce(a.nonce.clone())
            .with_change_secret(a.change_key.clone())
            .with_input(utxo.clone(), input)
            .with_amount(0, 500);
        let alice_stp = builder.build::<Blake256>().unwrap();

        let mut alice_tx_manager = TransactionManager::new();
        let mut bob_tx_manager = TransactionManager::new();

        let send_msg = alice_tx_manager.start_send_transaction(alice_stp).unwrap();

        let mut receive_msg = bob_tx_manager
            .accept_transaction(send_msg.clone(), b.nonce.clone(), b.spend_key.clone())
            .unwrap();

        receive_msg.tx_id = 0;

        assert_eq!(
            alice_tx_manager.accept_recipient_reply(receive_msg),
            Err(TransactionManagerError::TransactionDoesNotExistError)
        );
    }

}
