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
    output_manager_service::TxId,
    transaction_service::{
        error::TransactionServiceError,
        service::PendingCoinbaseSpendingKey,
        storage::database::{CompletedTransaction, InboundTransaction, OutboundTransaction},
    },
};
use futures::{stream::Fuse, StreamExt};
use std::collections::HashMap;
use tari_broadcast_channel::Subscriber;
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{tari_amount::MicroTari, transaction::Transaction};
use tari_service_framework::reply_channel::SenderService;
use tower::Service;

/// API Request enum
#[derive(Debug)]
pub enum TransactionServiceRequest {
    GetPendingInboundTransactions,
    GetPendingOutboundTransactions,
    GetCompletedTransactions,
    SetBaseNodePublicKey(CommsPublicKey),
    SendTransaction((CommsPublicKey, MicroTari, MicroTari, String)),
    RequestCoinbaseSpendingKey((MicroTari, u64)),
    CompleteCoinbaseTransaction((TxId, Transaction)),
    CancelPendingCoinbaseTransaction(TxId),
    #[cfg(feature = "test_harness")]
    CompletePendingOutboundTransaction(CompletedTransaction),
    #[cfg(feature = "test_harness")]
    FinalizePendingInboundTransaction(TxId),
    #[cfg(feature = "test_harness")]
    AcceptTestTransaction((TxId, MicroTari, CommsPublicKey)),
    #[cfg(feature = "test_harness")]
    MineTransaction(TxId),
    #[cfg(feature = "test_harness")]
    BroadcastTransaction(TxId),
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceResponse {
    TransactionSent,
    PendingInboundTransactions(HashMap<u64, InboundTransaction>),
    PendingOutboundTransactions(HashMap<u64, OutboundTransaction>),
    CompletedTransactions(HashMap<u64, CompletedTransaction>),
    CoinbaseKey(PendingCoinbaseSpendingKey),
    CompletedCoinbaseTransactionReceived,
    CoinbaseTransactionCancelled,
    BaseNodePublicKeySet,
    #[cfg(feature = "test_harness")]
    CompletedPendingTransaction,
    #[cfg(feature = "test_harness")]
    FinalizedPendingInboundTransaction,
    #[cfg(feature = "test_harness")]
    AcceptedTestTransaction,
    #[cfg(feature = "test_harness")]
    TransactionMined,
    #[cfg(feature = "test_harness")]
    TransactionBroadcast,
}

/// Events that can be published on the Text Message Service Event Stream
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum TransactionEvent {
    MempoolBroadcastTimedOut(TxId),
    ReceivedTransaction(TxId),
    ReceivedTransactionReply(TxId),
    ReceivedFinalizedTransaction(TxId),
    TransactionSendDiscoveryComplete(TxId, bool),
    TransactionBroadcast(TxId),
    TransactionMined(TxId),
    TransactionSendDiscoverySuccess(TxId),
    TransactionSendDiscoveryFailure(TxId),
    TransactionMinedRequestTimedOut(TxId),
    Error(String),
}

/// The Transaction Service Handle is a struct that contains the interfaces used to communicate with a running
/// Transaction Service
#[derive(Clone)]
pub struct TransactionServiceHandle {
    handle: SenderService<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    event_stream: Subscriber<TransactionEvent>,
}

impl TransactionServiceHandle {
    pub fn new(
        handle: SenderService<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
        event_stream: Subscriber<TransactionEvent>,
    ) -> Self
    {
        Self { handle, event_stream }
    }

    pub fn get_event_stream_fused(&self) -> Fuse<Subscriber<TransactionEvent>> {
        self.event_stream.clone().fuse()
    }

    pub async fn send_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        message: String,
    ) -> Result<(), TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::SendTransaction((
                dest_pubkey,
                amount,
                fee_per_gram,
                message,
            )))
            .await??
        {
            TransactionServiceResponse::TransactionSent => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_pending_inbound_transactions(
        &mut self,
    ) -> Result<HashMap<u64, InboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetPendingInboundTransactions)
            .await??
        {
            TransactionServiceResponse::PendingInboundTransactions(p) => Ok(p),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_pending_outbound_transactions(
        &mut self,
    ) -> Result<HashMap<u64, OutboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetPendingOutboundTransactions)
            .await??
        {
            TransactionServiceResponse::PendingOutboundTransactions(p) => Ok(p),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_completed_transactions(
        &mut self,
    ) -> Result<HashMap<u64, CompletedTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCompletedTransactions)
            .await??
        {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn request_coinbase_key(
        &mut self,
        amount: MicroTari,
        maturity_height: u64,
    ) -> Result<PendingCoinbaseSpendingKey, TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::RequestCoinbaseSpendingKey((
                amount,
                maturity_height,
            )))
            .await??
        {
            TransactionServiceResponse::CoinbaseKey(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn complete_coinbase_transaction(
        &mut self,
        tx_id: TxId,
        completed_transaction: Transaction,
    ) -> Result<(), TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::CompleteCoinbaseTransaction((
                tx_id,
                completed_transaction,
            )))
            .await??
        {
            TransactionServiceResponse::CompletedCoinbaseTransactionReceived => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn cancel_coinbase_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::CancelPendingCoinbaseTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::CoinbaseTransactionCancelled => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn set_base_node_public_key(
        &mut self,
        public_key: CommsPublicKey,
    ) -> Result<(), TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::SetBaseNodePublicKey(public_key))
            .await??
        {
            TransactionServiceResponse::BaseNodePublicKeySet => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    #[cfg(feature = "test_harness")]
    pub async fn test_complete_pending_transaction(
        &mut self,
        completed_tx: CompletedTransaction,
    ) -> Result<(), TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::CompletePendingOutboundTransaction(
                completed_tx,
            ))
            .await??
        {
            TransactionServiceResponse::CompletedPendingTransaction => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    #[cfg(feature = "test_harness")]
    pub async fn test_accept_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        source_public_key: CommsPublicKey,
    ) -> Result<(), TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::AcceptTestTransaction((
                tx_id,
                amount,
                source_public_key,
            )))
            .await??
        {
            TransactionServiceResponse::AcceptedTestTransaction => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    #[cfg(feature = "test_harness")]
    pub async fn test_finalize_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::FinalizePendingInboundTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::FinalizedPendingInboundTransaction => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    #[cfg(feature = "test_harness")]
    pub async fn test_broadcast_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::BroadcastTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::TransactionBroadcast => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    #[cfg(feature = "test_harness")]
    pub async fn test_mine_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::MineTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::TransactionMined => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }
}
