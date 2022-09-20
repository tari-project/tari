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

use std::{fmt, fmt::Formatter, sync::Arc};

use chacha20poly1305::XChaCha20Poly1305;
use tari_common_types::{
    transaction::TxId,
    types::{Commitment, HashOutput, PublicKey},
};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{
            OutputFeatures,
            Transaction,
            TransactionOutput,
            UnblindedOutput,
            UnblindedOutputBuilder,
        },
        transaction_protocol::{sender::TransactionSenderMessage, RewindData, TransactionMetadata},
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_script::TariScript;
use tari_service_framework::reply_channel::SenderService;
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;
use tower::Service;

use crate::output_manager_service::{
    error::OutputManagerError,
    service::{Balance, OutputStatusesByTxId},
    storage::{
        database::OutputBackendQuery,
        models::{KnownOneSidedPaymentScript, SpendingPriority},
    },
    UtxoSelectionCriteria,
};

/// API Request enum
#[allow(clippy::large_enum_variant)]
pub enum OutputManagerRequest {
    GetBalance,
    AddOutput((Box<UnblindedOutput>, Option<SpendingPriority>)),
    // ToDo: This API request could probably be removed by expanding test utils if only needed for testing
    AddRewindableOutput((Box<UnblindedOutput>, Option<SpendingPriority>, Option<RewindData>)),
    AddOutputWithTxId((TxId, Box<UnblindedOutput>, Option<SpendingPriority>)),
    AddRewindableOutputWithTxId((TxId, Box<UnblindedOutput>, Option<SpendingPriority>, Option<RewindData>)),
    // ToDo: This API request could probably be removed by expanding test utils if only needed for testing
    ConvertToRewindableTransactionOutput(Box<UnblindedOutput>),
    AddUnvalidatedOutput((TxId, Box<UnblindedOutput>, Option<SpendingPriority>)),
    UpdateOutputMetadataSignature(Box<TransactionOutput>),
    GetRecipientTransaction(TransactionSenderMessage),
    GetCoinbaseTransaction((TxId, MicroTari, MicroTari, u64)),
    ConfirmPendingTransaction(TxId),
    PrepareToSendTransaction {
        tx_id: TxId,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroTari,
        tx_meta: TransactionMetadata,
        message: String,
        script: TariScript,
        covenant: Covenant,
        minimum_value_promise: MicroTari,
    },
    CreatePayToSelfTransaction {
        tx_id: TxId,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    },
    CreatePayToSelfWithOutputs {
        outputs: Vec<UnblindedOutputBuilder>,
        fee_per_gram: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
    },
    CancelTransaction(TxId),
    GetSpentOutputs,
    GetUnspentOutputs,
    GetOutputsBy(OutputBackendQuery),
    GetInvalidOutputs,
    ValidateUtxos,
    RevalidateTxos,
    CreateCoinSplit((Vec<Commitment>, MicroTari, usize, MicroTari)),
    CreateCoinSplitEven((Vec<Commitment>, usize, MicroTari)),
    PreviewCoinJoin((Vec<Commitment>, MicroTari)),
    PreviewCoinSplitEven((Vec<Commitment>, usize, MicroTari)),
    CreateCoinJoin {
        commitments: Vec<Commitment>,
        fee_per_gram: MicroTari,
    },
    ApplyEncryption(Box<XChaCha20Poly1305>),
    RemoveEncryption,
    FeeEstimate {
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
        num_kernels: usize,
        num_outputs: usize,
    },

    ScanForRecoverableOutputs(Vec<TransactionOutput>),
    ScanOutputs(Vec<TransactionOutput>),
    AddKnownOneSidedPaymentScript(KnownOneSidedPaymentScript),
    CreateOutputWithFeatures {
        value: MicroTari,
        features: Box<OutputFeatures>,
    },

    ReinstateCancelledInboundTx(TxId),
    SetCoinbaseAbandoned(TxId, bool),
    CreateClaimShaAtomicSwapTransaction(HashOutput, PublicKey, MicroTari),
    CreateHtlcRefundTransaction(HashOutput, MicroTari),
    GetOutputStatusesByTxId(TxId),
}

impl fmt::Display for OutputManagerRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use OutputManagerRequest::*;
        match self {
            GetBalance => write!(f, "GetBalance"),
            AddOutput((v, _)) => write!(f, "AddOutput ({})", v.value),
            AddRewindableOutput((v, _, _)) => write!(f, "AddRewindableOutput ({})", v.value),
            AddOutputWithTxId((t, v, _)) => write!(f, "AddOutputWithTxId ({}: {})", t, v.value),
            AddRewindableOutputWithTxId((t, v, _, _)) => write!(f, "AddRewindableOutputWithTxId ({}: {})", t, v.value),
            ConvertToRewindableTransactionOutput(v) => write!(f, "GetUnblindedOutputAsRewindableOutput ({})", v.value),
            AddUnvalidatedOutput((t, v, _)) => {
                write!(f, "AddUnvalidatedOutput ({}: {})", t, v.value)
            },
            UpdateOutputMetadataSignature(v) => write!(
                f,
                "UpdateOutputMetadataSignature ({}, {}, {})",
                v.metadata_signature.public_nonce().to_hex(),
                v.metadata_signature.u().to_hex(),
                v.metadata_signature.v().to_hex()
            ),
            GetRecipientTransaction(_) => write!(f, "GetRecipientTransaction"),
            ConfirmPendingTransaction(v) => write!(f, "ConfirmPendingTransaction ({})", v),
            PrepareToSendTransaction { message, .. } => write!(f, "PrepareToSendTransaction ({})", message),
            CreatePayToSelfTransaction { message, .. } => write!(f, "CreatePayToSelfTransaction ({})", message),
            CancelTransaction(v) => write!(f, "CancelTransaction ({})", v),
            GetSpentOutputs => write!(f, "GetSpentOutputs"),
            GetUnspentOutputs => write!(f, "GetUnspentOutputs"),
            GetOutputsBy(q) => write!(f, "GetOutputs({:#?})", q),
            GetInvalidOutputs => write!(f, "GetInvalidOutputs"),
            ValidateUtxos => write!(f, "ValidateUtxos"),
            RevalidateTxos => write!(f, "RevalidateTxos"),
            PreviewCoinJoin((commitments, fee_per_gram)) => write!(
                f,
                "PreviewCoinJoin(commitments={:#?}, fee_per_gram={})",
                commitments, fee_per_gram
            ),
            PreviewCoinSplitEven((commitments, number_of_splits, fee_per_gram)) => write!(
                f,
                "PreviewCoinSplitEven(commitments={:#?}, number_of_splits={}, fee_per_gram={})",
                commitments, number_of_splits, fee_per_gram
            ),
            CreateCoinSplit(v) => write!(f, "CreateCoinSplit ({:?})", v.0),
            CreateCoinSplitEven(v) => write!(f, "CreateCoinSplitEven ({:?})", v.0),
            CreateCoinJoin {
                commitments,
                fee_per_gram,
            } => write!(
                f,
                "CreateCoinJoin: commitments={:#?}, fee_per_gram={}",
                commitments, fee_per_gram,
            ),
            ApplyEncryption(_) => write!(f, "ApplyEncryption"),
            RemoveEncryption => write!(f, "RemoveEncryption"),
            GetCoinbaseTransaction(_) => write!(f, "GetCoinbaseTransaction"),
            FeeEstimate {
                amount,
                selection_criteria,
                fee_per_gram,
                num_kernels,
                num_outputs,
            } => write!(
                f,
                "FeeEstimate(amount: {}, fee_per_gram: {}, num_kernels: {}, num_outputs: {}, selection_criteria: {:?})",
                amount, fee_per_gram, num_kernels, num_outputs, selection_criteria
            ),
            ScanForRecoverableOutputs(_) => write!(f, "ScanForRecoverableOutputs"),
            ScanOutputs(_) => write!(f, "ScanOutputs"),
            AddKnownOneSidedPaymentScript(_) => write!(f, "AddKnownOneSidedPaymentScript"),
            CreateOutputWithFeatures { value, features } => {
                write!(f, "CreateOutputWithFeatures({}, {})", value, features,)
            },
            CreatePayToSelfWithOutputs { .. } => write!(f, "CreatePayToSelfWithOutputs"),
            ReinstateCancelledInboundTx(_) => write!(f, "ReinstateCancelledInboundTx"),
            SetCoinbaseAbandoned(_, _) => write!(f, "SetCoinbaseAbandoned"),
            CreateClaimShaAtomicSwapTransaction(output, pre_image, fee_per_gram) => write!(
                f,
                "ClaimShaAtomicSwap(output hash: {}, pre_image: {}, fee_per_gram: {} )",
                output.to_hex(),
                pre_image,
                fee_per_gram,
            ),
            CreateHtlcRefundTransaction(output, fee_per_gram) => write!(
                f,
                "CreateHtlcRefundTransaction(output hash: {}, , fee_per_gram: {} )",
                output.to_hex(),
                fee_per_gram,
            ),

            GetOutputStatusesByTxId(t) => write!(f, "GetOutputStatusesByTxId: {}", t),
        }
    }
}

/// API Reply enum
#[derive(Debug, Clone)]
pub enum OutputManagerResponse {
    Balance(Balance),
    OutputAdded,
    ConvertedToTransactionOutput(Box<TransactionOutput>),
    OutputMetadataSignatureUpdated,
    RecipientTransactionGenerated(ReceiverTransactionProtocol),
    CoinbaseTransaction(Transaction),
    OutputConfirmed,
    PendingTransactionConfirmed,
    PayToSelfTransaction((MicroTari, Transaction)),
    TransactionToSend(SenderTransactionProtocol),
    TransactionCancelled,
    SpentOutputs(Vec<UnblindedOutput>),
    UnspentOutputs(Vec<UnblindedOutput>),
    Outputs(Vec<UnblindedOutput>),
    InvalidOutputs(Vec<UnblindedOutput>),
    BaseNodePublicKeySet,
    TxoValidationStarted(u64),
    Transaction((TxId, Transaction, MicroTari)),
    EncryptionApplied,
    EncryptionRemoved,
    PublicRewindKeys(Box<PublicRewindKeys>),
    RecoveryByte(u8),
    FeeEstimate(MicroTari),
    RewoundOutputs(Vec<RecoveredOutput>),
    ScanOutputs(Vec<RecoveredOutput>),
    AddKnownOneSidedPaymentScript,
    CreateOutputWithFeatures { output: Box<UnblindedOutputBuilder> },
    CreatePayToSelfWithOutputs { transaction: Box<Transaction>, tx_id: TxId },
    ReinstatedCancelledInboundTx,
    CoinbaseAbandonedSet,
    ClaimHtlcTransaction((TxId, MicroTari, MicroTari, Transaction)),
    OutputStatusesByTxId(OutputStatusesByTxId),
    CoinPreview((Vec<MicroTari>, MicroTari)),
}

pub type OutputManagerEventSender = broadcast::Sender<Arc<OutputManagerEvent>>;
pub type OutputManagerEventReceiver = broadcast::Receiver<Arc<OutputManagerEvent>>;

/// Events that can be published on the Output Manager Service Event Stream
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputManagerEvent {
    TxoValidationSuccess(u64),
    TxoValidationInternalFailure(u64),
    TxoValidationCommunicationFailure(u64),
}

impl fmt::Display for OutputManagerEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            OutputManagerEvent::TxoValidationSuccess(tx) => {
                write!(f, "TxoValidationSuccess for {}", tx)
            },
            OutputManagerEvent::TxoValidationInternalFailure(tx) => {
                write!(f, "TxoValidationInternalFailure for {}", tx)
            },
            OutputManagerEvent::TxoValidationCommunicationFailure(tx) => {
                write!(f, "TxoValidationCommunicationFailure for {}", tx)
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct PublicRewindKeys {
    pub rewind_blinding_public_key: PublicKey,
}

#[derive(Debug, Clone)]
pub struct RecoveredOutput {
    pub tx_id: TxId,
    pub output: UnblindedOutput,
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
    ) -> Self {
        OutputManagerHandle {
            handle,
            event_stream_sender,
        }
    }

    pub fn get_event_stream(&self) -> OutputManagerEventReceiver {
        self.event_stream_sender.subscribe()
    }

    pub async fn add_output(
        &mut self,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddOutput((Box::new(output), spend_priority)))
            .await??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    // ToDo: This API method call could probably be removed by expanding test utils if only needed for testing
    pub async fn add_rewindable_output(
        &mut self,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
        custom_rewind_data: Option<RewindData>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddRewindableOutput((
                Box::new(output),
                spend_priority,
                custom_rewind_data,
            )))
            .await??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn add_output_with_tx_id(
        &mut self,
        tx_id: TxId,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddOutputWithTxId((
                tx_id,
                Box::new(output),
                spend_priority,
            )))
            .await??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn add_rewindable_output_with_tx_id(
        &mut self,
        tx_id: TxId,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
        custom_rewind_data: Option<RewindData>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddRewindableOutputWithTxId((
                tx_id,
                Box::new(output),
                spend_priority,
                custom_rewind_data,
            )))
            .await??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    // ToDo: This API method call could probably be removed by expanding test utils if only needed for testing
    pub async fn convert_to_rewindable_transaction_output(
        &mut self,
        output: UnblindedOutput,
    ) -> Result<TransactionOutput, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ConvertToRewindableTransactionOutput(Box::new(
                output,
            )))
            .await??
        {
            OutputManagerResponse::ConvertedToTransactionOutput(val) => Ok(*val),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn add_unvalidated_output(
        &mut self,
        tx_id: TxId,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddUnvalidatedOutput((
                tx_id,
                Box::new(output),
                spend_priority,
            )))
            .await??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_output_with_features(
        &mut self,
        value: MicroTari,
        features: OutputFeatures,
    ) -> Result<UnblindedOutputBuilder, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateOutputWithFeatures {
                value,
                features: Box::new(features),
            })
            .await??
        {
            OutputManagerResponse::CreateOutputWithFeatures { output } => Ok(*output),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn update_output_metadata_signature(
        &mut self,
        output: TransactionOutput,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::UpdateOutputMetadataSignature(Box::new(output)))
            .await??
        {
            OutputManagerResponse::OutputMetadataSignatureUpdated => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_balance(&mut self) -> Result<Balance, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetBalance).await?? {
            OutputManagerResponse::Balance(b) => Ok(b),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn revalidate_all_outputs(&mut self) -> Result<u64, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::RevalidateTxos).await?? {
            OutputManagerResponse::TxoValidationStarted(request_key) => Ok(request_key),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_recipient_transaction(
        &mut self,
        sender_message: TransactionSenderMessage,
    ) -> Result<ReceiverTransactionProtocol, OutputManagerError> {
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
    ) -> Result<Transaction, OutputManagerError> {
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
        tx_id: TxId,
        amount: MicroTari,
        utxo_selection: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroTari,
        tx_meta: TransactionMetadata,
        message: String,
        script: TariScript,
        covenant: Covenant,
        minimum_value_promise: MicroTari,
    ) -> Result<SenderTransactionProtocol, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::PrepareToSendTransaction {
                tx_id,
                amount,
                selection_criteria: utxo_selection,
                output_features: Box::new(output_features),
                fee_per_gram,
                tx_meta,
                message,
                script,
                covenant,
                minimum_value_promise,
            })
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
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
        num_kernels: usize,
        num_outputs: usize,
    ) -> Result<MicroTari, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::FeeEstimate {
                amount,
                selection_criteria,
                fee_per_gram,
                num_kernels,
                num_outputs,
            })
            .await??
        {
            OutputManagerResponse::FeeEstimate(fee) => Ok(fee),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn confirm_pending_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ConfirmPendingTransaction(tx_id))
            .await??
        {
            OutputManagerResponse::PendingTransactionConfirmed => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CancelTransaction(tx_id))
            .await??
        {
            OutputManagerResponse::TransactionCancelled => Ok(()),
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

    // ToDo: This API method call could probably be removed by expanding test utils if only needed for testing
    pub async fn get_invalid_outputs(&mut self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::GetInvalidOutputs).await?? {
            OutputManagerResponse::InvalidOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn validate_txos(&mut self) -> Result<u64, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::ValidateUtxos).await?? {
            OutputManagerResponse::TxoValidationStarted(request_key) => Ok(request_key),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn preview_coin_join_with_commitments(
        &mut self,
        commitments: Vec<Commitment>,
        fee_per_gram: MicroTari,
    ) -> Result<(Vec<MicroTari>, MicroTari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::PreviewCoinJoin((commitments, fee_per_gram)))
            .await??
        {
            OutputManagerResponse::CoinPreview((expected_outputs, fee)) => Ok((expected_outputs, fee)),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn preview_coin_split_with_commitments_no_amount(
        &mut self,
        commitments: Vec<Commitment>,
        split_count: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(Vec<MicroTari>, MicroTari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::PreviewCoinSplitEven((
                commitments,
                split_count,
                fee_per_gram,
            )))
            .await??
        {
            OutputManagerResponse::CoinPreview((expected_outputs, fee)) => Ok((expected_outputs, fee)),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    /// Create a coin split transaction.
    /// Returns (tx_id, tx, utxos_total_value).
    pub async fn create_coin_split(
        &mut self,
        commitments: Vec<Commitment>,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateCoinSplit((
                commitments,
                amount_per_split,
                split_count,
                fee_per_gram,
            )))
            .await??
        {
            OutputManagerResponse::Transaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_coin_split_even(
        &mut self,
        commitments: Vec<Commitment>,
        split_count: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateCoinSplitEven((
                commitments,
                split_count,
                fee_per_gram,
            )))
            .await??
        {
            OutputManagerResponse::Transaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_coin_join(
        &mut self,
        commitments: Vec<Commitment>,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateCoinJoin {
                commitments,
                fee_per_gram,
            })
            .await??
        {
            OutputManagerResponse::Transaction(result) => Ok(result),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_htlc_refund_transaction(
        &mut self,
        output: HashOutput,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, MicroTari, MicroTari, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateHtlcRefundTransaction(output, fee_per_gram))
            .await??
        {
            OutputManagerResponse::ClaimHtlcTransaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_claim_sha_atomic_swap_transaction(
        &mut self,
        output: HashOutput,
        pre_image: PublicKey,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, MicroTari, MicroTari, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateClaimShaAtomicSwapTransaction(
                output,
                pre_image,
                fee_per_gram,
            ))
            .await??
        {
            OutputManagerResponse::ClaimHtlcTransaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn apply_encryption(&mut self, cipher: XChaCha20Poly1305) -> Result<(), OutputManagerError> {
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

    pub async fn scan_for_recoverable_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ScanForRecoverableOutputs(outputs))
            .await??
        {
            OutputManagerResponse::RewoundOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::ScanOutputs(outputs)).await?? {
            OutputManagerResponse::ScanOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn add_known_script(&mut self, script: KnownOneSidedPaymentScript) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddKnownOneSidedPaymentScript(script))
            .await??
        {
            OutputManagerResponse::AddKnownOneSidedPaymentScript => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_send_to_self_with_output(
        &mut self,
        outputs: Vec<UnblindedOutputBuilder>,
        fee_per_gram: MicroTari,
        input_selection: UtxoSelectionCriteria,
    ) -> Result<(TxId, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreatePayToSelfWithOutputs {
                outputs,
                fee_per_gram,
                selection_criteria: input_selection,
            })
            .await??
        {
            OutputManagerResponse::CreatePayToSelfWithOutputs { transaction, tx_id } => Ok((tx_id, *transaction)),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn create_pay_to_self_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        utxo_selection: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<(MicroTari, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreatePayToSelfTransaction {
                tx_id,
                amount,
                selection_criteria: utxo_selection,
                output_features: Box::new(output_features),
                fee_per_gram,
                lock_height,
                message,
            })
            .await??
        {
            OutputManagerResponse::PayToSelfTransaction(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn reinstate_cancelled_inbound_transaction_outputs(
        &mut self,
        tx_id: TxId,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ReinstateCancelledInboundTx(tx_id))
            .await??
        {
            OutputManagerResponse::ReinstatedCancelledInboundTx => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn set_coinbase_abandoned(&mut self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::SetCoinbaseAbandoned(tx_id, abandoned))
            .await??
        {
            OutputManagerResponse::CoinbaseAbandonedSet => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn get_output_statuses_by_tx_id(
        &mut self,
        tx_id: TxId,
    ) -> Result<OutputStatusesByTxId, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetOutputStatusesByTxId(tx_id))
            .await??
        {
            OutputManagerResponse::OutputStatusesByTxId(output_statuses_by_tx_id) => Ok(output_statuses_by_tx_id),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }
}
