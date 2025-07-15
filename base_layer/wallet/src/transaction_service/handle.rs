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

use chrono::{DateTime, Utc};
use tari_common_types::{
    burnt_proof::BurntProof,
    epoch::VnEpoch,
    tari_address::TariAddress,
    transaction::{ImportStatus, TransactionDirection, TxId},
    types::{CompressedCommitment, CompressedPublicKey, FixedHash, HashOutput, PrivateKey, Signature},
};
use tari_comms::types::CommsPublicKey;
use tari_core::{
    mempool::FeePerGramStat,
    proto,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            payment_id::PaymentId,
            BuildInfo,
            CodeTemplateRegistration,
            OutputFeatures,
            TemplateType,
            Transaction,
            TransactionOutput,
        },
    },
};
use tari_max_size::MaxSizeString;
use tari_script::CompressedCheckSigSchnorrSignature;
use tari_service_framework::reply_channel::SenderService;
use tari_sidechain::EvictionProof;
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;
use tower::Service;

use crate::{
    output_manager_service::{service::UseOutput, UtxoSelectionCriteria},
    transaction_service::{
        error::TransactionServiceError,
        offline_signing::models::{PrepareOneSidedTransactionForSigningResult, SignedOneSidedTransactionResult},
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
    GetCompletedTransactions {
        payment_id: Option<Vec<u8>>,
        block_hash: Option<FixedHash>,
        block_height: Option<u64>,
        max_limit: u64,
    },
    GetCompletedTransactionsByAddresses {
        source_address: Option<TariAddress>,
        destination_address: Option<TariAddress>,
    },
    GetCancelledPendingInboundTransactions,
    GetCancelledPendingOutboundTransactions,
    GetCancelledCompletedTransactions(u64),
    GetCompletedTransaction(TxId),
    GetAnyTransaction(TxId),
    ImportTransaction(WalletTransaction),
    SendTransaction {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    },
    BurnTari {
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
        claim_public_key: Option<CompressedPublicKey>,
        sidechain_deployment_key: Option<PrivateKey>,
    },
    EncumberAggregateUtxo {
        fee_per_gram: MicroMinotari,
        expected_commitment: CompressedCommitment,
        script_input_shares: HashMap<CompressedPublicKey, CompressedCheckSigSchnorrSignature>,
        script_signature_public_nonces: Vec<CompressedPublicKey>,
        sender_offset_public_key_shares: Vec<CompressedPublicKey>,
        metadata_ephemeral_public_key_shares: Vec<CompressedPublicKey>,
        dh_shared_secret_shares: Vec<CompressedPublicKey>,
        recipient_address: TariAddress,
        original_maturity: u64,
        use_output: UseOutput,
        payment_id: PaymentId,
    },
    SpendBackupPreMineUtxo {
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        payment_id: PaymentId,
    },
    FetchUnspentOutputs {
        output_hashes: Vec<HashOutput>,
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
        validator_node_claim_public_key: CommsPublicKey,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    },
    SubmitValidatorNodeExit {
        amount: MicroMinotari,
        validator_node_public_key: CommsPublicKey,
        validator_node_signature: Signature,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    },
    RegisterCodeTemplate {
        template_name: MaxSizeString<32>,
        template_version: u16,
        template_type: TemplateType,
        build_info: BuildInfo,
        binary_sha: FixedHash,
        binary_url: MaxSizeString<255>,
        fee_per_gram: MicroMinotari,
        sidechain_deployment_key: Option<PrivateKey>,
    },
    SubmitValidatorEvictionProof {
        amount: MicroMinotari,
        proof: EvictionProof,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
        sidechain_deployment_key: Option<PrivateKey>,
    },
    PrepareOneSidedTransactionForSigning {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    },
    SignOneSidedTransaction {
        request: PrepareOneSidedTransactionForSigningResult,
    },
    BroadcastSignedOneSidedTransaction {
        request: SignedOneSidedTransactionResult,
    },
    SendOneSidedTransaction {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    },
    SendOneSidedToStealthAddressTransaction {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    },
    ScrapeWallet {
        destination: TariAddress,
        fee_per_gram: MicroMinotari,
    },
    SendShaAtomicSwapTransaction(
        TariAddress,
        MicroMinotari,
        UtxoSelectionCriteria,
        MicroMinotari,
        PaymentId,
    ),
    CancelTransaction(TxId),
    ImportUtxoWithStatus {
        amount: MicroMinotari,
        source_address: TariAddress,
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
        mined_timestamp: Option<DateTime<Utc>>,
        scanned_output: TransactionOutput,
        payment_id: PaymentId,
    },
    SubmitTransactionToSelf(TxId, Transaction, MicroMinotari, MicroMinotari, PaymentId),
    SetLowPowerMode,
    SetNormalPowerMode,
    RestartTransactionProtocols,
    RestartBroadcastProtocols,
    GetNumConfirmationsRequired,
    SetNumConfirmationsRequired(u64),
    ValidateTransactions,
    ReValidateRejectedTransactions,
    /// Returns the fee per gram estimates for the next {count} blocks.
    GetFeePerGramStatsPerBlock {
        count: u64,
    },
    /// Get transaction details for a PayRef (enhanced with multiple recipients)
    GetPaymentByReference {
        payref: FixedHash,
    },
    /// Get all transactions with their PayRefs (for listing/filtering)
    GetTransactionByPaymentReference(FixedHash),
}

impl fmt::Display for TransactionServiceRequest {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetPendingInboundTransactions => write!(f, "GetPendingInboundTransactions"),
            Self::GetPendingOutboundTransactions => write!(f, "GetPendingOutboundTransactions"),
            Self::GetCompletedTransactions { .. } => write!(f, "GetCompletedTransactions"),
            Self::GetCompletedTransactionsByAddresses { .. } => write!(f, "GetCompletedTransactionsByAddresses"),
            Self::ImportTransaction(tx) => write!(f, "ImportTransaction: {:?}", tx),
            Self::GetCancelledPendingInboundTransactions => write!(f, "GetCancelledPendingInboundTransactions"),
            Self::GetCancelledPendingOutboundTransactions => write!(f, "GetCancelledPendingOutboundTransactions"),
            Self::GetCancelledCompletedTransactions(_) => write!(f, "GetCancelledCompletedTransactions"),
            Self::GetCompletedTransaction(t) => write!(f, "GetCompletedTransaction({})", t),
            Self::ScrapeWallet {
                destination,
                fee_per_gram,
            } => {
                write!(
                    f,
                    "ScrapeWallet (destination: {}, fee_per_gram: {})",
                    destination, fee_per_gram
                )
            },
            Self::SendTransaction {
                destination,
                amount,
                payment_id,
                ..
            } => write!(
                f,
                "SendTransaction (amount: {}, to: {}, payment_id: {})",
                amount, destination, payment_id
            ),
            Self::BurnTari { amount, payment_id, .. } => write!(f, "Burning Tari ({}, {})", amount, payment_id),
            Self::SpendBackupPreMineUtxo {
                fee_per_gram,
                output_hash,
                expected_commitment,
                recipient_address,
                payment_id,
            } => f.write_str(&format!(
                "Spending backup pre-mine utxo with: fee_per_gram = {}, output_hash = {}, commitment = {}, recipient \
                 = {}, payment_id = {}",
                fee_per_gram,
                output_hash,
                expected_commitment.to_hex(),
                recipient_address,
                payment_id,
            )),
            Self::EncumberAggregateUtxo {
                fee_per_gram,
                expected_commitment,
                script_input_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address,
                original_maturity,
                use_output,
                payment_id,
                ..
            } => {
                let output_hash = match use_output {
                    UseOutput::FromBlockchain(hash) => *hash,
                    UseOutput::AsProvided(output) => output.hash(),
                };
                f.write_str(&format!(
                    "Creating encumber n-of-m utxo with: fee_per_gram = {}, output_hash = {}, commitment = {}, \
                     script_input_shares = {:?}, script_signature_shares = {:?}, sender_offset_public_key_shares = \
                     {:?}, metadata_ephemeral_public_key_shares = {:?}, dh_shared_secret_shares = {:?}, \
                     recipient_address = {}, original_maturity: {}, payment_id: {}",
                    fee_per_gram,
                    output_hash,
                    expected_commitment.to_hex(),
                    script_input_shares
                        .iter()
                        .map(|v| format!(
                            "(public_key: {}, sig: {}, nonce: {})",
                            v.0.to_hex(),
                            v.1.get_signature().to_hex(),
                            v.1.get_compressed_public_nonce().to_hex()
                        ))
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
                    original_maturity,
                    payment_id,
                ))
            },
            Self::FetchUnspentOutputs { output_hashes } => {
                write!(
                    f,
                    "FetchUnspentOutputs({:?})",
                    output_hashes.iter().map(|v| v.to_hex()).collect::<Vec<String>>()
                )
            },
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
                total_meta_data_signature.get_compressed_public_nonce().to_hex(),
                total_script_data_signature.get_signature().to_hex(),
                total_script_data_signature.get_compressed_public_nonce().to_hex(),
                script_offset.to_hex(),
            )),
            Self::RegisterValidatorNode {
                validator_node_public_key,
                payment_id,
                max_epoch,
                ..
            } => write!(
                f,
                "Registering VN ({}, {}, {})",
                validator_node_public_key, payment_id, max_epoch
            ),
            Self::SubmitValidatorNodeExit {
                validator_node_public_key,
                payment_id,
                max_epoch,
                ..
            } => write!(
                f,
                "Submit VN Exit ({}, {}, {})",
                validator_node_public_key, payment_id, max_epoch
            ),
            Self::PrepareOneSidedTransactionForSigning {
                destination,
                amount,
                payment_id,
                ..
            } => write!(
                f,
                "PrepareOneSidedTransactionForSigning (to {}, {}, {})",
                destination, amount, payment_id
            ),
            Self::SignOneSidedTransaction { request } => write!(f, "SignOneSidedTransaction (request {:?})", request,),
            Self::BroadcastSignedOneSidedTransaction { request } => {
                write!(f, "BroadcastSignedOneSidedTransaction (request {:?})", request,)
            },
            Self::SendOneSidedTransaction {
                destination,
                amount,
                payment_id,
                ..
            } => write!(
                f,
                "SendOneSidedTransaction (to {}, {}, {})",
                destination, amount, payment_id
            ),
            Self::SendOneSidedToStealthAddressTransaction {
                destination,
                amount,
                payment_id,
                ..
            } => write!(
                f,
                "SendOneSidedToStealthAddressTransaction (to {}, {}, {})",
                destination, amount, payment_id
            ),
            Self::SendShaAtomicSwapTransaction(k, _, v, _, id) => {
                write!(f, "SendShaAtomicSwapTransaction (to {}, {}, {})", k, v, id)
            },
            Self::CancelTransaction(t) => write!(f, "CancelTransaction ({})", t),
            Self::ImportUtxoWithStatus {
                amount,
                source_address,
                import_status,
                tx_id,
                current_height,
                mined_timestamp,
                payment_id,
                ..
            } => write!(
                f,
                "ImportUtxoWithStatus (amount: {}, from: {}, payment_id: {}, import status: {:?}, TxId: {:?}, height: \
                 {:?}, mined at: {:?}",
                amount, source_address, payment_id, import_status, tx_id, current_height, mined_timestamp
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
            Self::ReValidateRejectedTransactions => write!(f, "ReValidateRejectedTransactions"),
            Self::GetFeePerGramStatsPerBlock { count } => {
                write!(f, "GetFeePerGramEstimatesPerBlock(count: {})", count,)
            },
            Self::RegisterCodeTemplate { template_name, .. } => {
                write!(f, "RegisterCodeTemplate: {}", template_name)
            },
            Self::GetPaymentByReference { payref } => {
                write!(f, "GetPaymentByReference({})", payref)
            },
            Self::GetTransactionByPaymentReference(payref) => {
                write!(f, "GetTransactionByPaymentReference({})", payref)
            },
            Self::SubmitValidatorEvictionProof {
                amount,
                proof,
                fee_per_gram,
                payment_id,
                ..
            } => {
                write!(
                    f,
                    "SubmitValidatorEvictionProof (amount: {}, evicts: {}, fee_per_gram: {}, message: {})",
                    amount,
                    proof.node_to_evict(),
                    fee_per_gram,
                    payment_id
                )
            },
        }
    }
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceResponse {
    TransactionSent(TxId),
    TransactionSentWithOutputHash(TxId, FixedHash),
    EncumberAggregateUtxo(
        TxId,
        Box<Transaction>,
        Box<CompressedPublicKey>,
        Box<CompressedPublicKey>,
        Box<CompressedPublicKey>,
        Box<CompressedPublicKey>,
    ),
    UnspentOutputs(Vec<TransactionOutput>),
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
    PendingInboundTransactions(Vec<InboundTransaction>),
    PendingOutboundTransactions(Vec<OutboundTransaction>),
    CompletedTransactions(Vec<CompletedTransaction>),
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
    ShaAtomicSwapTransactionSent(Box<(TxId, CompressedPublicKey, TransactionOutput)>),
    FeePerGramStatsPerBlock(FeePerGramStat),
    /// Response containing PayRefs for a transaction
    TransactionPayRefs(Vec<FixedHash>),
    /// Response containing payment details for a PayRef
    PaymentDetails(Option<PaymentDetails>),
    OneSidedTransactionPreparedForSigning(Box<PrepareOneSidedTransactionForSigningResult>),
    SignedOneSidedTransaction(Box<SignedOneSidedTransactionResult>),
    CodeRegistrationTransactionSent {
        tx_id: TxId,
        template_address: FixedHash,
    },
    ValidatorEvictionProofSent {
        tx_id: TxId,
    },
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
    ReceivedTransaction(TxId),
    ReceivedTransactionReply(TxId),
    ReceivedFinalizedTransaction(TxId),
    TransactionDiscoveryInProgress(TxId),
    TransactionSendResult(TxId, TransactionSendStatus),
    TransactionCompletedImmediately(TxId),
    TransactionCancelled(TxId, TxCancellationReason),
    TransactionBroadcast(TxId),
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
    TransactionMinedUnconfirmed {
        tx_id: TxId,
        num_confirmations: u64,
        is_valid: bool,
    },
    TransactionImported(TxId),
    TransactionValidationStateChanged(OperationId),
    TransactionValidationCompleted(OperationId),
    TransactionValidationFailed(OperationId, u64),
    Error(String),
}

impl fmt::Display for TransactionEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
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
            TransactionEvent::TransactionImported(tx) => {
                write!(f, "TransactionImported for {tx}")
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

/// Enhanced payment details for PayRef functionality
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentDetails {
    pub payment_reference: FixedHash,
    pub amount: MicroMinotari,
    pub direction: TransactionDirection,
    pub block_height: u64,
    pub confirmations: u64,
    pub timestamp: Option<DateTime<Utc>>,
    pub payment_id: Option<Vec<u8>>,
    pub tx_id: TxId,
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
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendTransaction {
                destination,
                amount,
                selection_criteria,
                output_features: Box::new(output_features),
                fee_per_gram,
                payment_id,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn scrape_wallet(
        &mut self,
        destination: TariAddress,
        fee_per_gram: MicroMinotari,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ScrapeWallet {
                destination,
                fee_per_gram,
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
        validator_node_public_key: CompressedPublicKey,
        validator_node_signature: Signature,
        validator_node_claim_public_key: CompressedPublicKey,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::RegisterValidatorNode {
                amount,
                validator_node_public_key,
                validator_node_signature,
                validator_node_claim_public_key,
                sidechain_deployment_key,
                max_epoch,
                selection_criteria,
                fee_per_gram,
                payment_id,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn submit_validator_node_exit(
        &mut self,
        amount: MicroMinotari,
        validator_node_public_key: CompressedPublicKey,
        validator_node_signature: Signature,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SubmitValidatorNodeExit {
                amount,
                validator_node_public_key,
                validator_node_signature,
                sidechain_deployment_key,
                max_epoch,
                selection_criteria,
                fee_per_gram,
                payment_id,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn register_code_template(
        &mut self,
        template_name: MaxSizeString<32>,
        template_version: u16,
        template_type: TemplateType,
        build_info: BuildInfo,
        binary_sha: FixedHash,
        binary_url: MaxSizeString<255>,
        fee_per_gram: MicroMinotari,
        sidechain_deployment_key: Option<PrivateKey>,
    ) -> Result<(TxId, FixedHash), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::RegisterCodeTemplate {
                template_name,
                template_version,
                template_type,
                build_info,
                binary_sha,
                binary_url,
                fee_per_gram,
                sidechain_deployment_key,
            })
            .await??
        {
            TransactionServiceResponse::CodeRegistrationTransactionSent {
                tx_id,
                template_address,
            } => Ok((tx_id, template_address)),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn submit_validator_eviction_proof(
        &mut self,
        amount: MicroMinotari,
        proof: EvictionProof,
        fee_per_gram: MicroMinotari,
        sidechain_deployment_key: Option<PrivateKey>,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SubmitValidatorEvictionProof {
                amount,
                proof,
                fee_per_gram,
                payment_id,
                sidechain_deployment_key,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn prepare_one_sided_transaction_for_signing(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        payment_id: PaymentId,
    ) -> Result<PrepareOneSidedTransactionForSigningResult, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::PrepareOneSidedTransactionForSigning {
                destination,
                amount,
                selection_criteria,
                output_features: Box::new(output_features),
                fee_per_gram,
                payment_id,
            })
            .await??
        {
            TransactionServiceResponse::OneSidedTransactionPreparedForSigning(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn sign_one_sided_transaction(
        &mut self,
        request: PrepareOneSidedTransactionForSigningResult,
    ) -> Result<SignedOneSidedTransactionResult, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SignOneSidedTransaction { request })
            .await??
        {
            TransactionServiceResponse::SignedOneSidedTransaction(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn broadcast_signed_one_sided_transaction(
        &mut self,
        request: SignedOneSidedTransactionResult,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::BroadcastSignedOneSidedTransaction { request })
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
        payment_id: PaymentId,
        claim_public_key: Option<CompressedPublicKey>,
        sidechain_deployment_key: Option<PrivateKey>,
    ) -> Result<(TxId, BurntProof), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::BurnTari {
                amount,
                selection_criteria,
                fee_per_gram,
                payment_id,
                claim_public_key,
                sidechain_deployment_key,
            })
            .await??
        {
            TransactionServiceResponse::BurntTransactionSent { tx_id, proof } => Ok((tx_id, *proof)),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    #[allow(clippy::mutable_key_type)]
    pub async fn encumber_aggregate_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        expected_commitment: CompressedCommitment,
        script_input_shares: HashMap<CompressedPublicKey, CompressedCheckSigSchnorrSignature>,
        script_signature_public_nonces: Vec<CompressedPublicKey>,
        sender_offset_public_key_shares: Vec<CompressedPublicKey>,
        metadata_ephemeral_public_key_shares: Vec<CompressedPublicKey>,
        dh_shared_secret_shares: Vec<CompressedPublicKey>,
        recipient_address: TariAddress,
        original_maturity: u64,
        use_output: UseOutput,
        payment_id: PaymentId,
    ) -> Result<
        (
            TxId,
            Transaction,
            CompressedPublicKey,
            CompressedPublicKey,
            CompressedPublicKey,
            CompressedPublicKey,
        ),
        TransactionServiceError,
    > {
        match self
            .handle
            .call(TransactionServiceRequest::EncumberAggregateUtxo {
                fee_per_gram,
                expected_commitment,
                script_input_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address,
                original_maturity,
                use_output,
                payment_id,
            })
            .await??
        {
            TransactionServiceResponse::EncumberAggregateUtxo(
                tx_id,
                transaction,
                total_script_key,
                total_metadata_ephemeral_public_key,
                total_script_nonce,
                shared_secret,
            ) => Ok((
                tx_id,
                *transaction,
                *total_script_key,
                *total_metadata_ephemeral_public_key,
                *total_script_nonce,
                *shared_secret,
            )),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn spend_backup_pre_mine_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SpendBackupPreMineUtxo {
                fee_per_gram,
                output_hash,
                expected_commitment,
                recipient_address,
                payment_id,
            })
            .await??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn fetch_unspent_outputs(
        &mut self,
        output_hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::FetchUnspentOutputs { output_hashes })
            .await??
        {
            TransactionServiceResponse::UnspentOutputs(outputs) => Ok(outputs),
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
    ) -> Result<Vec<InboundTransaction>, TransactionServiceError> {
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
    ) -> Result<Vec<InboundTransaction>, TransactionServiceError> {
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
    ) -> Result<Vec<OutboundTransaction>, TransactionServiceError> {
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
    ) -> Result<Vec<OutboundTransaction>, TransactionServiceError> {
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
        payment_id: Option<Vec<u8>>,
        block_hash: Option<FixedHash>,
        block_height: Option<u64>,
        max_limit: u64,
    ) -> Result<Vec<CompletedTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCompletedTransactions {
                payment_id,
                block_hash,
                block_height,
                max_limit,
            })
            .await??
        {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_completed_transactions_by_addresses(
        &mut self,
        source_address: Option<TariAddress>,
        destination_address: Option<TariAddress>,
    ) -> Result<Vec<CompletedTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCompletedTransactionsByAddresses {
                source_address,
                destination_address,
            })
            .await??
        {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    pub async fn get_cancelled_completed_transactions(
        &mut self,
        max_limit: u64,
    ) -> Result<Vec<CompletedTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCancelledCompletedTransactions(max_limit))
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
        import_status: ImportStatus,
        tx_id: Option<TxId>,
        current_height: Option<u64>,
        mined_timestamp: Option<DateTime<Utc>>,
        scanned_output: TransactionOutput,
        payment_id: PaymentId,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ImportUtxoWithStatus {
                amount,
                source_address,
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
        payment_id: PaymentId,
    ) -> Result<(), TransactionServiceError> {
        let fee = tx.body.get_total_fee()?;
        match self
            .handle
            .call(TransactionServiceRequest::SubmitTransactionToSelf(
                tx_id, tx, fee, amount, payment_id,
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

    pub async fn revalidate_rejected_transactions(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ReValidateRejectedTransactions)
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
        payment_id: PaymentId,
    ) -> Result<(TxId, CompressedPublicKey, TransactionOutput), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendShaAtomicSwapTransaction(
                destination,
                amount,
                selection_criteria,
                fee_per_gram,
                payment_id,
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
        count: u64,
    ) -> Result<FeePerGramStat, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetFeePerGramStatsPerBlock { count })
            .await??
        {
            TransactionServiceResponse::FeePerGramStatsPerBlock(resp) => Ok(resp),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    /// Get details for a PayRef (enhanced with multiple recipients)
    pub async fn get_payment_by_reference(
        &mut self,
        payref: FixedHash,
    ) -> Result<Option<PaymentDetails>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetPaymentByReference { payref })
            .await??
        {
            TransactionServiceResponse::PaymentDetails(details) => Ok(details),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }

    /// Get a transaction by PayRef
    pub async fn get_transaction_by_payref(
        &mut self,
        payref: FixedHash,
    ) -> Result<CompletedTransaction, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetTransactionByPaymentReference(payref))
            .await??
        {
            TransactionServiceResponse::CompletedTransaction(tx) => Ok(*tx),
            _ => Err(TransactionServiceError::UnexpectedApiResponse),
        }
    }
}
