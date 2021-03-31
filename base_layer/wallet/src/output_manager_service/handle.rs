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
        protocols::txo_validation_protocol::TxoValidationType,
        service::Balance,
        storage::database::PendingTransactionOutputs,
        TxId,
    },
    types::ValidationRetryStrategy,
};
use aes_gcm::Aes256Gcm;
use futures::{stream::Fuse, StreamExt};
use std::{collections::HashMap, fmt, sync::Arc, time::Duration};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{Transaction, TransactionInput, TransactionOutput, UnblindedOutput},
    transaction_protocol::sender::TransactionSenderMessage,
    types::PublicKey,
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};
use tari_service_framework::reply_channel::SenderService;
use tokio::sync::broadcast;
use tower::Service;

/// API Request enum
pub enum OutputManagerRequest {
    GetBalance,
    AddOutput(Box<UnblindedOutput>),
    GetRecipientTransaction(TransactionSenderMessage),
    GetCoinbaseTransaction((u64, MicroTari, MicroTari, u64)),
    ConfirmPendingTransaction(u64),
    ConfirmTransaction((u64, Vec<TransactionInput>, Vec<TransactionOutput>)),
    PrepareToSendTransaction((MicroTari, MicroTari, Option<u64>, String)),
    CreatePayToSelfTransaction((MicroTari, MicroTari, Option<u64>, String)),
    CancelTransaction(u64),
    TimeoutTransactions(Duration),
    GetPendingTransactions,
    GetSpentOutputs,
    GetUnspentOutputs,
    GetInvalidOutputs,
    GetSeedWords,
    SetBaseNodePublicKey(CommsPublicKey),
    ValidateUtxos(TxoValidationType, ValidationRetryStrategy),
    CreateCoinSplit((MicroTari, usize, MicroTari, Option<u64>)),
    ApplyEncryption(Box<Aes256Gcm>),
    RemoveEncryption,
    GetPublicRewindKeys,
    FeeEstimate((MicroTari, MicroTari, u64, u64)),
    RewindOutputs(Vec<TransactionOutput>),
    UpdateMinedHeight(u64, u64),
}

impl fmt::Display for OutputManagerRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use OutputManagerRequest::*;
        match self {
            GetBalance => write!(f, "GetBalance"),
            AddOutput(v) => write!(f, "AddOutput ({})", v.value),
            GetRecipientTransaction(_) => write!(f, "GetRecipientTransaction"),
            ConfirmTransaction(v) => write!(f, "ConfirmTransaction ({})", v.0),
            ConfirmPendingTransaction(v) => write!(f, "ConfirmPendingTransaction ({})", v),
            PrepareToSendTransaction((_, _, _, msg)) => write!(f, "PrepareToSendTransaction ({})", msg),
            CreatePayToSelfTransaction((_, _, _, msg)) => write!(f, "CreatePayToSelfTransaction ({})", msg),
            CancelTransaction(v) => write!(f, "CancelTransaction ({})", v),
            TimeoutTransactions(d) => write!(f, "TimeoutTransactions ({}s)", d.as_secs()),
            GetPendingTransactions => write!(f, "GetPendingTransactions"),
            GetSpentOutputs => write!(f, "GetSpentOutputs"),
            GetUnspentOutputs => write!(f, "GetUnspentOutputs"),
            GetInvalidOutputs => write!(f, "GetInvalidOutputs"),
            GetSeedWords => write!(f, "GetSeedWords"),
            SetBaseNodePublicKey(k) => write!(f, "SetBaseNodePublicKey ({})", k),
            ValidateUtxos(validation_type, retry) => write!(f, "{} ({:?})", validation_type, retry),
            CreateCoinSplit(v) => write!(f, "CreateCoinSplit ({})", v.0),
            ApplyEncryption(_) => write!(f, "ApplyEncryption"),
            RemoveEncryption => write!(f, "RemoveEncryption"),
            GetCoinbaseTransaction(_) => write!(f, "GetCoinbaseTransaction"),
            GetPublicRewindKeys => write!(f, "GetPublicRewindKeys"),
            FeeEstimate(_) => write!(f, "FeeEstimate"),
            RewindOutputs(_) => write!(f, "RewindAndImportOutputs"),
            UpdateMinedHeight(_, _) => write!(f, "UpdateMinedHeight"),
        }
    }
}

/// API Reply enum
#[derive(Debug, Clone)]
pub enum OutputManagerResponse {
    Balance(Balance),
    OutputAdded,
    RecipientTransactionGenerated(ReceiverTransactionProtocol),
    CoinbaseTransaction(Transaction),
    OutputConfirmed,
    PendingTransactionConfirmed,
    PayToSelfTransaction((TxId, MicroTari, Transaction)),
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
    PublicRewindKeys(Box<PublicRewindKeys>),
    FeeEstimate(MicroTari),
    RewindOutputs(Vec<UnblindedOutput>),
    MinedHeightUpdated,
}

pub type OutputManagerEventSender = broadcast::Sender<Arc<OutputManagerEvent>>;
pub type OutputManagerEventReceiver = broadcast::Receiver<Arc<OutputManagerEvent>>;

/// Events that can be published on the Output Manager Service Event Stream
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputManagerEvent {
    TxoValidationTimedOut(u64, TxoValidationType),
    TxoValidationSuccess(u64, TxoValidationType),
    TxoValidationFailure(u64, TxoValidationType),
    TxoValidationAborted(u64, TxoValidationType),
    TxoValidationDelayed(u64, TxoValidationType),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct PublicRewindKeys {
    pub rewind_public_key: PublicKey,
    pub rewind_blinding_public_key: PublicKey,
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
        match self
            .handle
            .call(OutputManagerRequest::AddOutput(Box::new(output)))
            .await??
        {
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

    pub async fn get_recipient_transaction(
        &mut self,
        sender_message: TransactionSenderMessage,
    ) -> Result<ReceiverTransactionProtocol, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::GetRecipientTransaction(sender_message))
            .await??
        {
            OutputManagerResponse::RecipientTransactionGenerated(rtp) => Ok(rtp),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_coinbase_transaction(
        &mut self,
        tx_id: TxId,
        reward: MicroTari,
        fees: MicroTari,
        block_height: u64,
    ) -> Result<Transaction, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::GetCoinbaseTransaction((
                tx_id,
                reward,
                fees,
                block_height,
            )))
            .await??
        {
            OutputManagerResponse::CoinbaseTransaction(tx) => Ok(tx),
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

    /// Get a fee estimate for an amount of MicroTari, at a specified fee per gram and given number of kernels and
    /// outputs.
    pub async fn fee_estimate(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        num_kernels: u64,
        num_outputs: u64,
    ) -> Result<MicroTari, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::FeeEstimate((
                amount,
                fee_per_gram,
                num_kernels,
                num_outputs,
            )))
            .await??
        {
            OutputManagerResponse::FeeEstimate(fee) => Ok(fee),
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

    /// Sorted from lowest value to highest
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

    pub async fn get_rewind_public_keys(&mut self) -> Result<PublicRewindKeys, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetPublicRewindKeys).await?? {
            OutputManagerResponse::PublicRewindKeys(rk) => Ok(*rk),
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

    pub async fn validate_txos(
        &mut self,
        validation_type: TxoValidationType,
        retries: ValidationRetryStrategy,
    ) -> Result<u64, OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::ValidateUtxos(validation_type, retries))
            .await??
        {
            OutputManagerResponse::UtxoValidationStarted(request_key) => Ok(request_key),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    /// Create a coin split transaction.
    /// Returns (tx_id, tx, fee, utxo_total_value).
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

    pub async fn rewind_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError>
    {
        match self.handle.call(OutputManagerRequest::RewindOutputs(outputs)).await?? {
            OutputManagerResponse::RewindOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_pay_to_self_transaction(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<(TxId, MicroTari, Transaction), OutputManagerError>
    {
        match self
            .handle
            .call(OutputManagerRequest::CreatePayToSelfTransaction((
                amount,
                fee_per_gram,
                lock_height,
                message,
            )))
            .await??
        {
            OutputManagerResponse::PayToSelfTransaction(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn update_mined_height(&mut self, tx_id: u64, height: u64) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::UpdateMinedHeight(tx_id, height))
            .await??
        {
            OutputManagerResponse::MinedHeightUpdated => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }
}
