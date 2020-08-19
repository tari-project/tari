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
    protocols::utxo_validation_protocol::UtxoValidationRetry,
    service::Balance,
    storage::database::PendingTransactionOutputs,
};
use aes_gcm::Aes256Gcm;
use futures::{stream::Fuse, StreamExt};
use std::{collections::HashMap, fmt, time::Duration};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{Transaction, TransactionInput, TransactionOutput, UnblindedOutput},
    types::PrivateKey,
    SenderTransactionProtocol,
};
use tari_service_framework::reply_channel::SenderService;
use tokio::sync::broadcast;
use tower::Service;

/// API Request enum
pub enum OutputManagerRequest {
    GetBalance,
    AddOutput(UnblindedOutput),
    GetRecipientKey((u64, MicroTari)),
    GetCoinbaseKey((u64, MicroTari, u64)),
    ConfirmPendingTransaction(u64),
    ConfirmTransaction((u64, Vec<TransactionInput>, Vec<TransactionOutput>)),
    PrepareToSendTransaction((MicroTari, MicroTari, Option<u64>, String)),
    CancelTransaction(u64),
    TimeoutTransactions(Duration),
    GetPendingTransactions,
    GetSpentOutputs,
    GetUnspentOutputs,
    GetInvalidOutputs,
    GetSeedWords,
    SetBaseNodePublicKey(CommsPublicKey),
    ValidateUtxos(UtxoValidationRetry),
    CreateCoinSplit((MicroTari, usize, MicroTari, Option<u64>)),
    ApplyEncryption(Box<Aes256Gcm>),
    RemoveEncryption,
}

impl fmt::Display for OutputManagerRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetBalance => f.write_str("GetBalance"),
            Self::AddOutput(v) => f.write_str(&format!("AddOutput ({})", v.value())),
            Self::GetRecipientKey(v) => f.write_str(&format!("GetRecipientKey ({})", v.0)),
            Self::ConfirmTransaction(v) => f.write_str(&format!("ConfirmTransaction ({})", v.0)),
            Self::ConfirmPendingTransaction(v) => f.write_str(&format!("ConfirmPendingTransaction ({})", v)),
            Self::PrepareToSendTransaction((_, _, _, msg)) => {
                f.write_str(&format!("PrepareToSendTransaction ({})", msg))
            },
            Self::CancelTransaction(v) => f.write_str(&format!("CancelTransaction ({})", v)),
            Self::TimeoutTransactions(d) => f.write_str(&format!("TimeoutTransactions ({}s)", d.as_secs())),
            Self::GetPendingTransactions => f.write_str("GetPendingTransactions"),
            Self::GetSpentOutputs => f.write_str("GetSpentOutputs"),
            Self::GetUnspentOutputs => f.write_str("GetUnspentOutputs"),
            Self::GetInvalidOutputs => f.write_str("GetInvalidOutputs"),
            Self::GetSeedWords => f.write_str("GetSeedWords"),
            Self::SetBaseNodePublicKey(k) => f.write_str(&format!("SetBaseNodePublicKey ({})", k)),
            Self::ValidateUtxos(retry) => f.write_str(&format!("ValidateUtxos ({:?})", retry)),
            Self::CreateCoinSplit(v) => f.write_str(&format!("CreateCoinSplit ({})", v.0)),
            OutputManagerRequest::ApplyEncryption(_) => f.write_str("ApplyEncryption"),
            OutputManagerRequest::RemoveEncryption => f.write_str("RemoveEncryption"),
            OutputManagerRequest::GetCoinbaseKey(v) => f.write_str(&format!("GetCoinbaseKey ({})", v.0)),
        }
    }
}

/// API Reply enum
pub enum OutputManagerResponse {
    Balance(Balance),
    OutputAdded,
    RecipientKeyGenerated(PrivateKey),
    CoinbaseKeyGenerated(PrivateKey),
    OutputConfirmed,
    PendingTransactionConfirmed,
    TransactionConfirmed,
    TransactionToSend(SenderTransactionProtocol),
    TransactionCancelled,
    TransactionsTimedOut,
    PendingTransactions(HashMap<u64, PendingTransactionOutputs>),
    SpentOutputs(Vec<UnblindedOutput>),
    UnspentOutputs(Vec<UnblindedOutput>),
    InvalidOutputs(Vec<UnblindedOutput>),
    SeedWords(Vec<String>),
    BaseNodePublicKeySet,
    UtxoValidationStarted(u64),
    Transaction((u64, Transaction, MicroTari, MicroTari)),
    EncryptionApplied,
    EncryptionRemoved,
}

pub type OutputManagerEventSender = broadcast::Sender<OutputManagerEvent>;
pub type OutputManagerEventReceiver = broadcast::Receiver<OutputManagerEvent>;

/// Events that can be published on the Text Message Service Event Stream
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum OutputManagerEvent {
    UtxoValidationTimedOut(u64),
    UtxoValidationSuccess(u64),
    UtxoValidationFailure(u64),
    Error(String),
}

#[derive(Clone)]
pub struct OutputManagerHandle {
    handle: SenderService<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>,
    event_stream_sender: OutputManagerEventSender,
}

impl OutputManagerHandle {
    pub fn new(
        handle: SenderService<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>,
        event_stream_sender: OutputManagerEventSender,
    ) -> Self
    {
        OutputManagerHandle {
            handle,
            event_stream_sender,
        }
    }

    pub fn get_event_stream_fused(&self) -> Fuse<OutputManagerEventReceiver> {
        self.event_stream_sender.subscribe().fuse()
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
        block_height: u64,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::GetCoinbaseKey((tx_id, amount, block_height)))
            .await??
        {
            OutputManagerResponse::CoinbaseKeyGenerated(k) => Ok(k),
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

    pub async fn confirm_pending_transaction(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ConfirmPendingTransaction(tx_id))
            .await??
        {
            OutputManagerResponse::PendingTransactionConfirmed => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn confirm_transaction(
        &mut self,
        tx_id: u64,
        spent_outputs: Vec<TransactionInput>,
        received_outputs: Vec<TransactionOutput>,
    ) -> Result<(), OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::ConfirmTransaction((
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

    pub async fn get_invalid_outputs(&mut self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetInvalidOutputs).await?? {
            OutputManagerResponse::InvalidOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_seed_words(&mut self) -> Result<Vec<String>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetSeedWords).await?? {
            OutputManagerResponse::SeedWords(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn set_base_node_public_key(&mut self, public_key: CommsPublicKey) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::SetBaseNodePublicKey(public_key))
            .await??
        {
            OutputManagerResponse::BaseNodePublicKeySet => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn validate_utxos(&mut self, retries: UtxoValidationRetry) -> Result<u64, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::ValidateUtxos(retries)).await?? {
            OutputManagerResponse::UtxoValidationStarted(request_key) => Ok(request_key),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_coin_split(
        &mut self,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
    ) -> Result<(u64, Transaction, MicroTari, MicroTari), OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::CreateCoinSplit((
                amount_per_split,
                split_count,
                fee_per_gram,
                lock_height,
            )))
            .await??
        {
            OutputManagerResponse::Transaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn apply_encryption(&mut self, cipher: Aes256Gcm) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ApplyEncryption(Box::new(cipher)))
            .await??
        {
            OutputManagerResponse::EncryptionApplied => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn remove_encryption(&mut self) -> Result<(), OutputManagerError> {
        match self.handle.call(OutputManagerRequest::RemoveEncryption).await?? {
            OutputManagerResponse::EncryptionRemoved => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }
}
