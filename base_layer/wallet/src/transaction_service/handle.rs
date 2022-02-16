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

use std::{collections::HashMap, fmt, fmt::Formatter, sync::Arc};

use aes_gcm::Aes256Gcm;
use tari_common_types::{
    transaction::{ImportStatus, TxId},
    types::PublicKey,
};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction_components::{Transaction, TransactionOutput},
};
use tari_service_framework::reply_channel::SenderService;
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;
use tower::Service;

use crate::{
    transaction_service::{
        error::TransactionServiceError,
        storage::models::{
            CompletedTransaction,
            InboundTransaction,
            OutboundTransaction,
            TxCancellationReason,
            WalletTransaction,
        },
    },
    OperationId,
};

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
    SendTransaction {
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        message: String,
    },
    SendOneSidedTransaction {
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        message: String,
    },
    SendShaAtomicSwapTransaction(CommsPublicKey, MicroTari, MicroTari, String),
    CancelTransaction(TxId),
    ImportUtxoWithStatus {
        amount: MicroTari,
        source_public_key: CommsPublicKey,
        message: String,
        maturity: Option<u64>,
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
    },
    SubmitTransactionToSelf(TxId, Transaction, MicroTari, MicroTari, String),
    SetLowPowerMode,
    SetNormalPowerMode,
    ApplyEncryption(Box<Aes256Gcm>),
    RemoveEncryption,
    GenerateCoinbaseTransaction(MicroTari, MicroTari, u64),
    RestartTransactionProtocols,
    RestartBroadcastProtocols,
    GetNumConfirmationsRequired,
    SetNumConfirmationsRequired(u64),
    ValidateTransactions,
    ReValidateTransactions,
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
            Self::SendTransaction {
                dest_pubkey,
                amount,
                message,
                ..
            } => f.write_str(&format!(
                "SendTransaction (to {}, {}, {})",
                dest_pubkey.to_hex(),
                amount,
                message
            )),
            Self::SendOneSidedTransaction {
                dest_pubkey,
                amount,
                message,
                ..
            } => f.write_str(&format!(
                "SendOneSidedTransaction (to {}, {}, {})",
                dest_pubkey.to_hex(),
                amount,
                message
            )),
            Self::SendShaAtomicSwapTransaction(k, v, _, msg) => {
                f.write_str(&format!("SendShaAtomicSwapTransaction (to {}, {}, {})", k, v, msg))
            },
            Self::CancelTransaction(t) => f.write_str(&format!("CancelTransaction ({})", t)),
            Self::ImportUtxoWithStatus {
                amount,
                source_public_key,
                message,
                maturity,
                import_status,
                tx_id,
                current_height,
            } => f.write_str(&format!(
                "ImportUtxo (from {}, {}, {} with maturity {} and {:?} and {:?} and {:?})",
                source_public_key,
                amount,
                message,
                maturity.unwrap_or(0),
                import_status,
                tx_id,
                current_height,
            )),
            Self::SubmitTransactionToSelf(tx_id, _, _, _, _) => f.write_str(&format!("SubmitTransaction ({})", tx_id)),
            Self::SetLowPowerMode => f.write_str("SetLowPowerMode "),
            Self::SetNormalPowerMode => f.write_str("SetNormalPowerMode"),
            Self::ApplyEncryption(_) => f.write_str("ApplyEncryption"),
            Self::RemoveEncryption => f.write_str("RemoveEncryption"),
            Self::GenerateCoinbaseTransaction(_, _, bh) => {
                f.write_str(&format!("GenerateCoinbaseTransaction (Blockheight {})", bh))
            },
            Self::RestartTransactionProtocols => f.write_str("RestartTransactionProtocols"),
            Self::RestartBroadcastProtocols => f.write_str("RestartBroadcastProtocols"),
            Self::GetNumConfirmationsRequired => f.write_str("GetNumConfirmationsRequired"),
            Self::SetNumConfirmationsRequired(_) => f.write_str("SetNumConfirmationsRequired"),
            Self::GetAnyTransaction(t) => f.write_str(&format!("GetAnyTransaction({})", t)),
            TransactionServiceRequest::ValidateTransactions => f.write_str("ValidateTransactions"),
            TransactionServiceRequest::ReValidateTransactions => f.write_str("ReValidateTransactions"),
        }
    }
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceResponse {
    TransactionSent(TxId),
    TransactionCancelled,
    PendingInboundTransactions(HashMap<TxId, InboundTransaction>),
    PendingOutboundTransactions(HashMap<TxId, OutboundTransaction>),
    CompletedTransactions(HashMap<TxId, CompletedTransaction>),
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
    NumConfirmationsRequired(u64),
    NumConfirmationsSet,
    ValidationStarted(OperationId),
    CompletedTransactionValidityChanged,
    ShaAtomicSwapTransactionSent(Box<(TxId, PublicKey, TransactionOutput)>),
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
    TransactionCompletedImmediately(TxId),
    TransactionStoreForwardSendResult(TxId, bool),
    TransactionCancelled(TxId, TxCancellationReason),
    TransactionBroadcast(TxId),
    TransactionImported(TxId),
    FauxTransactionUnconfirmed {
        tx_id: TxId,
        num_confirmations: u64,
        is_valid: bool,
    },
    FauxTransactionConfirmed {
        tx_id: TxId,
        is_valid: bool,
    },
    TransactionMined {
        tx_id: TxId,
        is_valid: bool,
    },
    TransactionMinedRequestTimedOut(TxId),
    TransactionMinedUnconfirmed {
        tx_id: TxId,
        num_confirmations: u64,
        is_valid: bool,
    },
    TransactionValidationStateChanged(OperationId),
    TransactionValidationCompleted(OperationId),
    TransactionValidationFailed(OperationId),
    Error(String),
}

impl fmt::Display for TransactionEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TransactionEvent::MempoolBroadcastTimedOut(tx_id) => {
                write!(f, "MempoolBroadcastTimedOut for tx:{}", tx_id)
            },
            TransactionEvent::ReceivedTransaction(tx) => {
                write!(f, "ReceivedTransaction for {}", tx)
            },
            TransactionEvent::ReceivedTransactionReply(tx) => {
                write!(f, "ReceivedTransactionReply for {}", tx)
            },
            TransactionEvent::ReceivedFinalizedTransaction(tx) => {
                write!(f, "ReceivedFinalizedTransaction for {}", tx)
            },
            TransactionEvent::TransactionDiscoveryInProgress(tx) => {
                write!(f, "TransactionDiscoveryInProgress for {}", tx)
            },
            TransactionEvent::TransactionDirectSendResult(tx, success) => {
                write!(f, "TransactionDirectSendResult for {}: {}", tx, success)
            },
            TransactionEvent::TransactionCompletedImmediately(tx) => {
                write!(f, "TransactionCompletedImmediately for {}", tx)
            },
            TransactionEvent::TransactionStoreForwardSendResult(tx, success) => {
                write!(f, "TransactionStoreForwardSendResult for {}:{}", tx, success)
            },
            TransactionEvent::TransactionCancelled(tx, rejection) => {
                write!(f, "TransactionCancelled for {}:{:?}", tx, rejection)
            },
            TransactionEvent::TransactionBroadcast(tx) => {
                write!(f, "TransactionBroadcast for {}", tx)
            },
            TransactionEvent::TransactionImported(tx) => {
                write!(f, "TransactionImported for {}", tx)
            },
            TransactionEvent::FauxTransactionUnconfirmed {
                tx_id,
                num_confirmations,
                is_valid,
            } => {
                write!(
                    f,
                    "FauxTransactionUnconfirmed for {} with num confirmations: {}. is_valid: {}",
                    tx_id, num_confirmations, is_valid
                )
            },
            TransactionEvent::FauxTransactionConfirmed { tx_id, is_valid } => {
                write!(f, "FauxTransactionConfirmed for {}. is_valid: {}", tx_id, is_valid)
            },
            TransactionEvent::TransactionMined { tx_id, is_valid } => {
                write!(f, "TransactionMined for {}. is_valid: {}", tx_id, is_valid)
            },
            TransactionEvent::TransactionMinedRequestTimedOut(tx) => {
                write!(f, "TransactionMinedRequestTimedOut for {}", tx)
            },
            TransactionEvent::TransactionMinedUnconfirmed {
                tx_id,
                num_confirmations,
                is_valid,
            } => {
                write!(
                    f,
                    "TransactionMinedUnconfirmed for {} with num confirmations: {}. is_valid: {}",
                    tx_id, num_confirmations, is_valid
                )
            },
            TransactionEvent::Error(error) => {
                write!(f, "Error:{}", error)
            },
            TransactionEvent::TransactionValidationStateChanged(operation_id) => {
                write!(f, "Transaction validation state changed: {}", operation_id)
            },
            TransactionEvent::TransactionValidationCompleted(operation_id) => {
                write!(f, "Transaction validation completed: {}", operation_id)
            },
            TransactionEvent::TransactionValidationFailed(operation_id) => {
                write!(f, "Transaction validation failed: {}", operation_id)
            },
        }
    }
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
    ) -> Self {
        Self {
            handle,
            event_stream_sender,
        }
    }

    pub fn get_event_stream(&self) -> TransactionEventReceiver {
        self.event_stream_sender.subscribe()
    }

    pub async fn send_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        message: String,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendTransaction {
                dest_pubkey,
                amount,
                unique_id: None,
                parent_public_key: None,
                fee_per_gram,
                message,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn send_transaction_or_token(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        message: String,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendTransaction {
                dest_pubkey,
                amount,
                unique_id,
                parent_public_key,
                fee_per_gram,
                message,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn send_one_sided_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        message: String,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendOneSidedTransaction {
                dest_pubkey,
                amount,
                unique_id: None,
                parent_public_key: None,
                fee_per_gram,
                message,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn send_one_sided_transaction_or_token(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        message: String,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendOneSidedTransaction {
                dest_pubkey,
                amount,
                unique_id,
                parent_public_key,
                fee_per_gram,
                message,
            })
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
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionServiceError> {
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
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionServiceError> {
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
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionServiceError> {
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
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionServiceError> {
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
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionServiceError> {
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
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionServiceError> {
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
    ) -> Result<CompletedTransaction, TransactionServiceError> {
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
    ) -> Result<Option<WalletTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetAnyTransaction(tx_id))
            .await??
        {
            TransactionServiceResponse::AnyTransaction(t) => Ok(*t),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn import_utxo_with_status(
        &mut self,
        amount: MicroTari,
        source_public_key: CommsPublicKey,
        message: String,
        maturity: Option<u64>,
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ImportUtxoWithStatus {
                amount,
                source_public_key,
                message,
                maturity,
                import_status,
                tx_id,
                current_height,
            })
            .await??
        {
            TransactionServiceResponse::UtxoImported(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn submit_transaction(
        &mut self,
        tx_id: TxId,
        tx: Transaction,
        amount: MicroTari,
        message: String,
    ) -> Result<(), TransactionServiceError> {
        let fee = tx.body.get_total_fee();
        match self
            .handle
            .call(TransactionServiceRequest::SubmitTransactionToSelf(
                tx_id, tx, fee, amount, message,
            ))
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

    pub async fn revalidate_all_transactions(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ReValidateTransactions)
            .await??
        {
            TransactionServiceResponse::ValidationStarted(_) => Ok(()),
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

    pub async fn get_num_confirmations_required(&mut self) -> Result<u64, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetNumConfirmationsRequired)
            .await??
        {
            TransactionServiceResponse::NumConfirmationsRequired(confirmations) => Ok(confirmations),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn set_num_confirmations_required(&mut self, number: u64) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SetNumConfirmationsRequired(number))
            .await??
        {
            TransactionServiceResponse::NumConfirmationsSet => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn generate_coinbase_transaction(
        &mut self,
        rewards: MicroTari,
        fees: MicroTari,
        block_height: u64,
    ) -> Result<Transaction, TransactionServiceError> {
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

    pub async fn validate_transactions(&mut self) -> Result<OperationId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ValidateTransactions)
            .await??
        {
            TransactionServiceResponse::ValidationStarted(id) => Ok(id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn send_sha_atomic_swap_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        message: String,
    ) -> Result<(TxId, PublicKey, TransactionOutput), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendShaAtomicSwapTransaction(
                dest_pubkey,
                amount,
                fee_per_gram,
                message,
            ))
            .await??
        {
            TransactionServiceResponse::ShaAtomicSwapTransactionSent(boxed) => {
                let (tx_id, pre_image, output) = *boxed;
                Ok((tx_id, pre_image, output))
            },
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }
}
