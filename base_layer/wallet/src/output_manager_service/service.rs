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
    output_manager_service::{
        error::OutputManagerError,
        handle::{OutputManagerRequest, OutputManagerResponse},
    },
    types::{HashDigest, KeyDigest, TransactionRng},
};
use chrono::{Duration as ChronoDuration, NaiveDateTime, Utc};
use futures::{pin_mut, StreamExt};
use log::*;
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tari_core::{
    fee::Fee,
    tari_amount::MicroTari,
    transaction::{OutputFeatures, TransactionInput, TransactionOutput, UnblindedOutput},
    types::{PrivateKey, COMMITMENT_FACTORY, PROVER},
    SenderTransactionProtocol,
};
use tari_crypto::keys::SecretKey;
use tari_key_manager::keymanager::KeyManager;
use tari_service_framework::reply_channel;
const LOG_TARGET: &'static str = "base_layer::wallet::output_manager_service";

/// This service will manage a wallet's available outputs and the key manager that produces the keys for these outputs.
/// The service will assemble transactions to be sent from the wallets available outputs and provide keys to receive
/// outputs. When the outputs are detected on the blockchain the Transaction service will call this Service to confirm
/// them to be moved to the spent and unspent output lists respectively.
pub struct OutputManagerService {
    key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    unspent_outputs: Vec<UnblindedOutput>,
    spent_outputs: Vec<UnblindedOutput>,
    pending_transactions: HashMap<u64, PendingTransactionOutputs>,
    request_stream:
        Option<reply_channel::Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
}

impl OutputManagerService {
    pub fn new(
        request_stream: reply_channel::Receiver<
            OutputManagerRequest,
            Result<OutputManagerResponse, OutputManagerError>,
        >,
        master_key: PrivateKey,
        branch_seed: String,
        primary_key_index: usize,
    ) -> OutputManagerService
    {
        OutputManagerService {
            key_manager: Mutex::new(KeyManager::<PrivateKey, KeyDigest>::from(
                master_key,
                branch_seed,
                primary_key_index,
            )),
            unspent_outputs: Vec::new(),
            spent_outputs: Vec::new(),
            pending_transactions: HashMap::new(),
            request_stream: Some(request_stream),
        }
    }

    pub async fn start(mut self) -> Result<(), OutputManagerError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("OutputManagerService initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        info!("Output Manager Service started");
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                complete => {
                    info!(target: LOG_TARGET, "Output manager service shutting down");
                    break;
                }
            }
        }
        info!("Output Manager Service ended");
        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    async fn handle_request(
        &mut self,
        request: OutputManagerRequest,
    ) -> Result<OutputManagerResponse, OutputManagerError>
    {
        match request {
            OutputManagerRequest::AddOutput(uo) => self.add_output(uo).map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::GetBalance => Ok(OutputManagerResponse::Balance(self.get_balance())),
            OutputManagerRequest::GetRecipientKey((tx_id, amount)) => self
                .get_recipient_spending_key(tx_id, amount)
                .map(|k| OutputManagerResponse::RecipientKeyGenerated(k)),
            OutputManagerRequest::PrepareToSendTransaction((amount, fee_per_gram, lock_height)) => self
                .prepare_transaction_to_send(amount, fee_per_gram, lock_height)
                .map(|stp| OutputManagerResponse::TransactionToSend(stp)),
            OutputManagerRequest::ConfirmReceivedOutput((tx_id, output)) => self
                .confirm_received_transaction_output(tx_id, &output)
                .map(|_| OutputManagerResponse::OutputConfirmed),
            OutputManagerRequest::ConfirmSentTransaction((tx_id, spent_outputs, received_outputs)) => self
                .confirm_sent_transaction(tx_id, &spent_outputs, &received_outputs)
                .map(|_| OutputManagerResponse::TransactionConfirmed),
            OutputManagerRequest::CancelTransaction(tx_id) => self
                .cancel_transaction(tx_id)
                .map(|_| OutputManagerResponse::TransactionCancelled),
            OutputManagerRequest::TimeoutTransactions(period) => self
                .timeout_pending_transactions(period)
                .map(|_| OutputManagerResponse::TransactionsTimedOut),
            OutputManagerRequest::GetPendingTransactions => Ok(OutputManagerResponse::PendingTransactions(
                self.get_pending_transactions(),
            )),
            OutputManagerRequest::GetSpentOutputs => Ok(OutputManagerResponse::SpentOutputs(self.get_spent_outputs())),
            OutputManagerRequest::GetUnspentOutputs => {
                Ok(OutputManagerResponse::UnspentOutputs(self.get_unspent_outputs()))
            },
        }
    }

    /// Add an unblinded output to the unspent outputs list
    pub fn add_output(&mut self, output: UnblindedOutput) -> Result<(), OutputManagerError> {
        // Check it is not already present in the various output sets
        if self.contains_output(&output) {
            return Err(OutputManagerError::DuplicateOutput);
        }

        self.unspent_outputs.push(output);

        Ok(())
    }

    pub fn get_balance(&self) -> MicroTari {
        self.unspent_outputs
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.value)
    }

    /// Request a spending key to be used to accept a transaction from a sender.
    pub fn get_recipient_spending_key(
        &mut self,
        tx_id: u64,
        amount: MicroTari,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        let mut km = acquire_lock!(self.key_manager);

        let key = km.next_key()?.k;

        self.pending_transactions.insert(tx_id, PendingTransactionOutputs {
            tx_id,
            outputs_to_be_spent: Vec::new(),
            outputs_to_be_received: vec![UnblindedOutput {
                value: amount,
                spending_key: key.clone(),
                features: OutputFeatures::default(),
            }],
            timestamp: Utc::now().naive_utc(),
        });

        Ok(key)
    }

    /// Confirm the reception of an expected transaction output. This will be called by the Transaction Service when it
    /// detects the output on the blockchain
    pub fn confirm_received_transaction_output(
        &mut self,
        tx_id: u64,
        received_output: &TransactionOutput,
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self
            .pending_transactions
            .get_mut(&tx_id)
            .ok_or(OutputManagerError::PendingTransactionNotFound)?;

        // Assumption: We are only allowing a single output per receiver in the current transaction protocols.
        if pending_transaction.outputs_to_be_received.len() != 1 ||
            pending_transaction.outputs_to_be_received[0]
                .as_transaction_input(&COMMITMENT_FACTORY, OutputFeatures::default())
                .commitment !=
                received_output.commitment
        {
            return Err(OutputManagerError::IncompleteTransaction);
        }

        self.unspent_outputs
            .append(&mut pending_transaction.outputs_to_be_received);
        let _ = self.pending_transactions.remove(&tx_id);
        Ok(())
    }

    /// Prepare a Sender Transaction Protocol for the amount and fee_per_gram specified. If required a change output
    /// will be produced.
    pub fn prepare_transaction_to_send(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
    ) -> Result<SenderTransactionProtocol, OutputManagerError>
    {
        let mut rng = TransactionRng::new().unwrap();
        let outputs = self.select_outputs(amount, fee_per_gram, UTXOSelectionStrategy::Smallest)?;
        let total = outputs.iter().fold(MicroTari::from(0), |acc, x| acc + x.value);

        let offset = PrivateKey::random(&mut rng);
        let nonce = PrivateKey::random(&mut rng);

        let mut builder = SenderTransactionProtocol::builder(1);
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_amount(0, amount);

        for uo in outputs.iter() {
            builder.with_input(
                uo.as_transaction_input(&COMMITMENT_FACTORY, OutputFeatures::default()),
                uo.clone(),
            );
        }

        let fee_without_change = Fee::calculate(fee_per_gram, outputs.len(), 1);
        let mut change_key: Option<PrivateKey> = None;
        // If the input values > the amount to be sent + fees_without_change then we will need to include a change
        // output
        if total > amount + fee_without_change {
            let mut km = acquire_lock!(self.key_manager);
            let key = km.next_key()?.k;
            change_key = Some(key.clone());
            builder.with_change_secret(key);
        }

        let stp = builder
            .build::<HashDigest>(&PROVER, &COMMITMENT_FACTORY)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let outputs_to_be_spent = self
            .unspent_outputs
            .drain_filter(|uo| outputs.iter().any(|o| uo.spending_key == o.spending_key))
            .collect();

        let mut pending_transaction = PendingTransactionOutputs {
            tx_id: stp.get_tx_id()?,
            outputs_to_be_spent,
            outputs_to_be_received: Vec::new(),
            timestamp: Utc::now().naive_utc(),
        };
        // If a change output was created add it to the pending_outputs list.
        if let Some(key) = change_key {
            pending_transaction.outputs_to_be_received.push(UnblindedOutput {
                value: stp.get_amount_to_self()?,
                spending_key: key,
                features: OutputFeatures::default(),
            })
        }

        self.pending_transactions
            .insert(pending_transaction.tx_id, pending_transaction);

        Ok(stp)
    }

    /// Confirm that a received or sent transaction and its outputs have been detected on the base chain. This will
    /// usually be called by the Transaction Service which monitors the base chain.
    pub fn confirm_sent_transaction(
        &mut self,
        tx_id: u64,
        spent_outputs: &Vec<TransactionInput>,
        received_outputs: &Vec<TransactionOutput>,
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self
            .pending_transactions
            .get_mut(&tx_id)
            .ok_or(OutputManagerError::PendingTransactionNotFound)?;

        // Check that the set of TransactionInputs and TransactionOutputs provided contain all the spent and received
        // outputs in the PendingTransaction
        // Assumption: There will only be ONE extra output which belongs to the receiver
        if spent_outputs.len() != pending_transaction.outputs_to_be_spent.len() ||
            !pending_transaction.outputs_to_be_spent.iter().fold(true, |acc, i| {
                acc && spent_outputs.iter().any(|o| {
                    o.commitment ==
                        i.as_transaction_input(&COMMITMENT_FACTORY, OutputFeatures::default())
                            .commitment
                })
            }) ||
            received_outputs.len() - 1 != pending_transaction.outputs_to_be_received.len() ||
            !pending_transaction.outputs_to_be_received.iter().fold(true, |acc, i| {
                acc && received_outputs.iter().any(|o| {
                    o.commitment ==
                        i.as_transaction_input(&COMMITMENT_FACTORY, OutputFeatures::default())
                            .commitment
                })
            })
        {
            return Err(OutputManagerError::IncompleteTransaction);
        }

        self.unspent_outputs
            .append(&mut pending_transaction.outputs_to_be_received);
        self.spent_outputs.append(&mut pending_transaction.outputs_to_be_spent);
        let _ = self.pending_transactions.remove(&tx_id);

        Ok(())
    }

    /// Cancel a pending transaction and place the encumbered outputs back into the unspent pool
    pub fn cancel_transaction(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        let pending_transaction = self
            .pending_transactions
            .get_mut(&tx_id)
            .ok_or(OutputManagerError::PendingTransactionNotFound)?;

        self.unspent_outputs
            .append(&mut pending_transaction.outputs_to_be_spent);

        Ok(())
    }

    /// Go through the pending transaction and if any have existed longer than the specified duration, cancel them
    pub fn timeout_pending_transactions(&mut self, period: Duration) -> Result<(), OutputManagerError> {
        let mut transactions_to_be_cancelled = Vec::new();
        for (tx_id, pt) in self.pending_transactions.iter() {
            if pt.timestamp + ChronoDuration::from_std(period)? < Utc::now().naive_utc() {
                transactions_to_be_cancelled.push(tx_id.clone());
            }
        }

        for t in transactions_to_be_cancelled {
            self.cancel_transaction(t.clone())?
        }

        Ok(())
    }

    /// Select which outputs to use to send a transaction of the specified amount. Use the specified selection strategy
    /// to choose the outputs
    fn select_outputs(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        strategy: UTXOSelectionStrategy,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError>
    {
        let mut outputs = Vec::new();
        let mut total = MicroTari::from(0);
        let mut fee_without_change = MicroTari::from(0);
        let mut fee_with_change = MicroTari::from(0);

        match strategy {
            UTXOSelectionStrategy::Smallest => {
                self.unspent_outputs.sort();
                for o in self.unspent_outputs.iter() {
                    outputs.push(o.clone());
                    total += o.value.clone();
                    // I am assuming that the only output will be the payment output and change if required
                    fee_without_change = Fee::calculate(fee_per_gram, outputs.len(), 1);
                    fee_with_change = Fee::calculate(fee_per_gram, outputs.len(), 2);

                    if total == amount + fee_without_change || total >= amount + fee_with_change {
                        break;
                    }
                }
            },
        }

        if (total != amount + fee_without_change) && (total < amount + fee_with_change) {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        Ok(outputs)
    }

    pub fn get_pending_transactions(&self) -> HashMap<u64, PendingTransactionOutputs> {
        self.pending_transactions.clone()
    }

    pub fn get_spent_outputs(&self) -> Vec<UnblindedOutput> {
        self.spent_outputs.clone()
    }

    pub fn get_unspent_outputs(&self) -> Vec<UnblindedOutput> {
        self.unspent_outputs.clone()
    }

    /// Utility function to determine if an output exists in the spent, unspent or pending output sets
    pub fn contains_output(&self, output: &UnblindedOutput) -> bool {
        self.unspent_outputs
            .iter()
            .any(|o| o.value == output.value && o.spending_key == output.spending_key) ||
            self.spent_outputs
                .iter()
                .any(|o| o.value == output.value && o.spending_key == output.spending_key) ||
            self.pending_transactions.values().fold(false, |acc, pt| {
                acc || pt
                    .outputs_to_be_spent
                    .iter()
                    .chain(pt.outputs_to_be_received.iter())
                    .any(|o| o.value == output.value && o.spending_key == output.spending_key)
            })
    }
}

/// Holds the outputs that have been selected for a given pending transaction waiting for confirmation
#[derive(Clone)]
pub struct PendingTransactionOutputs {
    pub tx_id: u64,
    pub outputs_to_be_spent: Vec<UnblindedOutput>,
    pub outputs_to_be_received: Vec<UnblindedOutput>,
    pub timestamp: NaiveDateTime,
}

/// Different UTXO selection strategies for choosing which UTXO's are used to fulfill a transaction
/// TODO Investigate and implement more optimal strategies
pub enum UTXOSelectionStrategy {
    // Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit is removing small
    // UTXOs from the blockchain, con is that it costs more in fees
    Smallest,
}

#[cfg(test)]
mod test {
    use crate::output_manager_service::service::{OutputManagerService, PendingTransactionOutputs};
    use chrono::Utc;
    use rand::{CryptoRng, Rng, RngCore};
    use tari_core::{
        tari_amount::MicroTari,
        transaction::UnblindedOutput,
        types::{PrivateKey, PublicKey},
    };
    use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey};
    use tari_service_framework::reply_channel;

    fn make_output<R: Rng + CryptoRng>(rng: &mut R, val: MicroTari) -> UnblindedOutput {
        let key = PrivateKey::random(rng);
        UnblindedOutput::new(val, key, None)
    }

    #[test]
    fn test_contains_output_function() {
        let mut rng = rand::OsRng::new().unwrap();
        let (secret_key, _public_key) = PublicKey::random_keypair(&mut rng);
        let (_sender, receiver) = reply_channel::unbounded();
        let mut oms = OutputManagerService::new(receiver, secret_key, "".to_string(), 0);
        let mut balance = MicroTari::from(0);
        for _i in 0..3 {
            let uo = make_output(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
            balance += uo.value.clone();
            oms.add_output(uo).unwrap();
        }

        let uo1 = make_output(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));

        assert!(!oms.contains_output(&uo1));
        oms.add_output(uo1.clone()).unwrap();
        assert!(oms.contains_output(&uo1));

        let uo2 = make_output(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
        assert!(!oms.contains_output(&uo2));
        oms.spent_outputs.push(uo2.clone());
        assert!(oms.contains_output(&uo2));

        let uo3 = make_output(&mut rng.clone(), MicroTari::from(100 + rng.next_u64() % 1000));
        assert!(!oms.contains_output(&uo3));
        oms.pending_transactions.insert(1, PendingTransactionOutputs {
            tx_id: 1,
            outputs_to_be_received: vec![uo3.clone()],
            outputs_to_be_spent: Vec::new(),
            timestamp: Utc::now().naive_utc(),
        });
        assert!(oms.contains_output(&uo3));

        assert_eq!(uo1.value + balance, oms.get_balance());
    }
}
