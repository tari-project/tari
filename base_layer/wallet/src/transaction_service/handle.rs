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

use std::{
    collections::HashMap,
    fmt,
    fmt::{Display, Formatter},
    sync::Arc,
};

use chrono::NaiveDateTime;
use tari_common_types::{
    burnt_proof::BurntProof,
    tari_address::TariAddress,
    transaction::{ImportStatus, TxId},
    types::{FixedHash, PrivateKey, PublicKey, Signature},
};
use tari_comms::types::CommsPublicKey;
use tari_core::{
    consensus::{MaxSizeBytes, MaxSizeString},
    mempool::FeePerGramStat,
    proto,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            encrypted_data::PaymentId,
            BuildInfo,
            CodeTemplateRegistration,
            OutputFeatures,
            TemplateType,
            Transaction,
            TransactionOutput,
        },
    },
};
use tari_service_framework::reply_channel::SenderService;
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;
use tower::Service;

use crate::{
    output_manager_service::UtxoSelectionCriteria,
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
#[derive(Clone)]
pub enum TransactionServiceRequest {
    GetPendingInboundTransactions,
    GetPendingOutboundTransactions,
    GetCompletedTransactions,
    GetCancelledPendingInboundTransactions,
    GetCancelledPendingOutboundTransactions,
    GetCancelledCompletedTransactions,
    GetCompletedTransaction(TxId),
    GetAnyTransaction(TxId),
    ImportTransaction(WalletTransaction),
    SendTransaction {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        message: String,
    },
    BurnTari {
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        message: String,
        claim_public_key: Option<PublicKey>,
    },
    CreateNMUtxo {
        amount: MicroMinotari,
        fee_per_gram: MicroMinotari,
        n: u8,
        m: u8,
        public_keys: Vec<PublicKey>,
        message: [u8; 32],
        maturity: u64,
    },
    EncumberAggregateUtxo {
        fee_per_gram: MicroMinotari,
        output_hash: String,
        script_input_shares: Vec<Signature>,
        script_public_key_shares: Vec<PublicKey>,
        script_signature_public_nonces: Vec<PublicKey>,
        sender_offset_public_key_shares: Vec<PublicKey>,
        metadata_ephemeral_public_key_shares: Vec<PublicKey>,
        dh_shared_secret_shares: Vec<PublicKey>,
        recipient_address: TariAddress,
    },
    FinalizeSentAggregateTransaction {
        tx_id: u64,
        total_meta_data_signature: Signature,
        total_script_data_signature: Signature,
        script_offset: PrivateKey,
    },
    RegisterValidatorNode {
        amount: MicroMinotari,
        validator_node_public_key: CommsPublicKey,
        validator_node_signature: Signature,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        message: String,
    },
    RegisterCodeTemplate {
        author_public_key: PublicKey,
        author_signature: Signature,
        template_name: MaxSizeString<32>,
        template_version: u16,
        template_type: TemplateType,
        build_info: BuildInfo,
        binary_sha: MaxSizeBytes<32>,
        binary_url: MaxSizeString<255>,
        fee_per_gram: MicroMinotari,
    },
    SendOneSidedTransaction {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        message: String,
        payment_id: PaymentId,
    },
    SendOneSidedToStealthAddressTransaction {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        message: String,
        payment_id: PaymentId,
    },
    SendShaAtomicSwapTransaction(TariAddress, MicroMinotari, UtxoSelectionCriteria, MicroMinotari, String),
    CancelTransaction(TxId),
    ImportUtxoWithStatus {
        amount: MicroMinotari,
        source_address: TariAddress,
        message: String,
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
        mined_timestamp: Option<NaiveDateTime>,
        scanned_output: TransactionOutput,
        payment_id: PaymentId,
    },
    SubmitTransactionToSelf(TxId, Transaction, MicroMinotari, MicroMinotari, String),
    SetLowPowerMode,
    SetNormalPowerMode,
    RestartTransactionProtocols,
    RestartBroadcastProtocols,
    GetNumConfirmationsRequired,
    SetNumConfirmationsRequired(u64),
    ValidateTransactions,
    ReValidateTransactions,
    /// Returns the fee per gram estimates for the next {count} blocks.
    GetFeePerGramStatsPerBlock {
        count: usize,
    },
}

impl fmt::Display for TransactionServiceRequest {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetPendingInboundTransactions => write!(f, "GetPendingInboundTransactions"),
            Self::GetPendingOutboundTransactions => write!(f, "GetPendingOutboundTransactions"),
            Self::GetCompletedTransactions => write!(f, "GetCompletedTransactions"),
            Self::ImportTransaction(tx) => write!(f, "ImportTransaction: {:?}", tx),
            Self::GetCancelledPendingInboundTransactions => write!(f, "GetCancelledPendingInboundTransactions"),
            Self::GetCancelledPendingOutboundTransactions => write!(f, "GetCancelledPendingOutboundTransactions"),
            Self::GetCancelledCompletedTransactions => write!(f, "GetCancelledCompletedTransactions"),
            Self::GetCompletedTransaction(t) => write!(f, "GetCompletedTransaction({})", t),
            Self::SendTransaction {
                destination,
                amount,
                message,
                ..
            } => write!(
                f,
                "SendTransaction (amount: {}, to: {}, message: {})",
                amount, destination, message
            ),
            Self::BurnTari { amount, message, .. } => write!(f, "Burning Tari ({}, {})", amount, message),
            Self::CreateNMUtxo {
                amount,
                fee_per_gram: _,
                n,
                m,
                public_keys: _,
                message: _,
                maturity: _,
            } => f.write_str(&format!(
                "Creating a new n-of-m aggregate uxto with: amount = {}, n = {}, m = {}",
                amount, n, m
            )),
            Self::EncumberAggregateUtxo {
                fee_per_gram,
                output_hash,
                script_input_shares,
                script_public_key_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address,
                ..
            } => f.write_str(&format!(
                "Creating encumber n-of-m utxo with: fee_per_gram = {}, output_hash = {}, script_input_shares = {:?}, \
                 script_public_key_shares = {:?}, script_signature_shares = {:?}, sender_offset_public_key_shares = \
                 {:?}, metadata_ephemeral_public_key_shares = {:?}, dh_shared_secret_shares = {:?}, recipient_address \
                 = {}",
                fee_per_gram,
                output_hash,
                script_input_shares
                    .iter()
                    .map(|v| format!(
                        "(sig: {}, nonce: {})",
                        v.get_signature().to_hex(),
                        v.get_public_nonce().to_hex()
                    ))
                    .collect::<Vec<String>>(),
                script_public_key_shares
                    .iter()
                    .map(|v| v.to_hex())
                    .collect::<Vec<String>>(),
                script_signature_public_nonces
                    .iter()
                    .map(|v| format!("(public nonce: {})", v.to_hex(),))
                    .collect::<Vec<String>>(),
                sender_offset_public_key_shares
                    .iter()
                    .map(|v| v.to_hex())
                    .collect::<Vec<String>>(),
                metadata_ephemeral_public_key_shares
                    .iter()
                    .map(|v| v.to_hex())
                    .collect::<Vec<String>>(),
                dh_shared_secret_shares
                    .iter()
                    .map(|v| v.to_hex())
                    .collect::<Vec<String>>(),
                recipient_address,
            )),
            Self::FinalizeSentAggregateTransaction {
                tx_id,
                total_meta_data_signature,
                total_script_data_signature,
                script_offset,
            } => f.write_str(&format!(
                "Finalizing encumbered n-of-m tx(#{}) with: meta_sig(sig: {}, nonce: {}), script_sig(sig: {}, nonce: \
                 {}) and script_offset: {}",
                tx_id,
                total_meta_data_signature.get_signature().to_hex(),
                total_meta_data_signature.get_public_nonce().to_hex(),
                total_script_data_signature.get_signature().to_hex(),
                total_script_data_signature.get_public_nonce().to_hex(),
                script_offset.to_hex(),
            )),
            Self::RegisterValidatorNode {
                validator_node_public_key,
                message,
                ..
            } => write!(f, "Registering VN ({}, {})", validator_node_public_key, message),
            Self::SendOneSidedTransaction {
                destination,
                amount,
                message,
                ..
            } => write!(
                f,
                "SendOneSidedTransaction (to {}, {}, {})",
                destination, amount, message
            ),
            Self::SendOneSidedToStealthAddressTransaction {
                destination,
                amount,
                message,
                ..
            } => write!(
                f,
                "SendOneSidedToStealthAddressTransaction (to {}, {}, {})",
                destination, amount, message
            ),
            Self::SendShaAtomicSwapTransaction(k, _, v, _, msg) => {
                write!(f, "SendShaAtomicSwapTransaction (to {}, {}, {})", k, v, msg)
            },
            Self::CancelTransaction(t) => write!(f, "CancelTransaction ({})", t),
            Self::ImportUtxoWithStatus {
                amount,
                source_address,
                message,
                import_status,
                tx_id,
                current_height,
                mined_timestamp,
                ..
            } => write!(
                f,
                "ImportUtxoWithStatus (amount: {}, from: {}, message: {}, import status: {:?}, TxId: {:?}, height: \
                 {:?}, mined at: {:?}",
                amount, source_address, message, import_status, tx_id, current_height, mined_timestamp
            ),
            Self::SubmitTransactionToSelf(tx_id, _, _, _, _) => write!(f, "SubmitTransaction ({})", tx_id),
            Self::SetLowPowerMode => write!(f, "SetLowPowerMode "),
            Self::SetNormalPowerMode => write!(f, "SetNormalPowerMode"),
            Self::RestartTransactionProtocols => write!(f, "RestartTransactionProtocols"),
            Self::RestartBroadcastProtocols => write!(f, "RestartBroadcastProtocols"),
            Self::GetNumConfirmationsRequired => write!(f, "GetNumConfirmationsRequired"),
            Self::SetNumConfirmationsRequired(_) => write!(f, "SetNumConfirmationsRequired"),
            Self::GetAnyTransaction(t) => write!(f, "GetAnyTransaction({})", t),
            Self::ValidateTransactions => write!(f, "ValidateTransactions"),
            Self::ReValidateTransactions => write!(f, "ReValidateTransactions"),
            Self::GetFeePerGramStatsPerBlock { count } => {
                write!(f, "GetFeePerGramEstimatesPerBlock(count: {})", count,)
            },
            TransactionServiceRequest::RegisterCodeTemplate { template_name, .. } => {
                write!(f, "RegisterCodeTemplate: {}", template_name)
            },
        }
    }
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceResponse {
    TransactionSent(TxId),
    TransactionSentWithOutputHash(TxId, FixedHash),
    EncumberAggregateUtxo(TxId, Box<Transaction>, Box<PublicKey>, Box<PublicKey>, Box<PublicKey>),
    TransactionImported(TxId),
    BurntTransactionSent {
        tx_id: TxId,
        proof: Box<BurntProof>,
    },
    TemplateRegistrationTransactionSent {
        tx_id: TxId,
        template_registration: Box<CodeTemplateRegistration>,
    },
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
    ProtocolsRestarted,
    AnyTransaction(Box<Option<WalletTransaction>>),
    NumConfirmationsRequired(u64),
    NumConfirmationsSet,
    ValidationStarted(OperationId),
    CompletedTransactionValidityChanged,
    ShaAtomicSwapTransactionSent(Box<(TxId, PublicKey, TransactionOutput)>),
    FeePerGramStatsPerBlock(FeePerGramStatsResponse),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Default)]
pub struct TransactionSendStatus {
    pub direct_send_result: bool,
    pub store_and_forward_send_result: bool,
    pub queued_for_retry: bool,
}

impl Display for TransactionSendStatus {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "direct('{}')' saf('{}') queued('{}')",
            self.direct_send_result, self.store_and_forward_send_result, self.queued_for_retry,
        )
    }
}

/// Events that can be published on the Text Message Service Event Stream
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum TransactionEvent {
    MempoolBroadcastTimedOut(TxId),
    ReceivedTransaction(TxId),
    ReceivedTransactionReply(TxId),
    ReceivedFinalizedTransaction(TxId),
    TransactionDiscoveryInProgress(TxId),
    TransactionSendResult(TxId, TransactionSendStatus),
    TransactionCompletedImmediately(TxId),
    TransactionCancelled(TxId, TxCancellationReason),
    TransactionBroadcast(TxId),
    TransactionImported(TxId),
    DetectedTransactionUnconfirmed {
        tx_id: TxId,
        num_confirmations: u64,
        is_valid: bool,
    },
    DetectedTransactionConfirmed {
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
    TransactionValidationFailed(OperationId, u64),
    Error(String),
}

impl fmt::Display for TransactionEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TransactionEvent::MempoolBroadcastTimedOut(tx_id) => {
                write!(f, "MempoolBroadcastTimedOut for tx:{tx_id}")
            },
            TransactionEvent::ReceivedTransaction(tx) => {
                write!(f, "ReceivedTransaction for {tx}")
            },
            TransactionEvent::ReceivedTransactionReply(tx) => {
                write!(f, "ReceivedTransactionReply for {tx}")
            },
            TransactionEvent::ReceivedFinalizedTransaction(tx) => {
                write!(f, "ReceivedFinalizedTransaction for {tx}")
            },
            TransactionEvent::TransactionDiscoveryInProgress(tx) => {
                write!(f, "TransactionDiscoveryInProgress for {tx}")
            },
            TransactionEvent::TransactionSendResult(tx, status) => {
                write!(f, "TransactionSendResult for {tx}: {status}")
            },
            TransactionEvent::TransactionCompletedImmediately(tx) => {
                write!(f, "TransactionCompletedImmediately for {tx}")
            },
            TransactionEvent::TransactionCancelled(tx, rejection) => {
                write!(f, "TransactionCancelled for {tx}:{:?}", rejection)
            },
            TransactionEvent::TransactionBroadcast(tx) => {
                write!(f, "TransactionBroadcast for {tx}")
            },
            TransactionEvent::TransactionImported(tx) => {
                write!(f, "TransactionImported for {tx}")
            },
            TransactionEvent::DetectedTransactionUnconfirmed {
                tx_id,
                num_confirmations,
                is_valid,
            } => {
                write!(
                    f,
                    "DetectedTransactionUnconfirmed for {tx_id} with num confirmations: {num_confirmations}. \
                     is_valid: {is_valid}"
                )
            },
            TransactionEvent::DetectedTransactionConfirmed { tx_id, is_valid } => {
                write!(f, "DetectedTransactionConfirmed for {tx_id}. is_valid: {is_valid}")
            },
            TransactionEvent::TransactionMined { tx_id, is_valid } => {
                write!(f, "TransactionMined for {tx_id}. is_valid: {is_valid}")
            },
            TransactionEvent::TransactionMinedRequestTimedOut(tx) => {
                write!(f, "TransactionMinedRequestTimedOut for {tx}")
            },
            TransactionEvent::TransactionMinedUnconfirmed {
                tx_id,
                num_confirmations,
                is_valid,
            } => {
                write!(
                    f,
                    "TransactionMinedUnconfirmed for {tx_id} with num confirmations: {num_confirmations}. is_valid: \
                     {is_valid}",
                )
            },
            TransactionEvent::Error(error) => {
                write!(f, "Error:{error}")
            },
            TransactionEvent::TransactionValidationStateChanged(operation_id) => {
                write!(f, "Transaction validation state changed: {operation_id}")
            },
            TransactionEvent::TransactionValidationCompleted(operation_id) => {
                write!(f, "Transaction validation(#{operation_id}) completed")
            },
            TransactionEvent::TransactionValidationFailed(operation_id, reason) => {
                write!(f, "Transaction validation(#{operation_id}) failed: {reason}")
            },
        }
    }
}

pub type TransactionEventSender = broadcast::Sender<Arc<TransactionEvent>>;
pub type TransactionEventReceiver = broadcast::Receiver<Arc<TransactionEvent>>;

#[derive(Debug, Clone, Default)]
pub struct FeePerGramStatsResponse {
    pub stats: Vec<FeePerGramStat>,
}

impl From<proto::base_node::GetMempoolFeePerGramStatsResponse> for FeePerGramStatsResponse {
    fn from(value: proto::base_node::GetMempoolFeePerGramStatsResponse) -> Self {
        Self {
            stats: value.stats.into_iter().map(Into::into).collect(),
        }
    }
}

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
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        message: String,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendTransaction {
                destination,
                amount,
                selection_criteria,
                output_features: Box::new(output_features),
                fee_per_gram,
                message,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn register_validator_node(
        &mut self,
        amount: MicroMinotari,
        validator_node_public_key: PublicKey,
        validator_node_signature: Signature,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        message: String,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::RegisterValidatorNode {
                amount,
                validator_node_public_key,
                validator_node_signature,
                selection_criteria,
                fee_per_gram,
                message,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn register_code_template(
        &mut self,
        author_public_key: PublicKey,
        author_signature: Signature,
        template_name: MaxSizeString<32>,
        template_version: u16,
        template_type: TemplateType,
        build_info: BuildInfo,
        binary_sha: MaxSizeBytes<32>,
        binary_url: MaxSizeString<255>,
        fee_per_gram: MicroMinotari,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::RegisterCodeTemplate {
                author_public_key,
                author_signature,
                template_name,
                template_version,
                template_type,
                build_info,
                binary_sha,
                binary_url,
                fee_per_gram,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn send_one_sided_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        message: String,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendOneSidedTransaction {
                destination,
                amount,
                selection_criteria,
                output_features: Box::new(output_features),
                fee_per_gram,
                message,
                payment_id,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    /// Burns the given amount of Tari from the wallet
    pub async fn burn_tari(
        &mut self,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        message: String,
        claim_public_key: Option<PublicKey>,
    ) -> Result<(TxId, BurntProof), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::BurnTari {
                amount,
                selection_criteria,
                fee_per_gram,
                message,
                claim_public_key,
            })
            .await??
        {
            TransactionServiceResponse::BurntTransactionSent { tx_id, proof } => Ok((tx_id, *proof)),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn create_aggregate_signature_utxo(
        &mut self,
        amount: MicroMinotari,
        fee_per_gram: MicroMinotari,
        n: u8,
        m: u8,
        public_keys: Vec<PublicKey>,
        message: [u8; 32],
        maturity: u64,
    ) -> Result<(TxId, FixedHash), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::CreateNMUtxo {
                amount,
                fee_per_gram,
                n,
                m,
                public_keys,
                message,
                maturity,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSentWithOutputHash(tx_id, output_hash) => Ok((tx_id, output_hash)),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn encumber_aggregate_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        output_hash: String,
        script_input_shares: Vec<Signature>,
        script_public_key_shares: Vec<PublicKey>,
        script_signature_public_nonces: Vec<PublicKey>,
        sender_offset_public_key_shares: Vec<PublicKey>,
        metadata_ephemeral_public_key_shares: Vec<PublicKey>,
        dh_shared_secret_shares: Vec<PublicKey>,
        recipient_address: TariAddress,
    ) -> Result<(TxId, Transaction, PublicKey, PublicKey, PublicKey), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::EncumberAggregateUtxo {
                fee_per_gram,
                output_hash,
                script_input_shares,
                script_public_key_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address,
            })
            .await??
        {
            TransactionServiceResponse::EncumberAggregateUtxo(
                tx_id,
                transaction,
                total_script_key,
                total_metadata_ephemeral_public_key,
                total_script_nonce,
            ) => Ok((
                tx_id,
                *transaction,
                *total_script_key,
                *total_metadata_ephemeral_public_key,
                *total_script_nonce,
            )),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn finalize_aggregate_utxo(
        &mut self,
        tx_id: u64,
        total_meta_data_signature: Signature,
        total_script_data_signature: Signature,
        script_offset: PrivateKey,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::FinalizeSentAggregateTransaction {
                tx_id,
                total_meta_data_signature,
                total_script_data_signature,
                script_offset,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn send_one_sided_to_stealth_address_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        message: String,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendOneSidedToStealthAddressTransaction {
                destination,
                amount,
                selection_criteria,
                output_features: Box::new(output_features),
                fee_per_gram,
                message,
                payment_id,
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

    pub async fn import_transaction(&mut self, tx: WalletTransaction) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ImportTransaction(tx))
            .await??
        {
            TransactionServiceResponse::TransactionImported(t) => Ok(t),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn import_utxo_with_status(
        &mut self,
        amount: MicroMinotari,
        source_address: TariAddress,
        message: String,
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
        mined_timestamp: Option<NaiveDateTime>,
        scanned_output: TransactionOutput,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ImportUtxoWithStatus {
                amount,
                source_address,
                message,
                import_status,
                tx_id,
                current_height,
                mined_timestamp,
                scanned_output,
                payment_id,
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
        amount: MicroMinotari,
        message: String,
    ) -> Result<(), TransactionServiceError> {
        let fee = tx.body.get_total_fee()?;
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
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        message: String,
    ) -> Result<(TxId, PublicKey, TransactionOutput), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendShaAtomicSwapTransaction(
                destination,
                amount,
                selection_criteria,
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

    /// Query the base node for the fee per gram stats of the next {count} blocks.
    pub async fn get_fee_per_gram_stats_per_block(
        &mut self,
        count: usize,
    ) -> Result<FeePerGramStatsResponse, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetFeePerGramStatsPerBlock { count })
            .await??
        {
            TransactionServiceResponse::FeePerGramStatsPerBlock(resp) => Ok(resp),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }
}
