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
        storage::database::{KeyManagerState, OutputManagerBackend, OutputManagerDatabase, PendingTransactionOutputs},
        TxId,
    },
    types::{HashDigest, KeyDigest},
};
use futures::{pin_mut, StreamExt};
use log::*;
use rand::rngs::OsRng;
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tari_core::transactions::{
    fee::Fee,
    tari_amount::MicroTari,
    transaction::{OutputFeatures, TransactionInput, TransactionOutput, UnblindedOutput},
    types::{CryptoFactories, PrivateKey},
    SenderTransactionProtocol,
};
use tari_crypto::keys::SecretKey as SecretKeyTrait;
use tari_key_manager::{
    key_manager::KeyManager,
    mnemonic::{from_secret_key, MnemonicLanguage},
};
use tari_service_framework::reply_channel;

const LOG_TARGET: &'static str = "base_layer::wallet::output_manager_service";

/// This service will manage a wallet's available outputs and the key manager that produces the keys for these outputs.
/// The service will assemble transactions to be sent from the wallets available outputs and provide keys to receive
/// outputs. When the outputs are detected on the blockchain the Transaction service will call this Service to confirm
/// them to be moved to the spent and unspent output lists respectively.
pub struct OutputManagerService<T>
where T: OutputManagerBackend + 'static
{
    key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    db: OutputManagerDatabase<T>,
    request_stream:
        Option<reply_channel::Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
    factories: CryptoFactories,
}

impl<T> OutputManagerService<T>
where T: OutputManagerBackend
{
    pub async fn new(
        request_stream: reply_channel::Receiver<
            OutputManagerRequest,
            Result<OutputManagerResponse, OutputManagerError>,
        >,
        db: OutputManagerDatabase<T>,
        factories: CryptoFactories,
    ) -> Result<OutputManagerService<T>, OutputManagerError>
    {
        // Check to see if there is any persisted state, otherwise start fresh
        let key_manager_state = match db.get_key_manager_state().await? {
            None => {
                let starting_state = KeyManagerState {
                    master_seed: PrivateKey::random(&mut OsRng),
                    branch_seed: "".to_string(),
                    primary_key_index: 0,
                };
                db.set_key_manager_state(starting_state.clone()).await?;
                starting_state
            },
            Some(km) => km,
        };

        Ok(OutputManagerService {
            key_manager: Mutex::new(KeyManager::<PrivateKey, KeyDigest>::from(
                key_manager_state.master_seed,
                key_manager_state.branch_seed,
                key_manager_state.primary_key_index,
            )),
            db,
            request_stream: Some(request_stream),
            factories,
        })
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
                    let _ = reply_tx.send(self.handle_request(request).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
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
            OutputManagerRequest::AddOutput(uo) => {
                self.add_output(uo).await.map(|_| OutputManagerResponse::OutputAdded)
            },
            OutputManagerRequest::GetBalance => self.get_balance().await.map(|a| OutputManagerResponse::Balance(a)),
            OutputManagerRequest::GetRecipientKey((tx_id, amount)) => self
                .get_recipient_spending_key(tx_id, amount)
                .await
                .map(|k| OutputManagerResponse::RecipientKeyGenerated(k)),
            OutputManagerRequest::PrepareToSendTransaction((amount, fee_per_gram, lock_height, message)) => self
                .prepare_transaction_to_send(amount, fee_per_gram, lock_height, message)
                .await
                .map(|stp| OutputManagerResponse::TransactionToSend(stp)),
            OutputManagerRequest::ConfirmTransaction((tx_id, spent_outputs, received_outputs)) => self
                .confirm_transaction(tx_id, &spent_outputs, &received_outputs)
                .await
                .map(|_| OutputManagerResponse::TransactionConfirmed),
            OutputManagerRequest::CancelTransaction(tx_id) => self
                .cancel_transaction(tx_id)
                .await
                .map(|_| OutputManagerResponse::TransactionCancelled),
            OutputManagerRequest::TimeoutTransactions(period) => self
                .timeout_pending_transactions(period)
                .await
                .map(|_| OutputManagerResponse::TransactionsTimedOut),
            OutputManagerRequest::GetPendingTransactions => self
                .fetch_pending_transaction_outputs()
                .await
                .map(|p| OutputManagerResponse::PendingTransactions(p)),
            OutputManagerRequest::GetSpentOutputs => self
                .fetch_spent_outputs()
                .await
                .map(|o| OutputManagerResponse::SpentOutputs(o)),
            OutputManagerRequest::GetUnspentOutputs => self
                .fetch_unspent_outputs()
                .await
                .map(|o| OutputManagerResponse::UnspentOutputs(o)),
            OutputManagerRequest::GetSeedWords => self.get_seed_words().map(|sw| OutputManagerResponse::SeedWords(sw)),
            OutputManagerRequest::GetCoinbaseKey((tx_id, amount, maturity_height)) => self
                .get_coinbase_spending_key(tx_id, amount, maturity_height)
                .await
                .map(|k| OutputManagerResponse::RecipientKeyGenerated(k)),
        }
    }

    /// Add an unblinded output to the unspent outputs list
    pub async fn add_output(&mut self, output: UnblindedOutput) -> Result<(), OutputManagerError> {
        Ok(self.db.add_unspent_output(output).await?)
    }

    pub async fn get_balance(&self) -> Result<Balance, OutputManagerError> {
        Ok(self.db.get_balance().await?)
    }

    /// Request a spending key to be used to accept a transaction from a sender.
    pub async fn get_recipient_spending_key(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        let mut key = PrivateKey::default();
        {
            let mut km = acquire_lock!(self.key_manager);
            key = km.next_key()?.k.clone();
        }

        self.db.increment_key_index().await?;
        self.db
            .accept_incoming_pending_transaction(tx_id, amount, key.clone(), OutputFeatures::default())
            .await?;

        Ok(key)
    }

    /// Request a spending key to be used to accept a coinbase output to be mined with the specified maturity height
    /// # Arguments:
    /// 'tx_id': the TxId that this coinbase transaction has been assigned
    /// 'amount': Amount of MicroTari the coinbase has as a value
    /// 'maturity_height': The block height at which this coinbase output becomes spendable
    pub async fn get_coinbase_spending_key(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        maturity_height: u64,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        let mut key = PrivateKey::default();

        {
            let mut km = acquire_lock!(self.key_manager);
            key = km.next_key()?.k;
        }

        self.db.increment_key_index().await?;
        self.db
            .accept_incoming_pending_transaction(
                tx_id,
                amount,
                key.clone(),
                OutputFeatures::create_coinbase(maturity_height),
            )
            .await?;

        Ok(key)
    }

    /// Confirm the reception of an expected transaction output. This will be called by the Transaction Service when it
    /// detects the output on the blockchain
    pub async fn confirm_received_transaction_output(
        &mut self,
        tx_id: u64,
        received_output: &TransactionOutput,
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self.db.fetch_pending_transaction_outputs(tx_id.clone()).await?;

        // Assumption: We are only allowing a single output per receiver in the current transaction protocols.
        if pending_transaction.outputs_to_be_received.len() != 1 ||
            pending_transaction.outputs_to_be_received[0]
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default())
                .commitment !=
                received_output.commitment
        {
            return Err(OutputManagerError::IncompleteTransaction);
        }

        self.db
            .confirm_pending_transaction_outputs(pending_transaction.tx_id.clone())
            .await?;

        Ok(())
    }

    /// Prepare a Sender Transaction Protocol for the amount and fee_per_gram specified. If required a change output
    /// will be produced.
    pub async fn prepare_transaction_to_send(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<SenderTransactionProtocol, OutputManagerError>
    {
        let outputs = self
            .select_outputs(amount, fee_per_gram, UTXOSelectionStrategy::Smallest)
            .await?;
        let total = outputs.iter().fold(MicroTari::from(0), |acc, x| acc + x.value);

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(1);
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_amount(0, amount)
            .with_message(message);

        for uo in outputs.iter() {
            builder.with_input(
                uo.as_transaction_input(&self.factories.commitment, OutputFeatures::default()),
                uo.clone(),
            );
        }

        let fee_without_change = Fee::calculate(fee_per_gram, outputs.len(), 1);
        let mut change_key: Option<PrivateKey> = None;
        // If the input values > the amount to be sent + fees_without_change then we will need to include a change
        // output
        if total > amount + fee_without_change {
            let mut key = PrivateKey::default();
            {
                let mut km = acquire_lock!(self.key_manager);
                key = km.next_key()?.k;
            }
            self.db.increment_key_index().await?;
            change_key = Some(key.clone());
            builder.with_change_secret(key);
        }

        let stp = builder
            .build::<HashDigest>(&self.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // If a change output was created add it to the pending_outputs list.
        let change_output = match change_key {
            Some(key) => Some(UnblindedOutput {
                value: stp.get_amount_to_self()?,
                spending_key: key,
                features: OutputFeatures::default(),
            }),
            None => None,
        };

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        self.db
            .encumber_outputs(stp.get_tx_id()?, outputs, change_output)
            .await?;

        Ok(stp)
    }

    /// Confirm that a received or sent transaction and its outputs have been detected on the base chain. The inputs and
    /// outputs are checked to see that they match what the stored PendingTransaction contians. This will
    /// be called by the Transaction Service which monitors the base chain.
    pub async fn confirm_transaction(
        &mut self,
        tx_id: u64,
        inputs: &Vec<TransactionInput>,
        outputs: &Vec<TransactionOutput>,
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self.db.fetch_pending_transaction_outputs(tx_id.clone()).await?;

        // Check that outputs to be spent can all be found in the provided transaction inputs
        let mut inputs_confirmed = true;
        for output_to_spend in pending_transaction.outputs_to_be_spent.iter() {
            let input_to_check = output_to_spend
                .clone()
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default());
            inputs_confirmed = inputs_confirmed &&
                inputs
                    .iter()
                    .find(|input| input.commitment == input_to_check.commitment)
                    .is_some();
        }

        // Check that outputs to be received can all be found in the provided transaction outputs
        let mut outputs_confirmed = true;
        for output_to_receive in pending_transaction.outputs_to_be_received.iter() {
            let output_to_check = output_to_receive
                .clone()
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default());
            outputs_confirmed = outputs_confirmed &&
                outputs
                    .iter()
                    .find(|output| output.commitment == output_to_check.commitment)
                    .is_some();
        }

        if !inputs_confirmed || !outputs_confirmed {
            return Err(OutputManagerError::IncompleteTransaction);
        }

        self.db
            .confirm_pending_transaction_outputs(pending_transaction.tx_id)
            .await?;

        Ok(())
    }

    /// Cancel a pending transaction and place the encumbered outputs back into the unspent pool
    pub async fn cancel_transaction(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        Ok(self.db.cancel_pending_transaction_outputs(tx_id).await?)
    }

    /// Go through the pending transaction and if any have existed longer than the specified duration, cancel them
    pub async fn timeout_pending_transactions(&mut self, period: Duration) -> Result<(), OutputManagerError> {
        Ok(self.db.timeout_pending_transaction_outputs(period).await?)
    }

    /// Select which outputs to use to send a transaction of the specified amount. Use the specified selection strategy
    /// to choose the outputs
    async fn select_outputs(
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

        let uo = self.db.fetch_sorted_unspent_outputs().await?;

        match strategy {
            UTXOSelectionStrategy::Smallest => {
                for o in uo.iter() {
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

    pub async fn fetch_pending_transaction_outputs(
        &self,
    ) -> Result<HashMap<u64, PendingTransactionOutputs>, OutputManagerError> {
        Ok(self.db.fetch_all_pending_transaction_outputs().await?)
    }

    pub async fn fetch_spent_outputs(&self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        Ok(self.db.fetch_spent_outputs().await?)
    }

    pub async fn fetch_unspent_outputs(&self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        Ok(self.db.fetch_sorted_unspent_outputs().await?)
    }

    /// Return the Seed words for the current Master Key set in the Key Manager
    pub fn get_seed_words(&self) -> Result<Vec<String>, OutputManagerError> {
        Ok(from_secret_key(
            &acquire_lock!(self.key_manager).master_key,
            &MnemonicLanguage::English,
        )?)
    }
}

/// Different UTXO selection strategies for choosing which UTXO's are used to fulfill a transaction
/// TODO Investigate and implement more optimal strategies
pub enum UTXOSelectionStrategy {
    // Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit is removing small
    // UTXOs from the blockchain, con is that it costs more in fees
    Smallest,
}

/// This struct holds the detailed balance of the Output Manager Service.
#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    /// The current balance that is available to spend
    pub available_balance: MicroTari,
    /// The current balance of funds that are due to be received but have not yet been confirmed
    pub pending_incoming_balance: MicroTari,
    /// The current balance of funds encumbered in pending outbound transactions that have not been confirmed
    pub pending_outgoing_balance: MicroTari,
}
