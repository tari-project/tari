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

use crate::output_manager_service::{
    error::OutputManagerError,
    service::Balance,
    storage::database::PendingTransactionOutputs,
};
use std::{collections::HashMap, time::Duration};
use tari_service_framework::reply_channel::SenderService;
use tari_transactions::{
    tari_amount::MicroTari,
    transaction::{TransactionInput, TransactionOutput, UnblindedOutput},
    types::PrivateKey,
    SenderTransactionProtocol,
};
use tower::Service;

/// API Request enum
#[derive(Debug)]
pub enum OutputManagerRequest {
    GetBalance,
    AddOutput(UnblindedOutput),
    GetRecipientKey((u64, MicroTari)),
    GetCoinbaseKey((u64, MicroTari, u64)),
    ConfirmReceivedOutput((u64, TransactionOutput)),
    ConfirmSentTransaction((u64, Vec<TransactionInput>, Vec<TransactionOutput>)),
    PrepareToSendTransaction((MicroTari, MicroTari, Option<u64>, String)),
    CancelTransaction(u64),
    TimeoutTransactions(Duration),
    GetPendingTransactions,
    GetSpentOutputs,
    GetUnspentOutputs,
    GetSeedWords,
}

/// API Reply enum
pub enum OutputManagerResponse {
    Balance(Balance),
    OutputAdded,
    RecipientKeyGenerated(PrivateKey),
    OutputConfirmed,
    TransactionConfirmed,
    TransactionToSend(SenderTransactionProtocol),
    TransactionCancelled,
    TransactionsTimedOut,
    PendingTransactions(HashMap<u64, PendingTransactionOutputs>),
    SpentOutputs(Vec<UnblindedOutput>),
    UnspentOutputs(Vec<UnblindedOutput>),
    SeedWords(Vec<String>),
}

#[derive(Clone)]
pub struct OutputManagerHandle {
    handle: SenderService<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>,
}

impl OutputManagerHandle {
    pub fn new(handle: SenderService<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>) -> Self {
        OutputManagerHandle { handle }
    }

    pub async fn add_output(&mut self, output: UnblindedOutput) -> Result<(), OutputManagerError> {
        match self.handle.call(OutputManagerRequest::AddOutput(output)).await?? {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_balance(&mut self) -> Result<Balance, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetBalance).await?? {
            OutputManagerResponse::Balance(b) => Ok(b),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_recipient_spending_key(
        &mut self,
        tx_id: u64,
        amount: MicroTari,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::GetRecipientKey((tx_id, amount)))
            .await??
        {
            OutputManagerResponse::RecipientKeyGenerated(k) => Ok(k),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_coinbase_spending_key(
        &mut self,
        tx_id: u64,
        amount: MicroTari,
        maturity_height: u64,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::GetCoinbaseKey((tx_id, amount, maturity_height)))
            .await??
        {
            OutputManagerResponse::RecipientKeyGenerated(k) => Ok(k),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn prepare_transaction_to_send(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<SenderTransactionProtocol, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::PrepareToSendTransaction((
                amount,
                fee_per_gram,
                lock_height,
                message,
            )))
            .await??
        {
            OutputManagerResponse::TransactionToSend(stp) => Ok(stp),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn confirm_received_output(
        &mut self,
        tx_id: u64,
        output: TransactionOutput,
    ) -> Result<(), OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::ConfirmReceivedOutput((tx_id, output)))
            .await??
        {
            OutputManagerResponse::OutputConfirmed => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn confirm_sent_transaction(
        &mut self,
        tx_id: u64,
        spent_outputs: Vec<TransactionInput>,
        received_outputs: Vec<TransactionOutput>,
    ) -> Result<(), OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::ConfirmSentTransaction((
                tx_id,
                spent_outputs,
                received_outputs,
            )))
            .await??
        {
            OutputManagerResponse::TransactionConfirmed => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn cancel_transaction(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CancelTransaction(tx_id))
            .await??
        {
            OutputManagerResponse::TransactionCancelled => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn timeout_transactions(&mut self, period: Duration) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::TimeoutTransactions(period))
            .await??
        {
            OutputManagerResponse::TransactionsTimedOut => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_pending_transactions(
        &mut self,
    ) -> Result<HashMap<u64, PendingTransactionOutputs>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetPendingTransactions).await?? {
            OutputManagerResponse::PendingTransactions(p) => Ok(p),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_spent_outputs(&mut self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetSpentOutputs).await?? {
            OutputManagerResponse::SpentOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_unspent_outputs(&mut self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetUnspentOutputs).await?? {
            OutputManagerResponse::UnspentOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_seed_words(&mut self) -> Result<Vec<String>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetSeedWords).await?? {
            OutputManagerResponse::SeedWords(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }
}
