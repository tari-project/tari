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
        storage::models::{CompletedTransaction, InboundTransaction, OutboundTransaction, WalletTransaction},
    },
};
use aes_gcm::Aes256Gcm;
use futures::{stream::Fuse, StreamExt};
use std::{collections::HashMap, fmt, sync::Arc};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{tari_amount::MicroTari, transaction::Transaction};
use tari_service_framework::reply_channel::SenderService;
use tokio::sync::broadcast;
use tower::Service;

/// API Request enum
#[allow(clippy::large_enum_variant)]
pub enum TransactionServiceRequest {
    GetPendingInboundTransactions,
    GetPendingOutboundTransactions,
    GetCompletedTransactions,
    GetCancelledPendingInboundTransactions,
    GetCancelledPendingOutboundTransactions,
    GetCancelledCompletedTransactions,
    GetCompletedTransaction(TxId),
    GetAnyTransaction(TxId),
    SetBaseNodePublicKey(CommsPublicKey),
    SendTransaction((CommsPublicKey, MicroTari, MicroTari, String)),
    CancelTransaction(TxId),
    ImportUtxo(MicroTari, CommsPublicKey, String),
    SubmitTransaction((TxId, Transaction, MicroTari, MicroTari, String)),
    SetLowPowerMode,
    SetNormalPowerMode,
    ApplyEncryption(Box<Aes256Gcm>),
    RemoveEncryption,
    GenerateCoinbaseTransaction(MicroTari, MicroTari, u64),
    RestartTransactionProtocols,
    RestartBroadcastProtocols,
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

impl fmt::Display for TransactionServiceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetPendingInboundTransactions => f.write_str("GetPendingInboundTransactions"),
            Self::GetPendingOutboundTransactions => f.write_str("GetPendingOutboundTransactions"),
            Self::GetCompletedTransactions => f.write_str("GetCompletedTransactions"),
            Self::GetCancelledPendingInboundTransactions => f.write_str("GetCancelledPendingInboundTransactions"),
            Self::GetCancelledPendingOutboundTransactions => f.write_str("GetCancelledPendingOutboundTransactions"),
            Self::GetCancelledCompletedTransactions => f.write_str("GetCancelledCompletedTransactions"),
            Self::GetCompletedTransaction(t) => f.write_str(&format!("GetCompletedTransaction({})", t)),
            Self::SetBaseNodePublicKey(k) => f.write_str(&format!("SetBaseNodePublicKey ({})", k)),
            Self::SendTransaction((k, v, _, msg)) => {
                f.write_str(&format!("SendTransaction (to {}, {}, {})", k, v, msg))
            },
            Self::CancelTransaction(t) => f.write_str(&format!("CancelTransaction ({})", t)),
            Self::ImportUtxo(v, k, msg) => f.write_str(&format!("ImportUtxo (from {}, {}, {})", k, v, msg)),
            Self::SubmitTransaction((id, _, _, _, _)) => f.write_str(&format!("SubmitTransaction ({})", id)),
            Self::SetLowPowerMode => f.write_str("SetLowPowerMode "),
            Self::SetNormalPowerMode => f.write_str("SetNormalPowerMode"),
            TransactionServiceRequest::ApplyEncryption(_) => f.write_str("ApplyEncryption"),
            TransactionServiceRequest::RemoveEncryption => f.write_str("RemoveEncryption"),
            TransactionServiceRequest::GenerateCoinbaseTransaction(_, _, bh) => {
                f.write_str(&format!("GenerateCoinbaseTransaction (Blockheight {})", bh))
            },
            TransactionServiceRequest::RestartTransactionProtocols => f.write_str("RestartTransactionProtocols"),
            TransactionServiceRequest::RestartBroadcastProtocols => f.write_str("RestartBroadcastProtocols"),
            #[cfg(feature = "test_harness")]
            Self::CompletePendingOutboundTransaction(tx) => {
                f.write_str(&format!("CompletePendingOutboundTransaction ({})", tx.tx_id))
            },
            #[cfg(feature = "test_harness")]
            Self::FinalizePendingInboundTransaction(id) => {
                f.write_str(&format!("FinalizePendingInboundTransaction ({})", id))
            },
            #[cfg(feature = "test_harness")]
            Self::AcceptTestTransaction((id, _, _)) => f.write_str(&format!("AcceptTestTransaction ({})", id)),
            #[cfg(feature = "test_harness")]
            Self::MineTransaction(id) => f.write_str(&format!("MineTransaction ({})", id)),
            #[cfg(feature = "test_harness")]
            Self::BroadcastTransaction(id) => f.write_str(&format!("BroadcastTransaction ({})", id)),
            TransactionServiceRequest::GetAnyTransaction(t) => f.write_str(&format!("GetAnyTransaction({})", t)),
        }
    }
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceResponse {
    TransactionSent(TxId),
    TransactionCancelled,
    PendingInboundTransactions(HashMap<u64, InboundTransaction>),
    PendingOutboundTransactions(HashMap<u64, OutboundTransaction>),
    CompletedTransactions(HashMap<u64, CompletedTransaction>),
    CompletedTransaction(Box<CompletedTransaction>),
    BaseNodePublicKeySet,
    UtxoImported(TxId),
    TransactionSubmitted,
    LowPowerModeSet,
    NormalPowerModeSet,
    EncryptionApplied,
    EncryptionRemoved,
    CoinbaseTransactionGenerated(Box<Transaction>),
    ProtocolsRestarted,
    AnyTransaction(Box<Option<WalletTransaction>>),
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
    TransactionDiscoveryInProgress(TxId),
    TransactionDirectSendResult(TxId, bool),
    TransactionStoreForwardSendResult(TxId, bool),
    TransactionCancelled(TxId),
    TransactionBroadcast(TxId),
    TransactionMined(TxId),
    TransactionMinedRequestTimedOut(TxId),
    Error(String),
}

pub type TransactionEventSender = broadcast::Sender<Arc<TransactionEvent>>;
pub type TransactionEventReceiver = broadcast::Receiver<Arc<TransactionEvent>>;
/// The Transaction Service Handle is a struct that contains the interfaces used to communicate with a running
/// Transaction Service
#[derive(Clone)]
pub struct TransactionServiceHandle {
    handle: SenderService<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    event_stream_sender: TransactionEventSender,
}

impl TransactionServiceHandle {
    pub fn new(
        handle: SenderService<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
        event_stream_sender: TransactionEventSender,
    ) -> Self
    {
        Self {
            handle,
            event_stream_sender,
        }
    }

    pub fn get_event_stream_fused(&self) -> Fuse<TransactionEventReceiver> {
        self.event_stream_sender.subscribe().fuse()
    }

    pub async fn send_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        message: String,
    ) -> Result<TxId, TransactionServiceError>
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
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::CancelTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::TransactionCancelled => Ok(()),
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

    pub async fn get_cancelled_pending_inbound_transactions(
        &mut self,
    ) -> Result<HashMap<u64, InboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCancelledPendingInboundTransactions)
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

    pub async fn get_cancelled_pending_outbound_transactions(
        &mut self,
    ) -> Result<HashMap<u64, OutboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCancelledPendingOutboundTransactions)
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

    pub async fn get_cancelled_completed_transactions(
        &mut self,
    ) -> Result<HashMap<u64, CompletedTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCancelledCompletedTransactions)
            .await??
        {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_completed_transaction(
        &mut self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::GetCompletedTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::CompletedTransaction(t) => Ok(*t),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_any_transaction(
        &mut self,
        tx_id: TxId,
    ) -> Result<Option<WalletTransaction>, TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::GetAnyTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::AnyTransaction(t) => Ok(*t),
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

    pub async fn import_utxo(
        &mut self,
        amount: MicroTari,
        source_public_key: CommsPublicKey,
        message: String,
    ) -> Result<TxId, TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::ImportUtxo(
                amount,
                source_public_key,
                message,
            ))
            .await??
        {
            TransactionServiceResponse::UtxoImported(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn submit_transaction(
        &mut self,
        tx_id: u64,
        tx: Transaction,
        fee: MicroTari,
        amount: MicroTari,
        message: String,
    ) -> Result<(), TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::SubmitTransaction((
                tx_id, tx, fee, amount, message,
            )))
            .await??
        {
            TransactionServiceResponse::TransactionSubmitted => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn set_low_power_mode(&mut self) -> Result<(), TransactionServiceError> {
        match self.handle.call(TransactionServiceRequest::SetLowPowerMode).await?? {
            TransactionServiceResponse::LowPowerModeSet => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn set_normal_power_mode(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SetNormalPowerMode)
            .await??
        {
            TransactionServiceResponse::NormalPowerModeSet => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn apply_encryption(&mut self, cipher: Aes256Gcm) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ApplyEncryption(Box::new(cipher)))
            .await??
        {
            TransactionServiceResponse::EncryptionApplied => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn remove_encryption(&mut self) -> Result<(), TransactionServiceError> {
        match self.handle.call(TransactionServiceRequest::RemoveEncryption).await?? {
            TransactionServiceResponse::EncryptionRemoved => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn generate_coinbase_transaction(
        &mut self,
        rewards: MicroTari,
        fees: MicroTari,
        block_height: u64,
    ) -> Result<Transaction, TransactionServiceError>
    {
        match self
            .handle
            .call(TransactionServiceRequest::GenerateCoinbaseTransaction(
                rewards,
                fees,
                block_height,
            ))
            .await??
        {
            TransactionServiceResponse::CoinbaseTransactionGenerated(tx) => Ok(*tx),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn restart_transaction_protocols(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::RestartTransactionProtocols)
            .await??
        {
            TransactionServiceResponse::ProtocolsRestarted => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn restart_broadcast_protocols(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::RestartBroadcastProtocols)
            .await??
        {
            TransactionServiceResponse::ProtocolsRestarted => Ok(()),
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
