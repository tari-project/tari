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

#[cfg(feature = "test_harness")]
use crate::output_manager_service::TxId;
use crate::transaction_service::{
    error::TransactionServiceError,
    storage::database::{CompletedTransaction, InboundTransaction, OutboundTransaction},
};
use futures::{stream::Fuse, StreamExt};
use std::collections::HashMap;
use tari_broadcast_channel::Subscriber;
use tari_comms::types::CommsPublicKey;
use tari_service_framework::reply_channel::SenderService;
use tari_transactions::tari_amount::MicroTari;
use tower::Service;

/// API Request enum
#[derive(Debug)]
pub enum TransactionServiceRequest {
    GetPendingInboundTransactions,
    GetPendingOutboundTransactions,
    GetCompletedTransactions,
    SendTransaction((CommsPublicKey, MicroTari, MicroTari, String)),
    #[cfg(feature = "test_harness")]
    CompletePendingOutboundTransaction(CompletedTransaction),
    #[cfg(feature = "test_harness")]
    AcceptTestTransaction((TxId, MicroTari, CommsPublicKey)),
    #[cfg(feature = "test_harness")]
    MineCompletedTransaction(TxId),
    #[cfg(feature = "test_harness")]
    BroadcastInboundTransaction(TxId),
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceResponse {
    TransactionSent,
    PendingInboundTransactions(HashMap<u64, InboundTransaction>),
    PendingOutboundTransactions(HashMap<u64, OutboundTransaction>),
    CompletedTransactions(HashMap<u64, CompletedTransaction>),
    #[cfg(feature = "test_harness")]
    CompletedPendingTransaction,
    #[cfg(feature = "test_harness")]
    AcceptedTestTransaction,
    #[cfg(feature = "test_harness")]
    CompletedTransactionMined,
    #[cfg(feature = "test_harness")]
    InboundTransactionBroadcast,
}

/// Events that can be published on the Text Message Service Event Stream
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum TransactionEvent {
    ReceivedTransaction,
    ReceivedTransactionReply,
    Error(String),
}

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
    pub async fn test_mine_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::MineCompletedTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::CompletedTransactionMined => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    #[cfg(feature = "test_harness")]
    pub async fn test_broadcast_inbound_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::BroadcastInboundTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::InboundTransactionBroadcast => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }
}
