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
use std::{collections::HashMap, fmt, fmt::Formatter, ops::Range, sync::Arc};

use log::warn;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{CompressedCommitment, CompressedPublicKey, FixedHash, HashOutput},
};
use tari_script::{CompressedCheckSigSchnorrSignature, TariScript};
use tari_service_framework::reply_channel::SenderService;
use tari_transaction_components::{
    transaction_components::{
        covenants::Covenant,
        MemoField,
        OutputFeatures,
        Transaction,
        TransactionOutput,
        WalletOutput,
        WalletOutputBuilder,
    },
    MicroMinotari,
    TransactionBuilder,
};
use tari_transaction_key_manager::legacy_key_manager::{wallet_types::FeeType, LegacyTransactionKeyManagerInterface};
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;
use tower::Service;

use crate::output_manager_service::{
    error::OutputManagerError,
    service::{Balance, OutputInfoByTxId, UseOutput},
    storage::{
        database::OutputBackendQuery,
        models::{DbWalletOutput, KnownOneSidedPaymentScript, SpendingPriority},
        sqlite_db::CoinBucket,
    },
    UtxoSelectionCriteria,
};

const LOG_TARGET: &str = "wallet::output_manager_service::handle";

/// API Request enum
pub enum OutputManagerRequest {
    GetBalance,
    GetCoinBuckets {
        ranges: Vec<Range<u64>>,
    },
    GetBalancePaymentId(Vec<u8>),
    AddOutput((Box<WalletOutput>, Option<SpendingPriority>)),
    AddOutputWithTxId((TxId, Box<WalletOutput>, Option<SpendingPriority>)),
    AddUnvalidatedOutput((TxId, Box<WalletOutput>, Option<SpendingPriority>)),
    UpdateOutputMetadataSignature(Box<TransactionOutput>),
    ConfirmPendingTransaction {
        tx_id: TxId,
        tx_id_update: Option<TxId>,
        change_outputs: Option<Vec<WalletOutput>>,
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
        payment_id: MemoField,
    },
    SpendBackupPreMineUtxo {
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
    },
    GetTransactionBuilder {
        tx_id: TxId,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        script: TariScript,
        covenant: Covenant,
    },
    GetTransactionBuilderRangeLimitedCoinJoin {
        tx_id: TxId,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee: FeeType,
        script: TariScript,
        covenant: Covenant,
    },
    CreatePayToSelfTransaction {
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        lock_height: Option<u64>,
        payment_id: MemoField,
        minimum_value_promise: MicroMinotari,
    },
    CancelTransaction(TxId),
    GetSpentOutputs,
    GetOutputsByQuery(OutputBackendQuery),
    GetUnspentOutputs,
    GetInvalidOutputs,
    GetManyOutputs {
        outputs: Vec<FixedHash>,
    },
    ValidateTxos,
    RevalidateTxos,
    CreateCoinSplit((Vec<CompressedCommitment>, MicroMinotari, usize, MicroMinotari)),
    CreateCoinSplitEven((Vec<CompressedCommitment>, usize, MicroMinotari)),
    PreviewCoinJoin((Vec<CompressedCommitment>, MicroMinotari)),
    PreviewCoinSplitEven((Vec<CompressedCommitment>, usize, MicroMinotari)),
    ScrapeWallet {
        tx_id: TxId,
        fee_per_gram: MicroMinotari,
    },
    CreateCoinJoin {
        commitments: Vec<CompressedCommitment>,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    },
    FeeEstimate {
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        num_kernels: usize,
        num_outputs: usize,
    },

    ScanForRecoverableOutputs(Vec<TransactionOutput>),
    ScanOutputs(Vec<TransactionOutput>),
    ScanOutputsForMultisig(Vec<TransactionOutput>),
    AddKnownOneSidedPaymentScript(KnownOneSidedPaymentScript),
    CreateOutputWithFeatures {
        value: MicroMinotari,
        features: Box<OutputFeatures>,
    },

    ReinstateCancelledInboundTx(TxId),
    CreateClaimShaAtomicSwapTransaction(HashOutput, CompressedPublicKey, MicroMinotari),
    CreateHtlcRefundTransaction(HashOutput, MicroMinotari),
    GetOutputInfoByTxId(TxId),
    FetchUnspentOutputs(Vec<HashOutput>),
    ClearShortTermEncumberances,
}

impl fmt::Display for OutputManagerRequest {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use OutputManagerRequest::*;
        match self {
            GetBalance => write!(f, "GetBalance"),
            GetCoinBuckets { ranges } => {
                let buckets = ranges
                    .iter()
                    .map(|v| format!("range: {}..{}", v.start, v.end))
                    .collect::<Vec<_>>();
                write!(f, "GetCoinBuckets: buckets {:?}", buckets)
            },
            GetBalancePaymentId(_) => write!(f, "GetBalance for user payment id"),
            AddOutput((v, _)) => write!(f, "AddOutput ({})", v.value()),
            AddOutputWithTxId((t, v, _)) => write!(f, "AddOutputWithTxId ({}: {})", t, v.value()),
            AddUnvalidatedOutput((t, v, _)) => {
                write!(f, "AddUnvalidatedOutput ({}: {})", t, v.value())
            },
            UpdateOutputMetadataSignature(v) => write!(
                f,
                "UpdateOutputMetadataSignature ({}, {}, {}, {}, {})",
                v.metadata_signature.ephemeral_commitment().to_hex(),
                v.metadata_signature.ephemeral_pubkey().to_hex(),
                v.metadata_signature.u_x().to_hex(),
                v.metadata_signature.u_y().to_hex(),
                v.metadata_signature.u_a().to_hex(),
            ),
            ScrapeWallet { tx_id, fee_per_gram } => {
                write!(f, "ScrapeWallet (tx_id: {tx_id}, fee_per_gram: {fee_per_gram})")
            },
            EncumberAggregateUtxo {
                expected_commitment,
                original_maturity,
                use_output,
                ..
            } => {
                let output_hash = match use_output {
                    UseOutput::FromBlockchain(hash) => *hash,
                    UseOutput::AsProvided(output) => output.hash(),
                };
                write!(
                    f,
                    "Encumber aggregate utxo with output: ({},{}) with original maturity: {}",
                    expected_commitment.to_hex(),
                    output_hash,
                    original_maturity,
                )
            },
            SpendBackupPreMineUtxo {
                output_hash,
                expected_commitment,
                ..
            } => write!(
                f,
                "spending backup pre-mine utxo with output: ({},{})",
                expected_commitment.to_hex(),
                output_hash
            ),
            ConfirmPendingTransaction {
                tx_id, tx_id_update, ..
            } => {
                write!(f, "ConfirmPendingTransaction ({tx_id} replace with {:?})", tx_id_update)
            },
            GetTransactionBuilder { .. } => write!(f, "GetTransactionBuilder "),
            GetTransactionBuilderRangeLimitedCoinJoin { .. } => write!(f, "GetTransactionBuilderRangeLimitedCoinJoin "),
            CreatePayToSelfTransaction { .. } => write!(f, "CreatePayToSelfTransaction",),
            CancelTransaction(v) => write!(f, "CancelTransaction ({v})"),
            GetSpentOutputs => write!(f, "GetSpentOutputs"),
            GetUnspentOutputs => write!(f, "GetUnspentOutputs"),
            GetInvalidOutputs => write!(f, "GetInvalidOutputs"),
            ValidateTxos => write!(f, "ValidateUtxos"),
            RevalidateTxos => write!(f, "RevalidateTxos"),
            PreviewCoinJoin((commitments, fee_per_gram)) => write!(
                f,
                "PreviewCoinJoin(commitments={commitments:#?}, fee_per_gram={fee_per_gram})"
            ),
            PreviewCoinSplitEven((commitments, number_of_splits, fee_per_gram)) => write!(
                f,
                "PreviewCoinSplitEven(commitments={commitments:#?}, number_of_splits={number_of_splits}, \
                 fee_per_gram={fee_per_gram})"
            ),
            CreateCoinSplit(v) => write!(f, "CreateCoinSplit ({:?})", v.0),
            CreateCoinSplitEven(v) => write!(f, "CreateCoinSplitEven ({:?})", v.0),
            CreateCoinJoin {
                commitments,
                fee_per_gram,
                ..
            } => write!(
                f,
                "CreateCoinJoin: commitments={commitments:#?}, fee_per_gram={fee_per_gram}"
            ),
            FeeEstimate {
                amount,
                selection_criteria,
                fee_per_gram,
                num_kernels,
                num_outputs,
            } => write!(
                f,
                "FeeEstimate(amount: {amount}, fee_per_gram: {fee_per_gram}, num_kernels: {num_kernels}, num_outputs: \
                 {num_outputs}, selection_criteria: {selection_criteria:?})"
            ),
            ScanForRecoverableOutputs(_) => write!(f, "ScanForRecoverableOutputs"),
            ScanOutputs(_) => write!(f, "ScanOutputs"),
            AddKnownOneSidedPaymentScript(_) => write!(f, "AddKnownOneSidedPaymentScript"),
            CreateOutputWithFeatures { value, features } => {
                write!(f, "CreateOutputWithFeatures({value}, {features})")
            },
            ReinstateCancelledInboundTx(_) => write!(f, "ReinstateCancelledInboundTx"),
            CreateClaimShaAtomicSwapTransaction(output, pre_image, fee_per_gram) => write!(
                f,
                "ClaimShaAtomicSwap(output hash: {output}, pre_image: {pre_image}, fee_per_gram: {fee_per_gram} )"
            ),
            CreateHtlcRefundTransaction(output, fee_per_gram) => write!(
                f,
                "CreateHtlcRefundTransaction(output hash: {output}, , fee_per_gram: {fee_per_gram} )"
            ),

            GetOutputInfoByTxId(t) => write!(f, "GetOutputInfoByTxId: {}", t),
            FetchUnspentOutputs(hashes) => write!(f, "FetchUnspentOutputs: {:?}", hashes),
            ClearShortTermEncumberances => write!(f, "ClearShortTermEncumberances"),
            GetOutputsByQuery(query) => write!(f, "GetOutputsByQuery: {:?}", query),
            ScanOutputsForMultisig(_) => write!(f, "ScanOutputsForMultisig"),
            GetManyOutputs { outputs } => write!(f, "GetManyOutputs ({})", outputs.len()),
        }
    }
}

/// API Reply enum
#[derive(Debug, Clone)]
pub enum OutputManagerResponse<KM> {
    Balance(Balance),
    GetCoinBuckets(Vec<CoinBucket>),
    GetRangeLimitedOutputs(Vec<DbWalletOutput>),
    OutputAdded,
    ConvertedToTransactionOutput(Box<TransactionOutput>),
    OutputMetadataSignatureUpdated,
    TxIdReplaced,
    // RecipientTransactionGenerated(ReceiverTransactionProtocol),
    EncumberAggregateUtxo {
        tx_id: TxId,
        transaction: Box<Transaction>,
        amount: MicroMinotari,
        fee: MicroMinotari,
        total_script_public_key: Box<CompressedPublicKey>,
        total_metadata_ephemeral_public_key: Box<CompressedPublicKey>,
        total_script_nonce: Box<CompressedPublicKey>,
        shared_secret_public_key: Box<CompressedPublicKey>,
    },
    SpendBackupPreMineUtxo((TxId, Transaction, MicroMinotari, MicroMinotari)),
    OutputConfirmed,
    PendingTransactionConfirmed,
    PayToSelfTransaction((MicroMinotari, Transaction, TxId)),
    TransactionBuilderToSend(Box<TransactionBuilder<KM>>),
    TransactionCancelled,
    SpentOutputs(Vec<DbWalletOutput>),
    UnspentOutputs(Vec<DbWalletOutput>),
    Outputs(Vec<WalletOutput>),
    InvalidOutputs(Vec<WalletOutput>),
    BaseNodePublicKeySet,
    TxoValidationStarted(u64),
    Transaction((TxId, Transaction, MicroMinotari)),
    PublicRewindKeys(Box<PublicRewindKeys>),
    RecoveryByte(u8),
    FeeEstimate(MicroMinotari, usize, bool),
    RewoundOutputs(Vec<RecoveredOutput>),
    ScanOutputs(Vec<RecoveredOutput>),
    AddKnownOneSidedPaymentScript,
    CreateOutputWithFeatures {
        output: Box<WalletOutputBuilder>,
    },
    CreatePayToSelfWithOutputs {
        transaction: Box<Transaction>,
        tx_id: TxId,
    },
    ReinstatedCancelledInboundTx,
    ClaimHtlcTransaction((TxId, MicroMinotari, MicroMinotari, Transaction)),
    OutputInfoByTxId(OutputInfoByTxId),
    CoinPreview((Vec<MicroMinotari>, MicroMinotari)),
    FetchUnspentOutputs(Vec<TransactionOutput>),
    ConfirmEncumberance,
    ClearShortTermEncumberances,
}

pub type OutputManagerEventSender = broadcast::Sender<Arc<OutputManagerEvent>>;
pub type OutputManagerEventReceiver = broadcast::Receiver<Arc<OutputManagerEvent>>;

/// Events that can be published on the Output Manager Service Event Stream
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputManagerEvent {
    TxoValidationSuccess(u64),
    TxoValidationInternalFailure(u64),
    TxoValidationCommunicationFailure(u64),
    TxoValidationAlreadyBusy(u64),
}

impl fmt::Display for OutputManagerEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            OutputManagerEvent::TxoValidationSuccess(tx) => {
                write!(f, "TxoValidationSuccess for {tx}")
            },
            OutputManagerEvent::TxoValidationInternalFailure(tx) => {
                write!(f, "TxoValidationInternalFailure for {tx}")
            },
            OutputManagerEvent::TxoValidationCommunicationFailure(tx) => {
                write!(f, "TxoValidationCommunicationFailure for {tx}")
            },
            OutputManagerEvent::TxoValidationAlreadyBusy(tx) => {
                write!(f, "Txo is already running, stopping {tx}")
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct PublicRewindKeys {
    pub rewind_blinding_public_key: CompressedPublicKey,
}

#[derive(Debug, Clone)]
pub struct RecoveredOutput {
    pub output: WalletOutput,
    pub hash: FixedHash,
}

#[derive(Clone)]
pub struct OutputManagerHandle<KM> {
    handle: SenderService<OutputManagerRequest, Result<OutputManagerResponse<KM>, OutputManagerError>>,
    event_stream_sender: OutputManagerEventSender,
}

impl<KM> OutputManagerHandle<KM>
where KM: LegacyTransactionKeyManagerInterface
{
    pub fn new(
        handle: SenderService<OutputManagerRequest, Result<OutputManagerResponse<KM>, OutputManagerError>>,
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
        output: WalletOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddOutput((Box::new(output), spend_priority)))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::AddOutput({e})"))??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::AddOutput".to_string(),
            )),
        }
    }

    pub async fn add_output_with_tx_id(
        &mut self,
        tx_id: TxId,
        output: WalletOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddOutputWithTxId((
                tx_id,
                Box::new(output),
                spend_priority,
            )))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::AddOutputWithTxId({e})"))??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::AddOutputWithTxId".to_string(),
            )),
        }
    }

    pub async fn add_unvalidated_output(
        &mut self,
        tx_id: TxId,
        output: WalletOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddUnvalidatedOutput((
                tx_id,
                Box::new(output),
                spend_priority,
            )))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::AddUnvalidatedOutput({e})"))??
        {
            OutputManagerResponse::OutputAdded => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::AddUnvalidatedOutput".to_string(),
            )),
        }
    }

    pub async fn create_output_with_features(
        &mut self,
        value: MicroMinotari,
        features: OutputFeatures,
    ) -> Result<WalletOutputBuilder, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateOutputWithFeatures {
                value,
                features: Box::new(features),
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::CreateOutputWithFeatures({e})"))??
        {
            OutputManagerResponse::CreateOutputWithFeatures { output } => Ok(*output),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CreateOutputWithFeatures".to_string(),
            )),
        }
    }

    pub async fn update_output_metadata_signature(
        &mut self,
        output: TransactionOutput,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::UpdateOutputMetadataSignature(Box::new(output)))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::UpdateOutputMetadataSignature({e})"))??
        {
            OutputManagerResponse::OutputMetadataSignatureUpdated => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::UpdateOutputMetadataSignature".to_string(),
            )),
        }
    }

    pub async fn get_balance(&mut self) -> Result<Balance, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetBalance)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetBalance({e})"))??
        {
            OutputManagerResponse::Balance(b) => Ok(b),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetBalance".to_string(),
            )),
        }
    }

    pub async fn count_outputs_in_ranges(
        &mut self,
        ranges: Vec<Range<u64>>,
    ) -> Result<Vec<CoinBucket>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetCoinBuckets { ranges })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetCoinBuckets({e})"))??
        {
            OutputManagerResponse::GetCoinBuckets(b) => Ok(b),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetCoinBuckets".to_string(),
            )),
        }
    }

    pub async fn get_balance_for_payment_id(&mut self, payment_id: Vec<u8>) -> Result<Balance, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetBalancePaymentId(payment_id))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetBalancePaymentId({e})"))??
        {
            OutputManagerResponse::Balance(b) => Ok(b),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetBalancePaymentId".to_string(),
            )),
        }
    }

    pub async fn revalidate_all_outputs(&mut self) -> Result<u64, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::RevalidateTxos)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::RevalidateTxos({e})"))??
        {
            OutputManagerResponse::TxoValidationStarted(request_key) => Ok(request_key),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::RevalidateTxos".to_string(),
            )),
        }
    }

    pub async fn prepare_transaction_to_send(
        &mut self,
        tx_id: TxId,
        amount: MicroMinotari,
        utxo_selection: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        script: TariScript,
        covenant: Covenant,
    ) -> Result<TransactionBuilder<KM>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetTransactionBuilder {
                tx_id,
                amount,
                selection_criteria: utxo_selection,
                output_features: Box::new(output_features),
                fee_per_gram,
                script,
                covenant,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetTransactionBuilder({e})"))??
        {
            OutputManagerResponse::TransactionBuilderToSend(stp) => Ok(*stp),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetTransactionBuilder".to_string(),
            )),
        }
    }

    pub async fn prepare_range_limited_coin_join_transaction_to_send(
        &mut self,
        tx_id: TxId,
        utxo_selection: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee: FeeType,
        script: TariScript,
        covenant: Covenant,
    ) -> Result<TransactionBuilder<KM>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetTransactionBuilderRangeLimitedCoinJoin {
                tx_id,
                selection_criteria: utxo_selection,
                output_features: Box::new(output_features),
                fee,
                script,
                covenant,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetTransactionBuilder({e})"))??
        {
            OutputManagerResponse::TransactionBuilderToSend(stp) => Ok(*stp),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetTransactionBuilder".to_string(),
            )),
        }
    }

    pub async fn scrape_wallet(
        &mut self,
        tx_id: TxId,
        fee_per_gram: MicroMinotari,
    ) -> Result<TransactionBuilder<KM>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ScrapeWallet { tx_id, fee_per_gram })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ScrapeWallet({e})"))??
        {
            OutputManagerResponse::TransactionBuilderToSend(tx_builder) => Ok(*tx_builder),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ScrapeWallet".to_string(),
            )),
        }
    }

    /// Get a fee estimate for an amount of MicroMinotari, at a specified fee per gram and given number of kernels and
    /// outputs.
    pub async fn fee_estimate(
        &mut self,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        num_kernels: usize,
        num_outputs: usize,
    ) -> Result<(MicroMinotari, usize, bool), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::FeeEstimate {
                amount,
                selection_criteria,
                fee_per_gram,
                num_kernels,
                num_outputs,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::FeeEstimate({e})"))??
        {
            OutputManagerResponse::FeeEstimate(fee, number_selected, change) => Ok((fee, number_selected, change)),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::FeeEstimate".to_string(),
            )),
        }
    }

    pub async fn confirm_pending_transaction(
        &mut self,
        tx_id: TxId,
        tx_id_update: Option<TxId>,
        change_outputs: Option<Vec<WalletOutput>>,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ConfirmPendingTransaction {
                tx_id,
                tx_id_update,
                change_outputs,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ConfirmPendingTransaction({e})"))??
        {
            OutputManagerResponse::PendingTransactionConfirmed => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ConfirmPendingTransaction".to_string(),
            )),
        }
    }

    pub async fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CancelTransaction(tx_id))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::CancelTransaction({e})"))??
        {
            OutputManagerResponse::TransactionCancelled => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CancelTransaction".to_string(),
            )),
        }
    }

    pub async fn get_many_outputs(&mut self, outputs: Vec<FixedHash>) -> Result<Vec<WalletOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetManyOutputs { outputs })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetManyOutputs({e})"))??
        {
            OutputManagerResponse::Outputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetManyOutputs".to_string(),
            )),
        }
    }

    pub async fn get_spent_outputs(&mut self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetSpentOutputs)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetSpentOutputs({e})"))??
        {
            OutputManagerResponse::SpentOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetSpentOutputs".to_string(),
            )),
        }
    }

    /// Sorted from lowest value to highest
    pub async fn get_unspent_outputs(&mut self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetUnspentOutputs)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetUnspentOutputs({e})"))??
        {
            OutputManagerResponse::UnspentOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetUnspentOutputs".to_string(),
            )),
        }
    }

    /// Sorted from lowest value to highest
    pub async fn get_outputs_by_query(
        &mut self,
        query: OutputBackendQuery,
    ) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetOutputsByQuery(query))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetOutputsByQuery({e})"))??
        {
            OutputManagerResponse::UnspentOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetOutputsByQuery".to_string(),
            )),
        }
    }

    pub async fn get_invalid_outputs(&mut self) -> Result<Vec<WalletOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetInvalidOutputs)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetInvalidOutputs({e})"))??
        {
            OutputManagerResponse::InvalidOutputs(s) => Ok(s),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetInvalidOutputs".to_string(),
            )),
        }
    }

    pub async fn validate_txos(&mut self) -> Result<u64, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ValidateTxos)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ValidateTxos({e})"))??
        {
            OutputManagerResponse::TxoValidationStarted(request_key) => Ok(request_key),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ValidateTxos".to_string(),
            )),
        }
    }

    pub async fn preview_coin_join_with_commitments(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        fee_per_gram: MicroMinotari,
    ) -> Result<(Vec<MicroMinotari>, MicroMinotari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::PreviewCoinJoin((commitments, fee_per_gram)))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::PreviewCoinJoin({e})"))??
        {
            OutputManagerResponse::CoinPreview((expected_outputs, fee)) => Ok((expected_outputs, fee)),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::PreviewCoinJoin".to_string(),
            )),
        }
    }

    pub async fn preview_coin_split_with_commitments_no_amount(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        split_count: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(Vec<MicroMinotari>, MicroMinotari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::PreviewCoinSplitEven((
                commitments,
                split_count,
                fee_per_gram,
            )))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::PreviewCoinSplitEven({e})"))??
        {
            OutputManagerResponse::CoinPreview((expected_outputs, fee)) => Ok((expected_outputs, fee)),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::PreviewCoinSplitEven".to_string(),
            )),
        }
    }

    /// Create a coin split transaction.
    /// Returns (tx_id, tx, utxos_total_value).
    pub async fn create_coin_split(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        amount_per_split: MicroMinotari,
        split_count: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateCoinSplit((
                commitments,
                amount_per_split,
                split_count,
                fee_per_gram,
            )))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::CreateCoinSplit({e})"))??
        {
            OutputManagerResponse::Transaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CreateCoinSplit".to_string(),
            )),
        }
    }

    pub async fn create_coin_split_even(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        split_count: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateCoinSplitEven((
                commitments,
                split_count,
                fee_per_gram,
            )))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::CreateCoinSplitEven({e})"))??
        {
            OutputManagerResponse::Transaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CreateCoinSplitEven".to_string(),
            )),
        }
    }

    pub async fn create_coin_join(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateCoinJoin {
                commitments,
                fee_per_gram,
                payment_id,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::CreateCoinJoin({e})"))??
        {
            OutputManagerResponse::Transaction(result) => Ok(result),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CreateCoinJoin".to_string(),
            )),
        }
    }

    pub async fn create_htlc_refund_transaction(
        &mut self,
        output: HashOutput,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, MicroMinotari, MicroMinotari, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateHtlcRefundTransaction(output, fee_per_gram))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::CreateHtlcRefundTransaction({e})"))??
        {
            OutputManagerResponse::ClaimHtlcTransaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CreateHtlcRefundTransaction".to_string(),
            )),
        }
    }

    pub async fn create_claim_sha_atomic_swap_transaction(
        &mut self,
        output: HashOutput,
        pre_image: CompressedPublicKey,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, MicroMinotari, MicroMinotari, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreateClaimShaAtomicSwapTransaction(
                output,
                pre_image,
                fee_per_gram,
            ))
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "OutputManagerRequest::CreateClaimShaAtomicSwapTransaction({e})"),
            )?? {
            OutputManagerResponse::ClaimHtlcTransaction(ct) => Ok(ct),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CreateClaimShaAtomicSwapTransaction".to_string(),
            )),
        }
    }

    pub async fn scan_for_recoverable_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ScanForRecoverableOutputs(outputs))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ScanForRecoverableOutputs({e})"))??
        {
            OutputManagerResponse::RewoundOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ScanForRecoverableOutputs".to_string(),
            )),
        }
    }

    pub async fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ScanOutputs(outputs))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ScanOutputs({e})"))??
        {
            OutputManagerResponse::ScanOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ScanOutputs".to_string(),
            )),
        }
    }

    pub async fn scan_outputs_for_multisig(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ScanOutputsForMultisig(outputs))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ScanOutputsForMultisig({e})"))??
        {
            OutputManagerResponse::ScanOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ScanOutputsForMultisig".to_string(),
            )),
        }
    }

    pub async fn add_known_script(&mut self, script: KnownOneSidedPaymentScript) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::AddKnownOneSidedPaymentScript(script))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::AddKnownOneSidedPaymentScript({e})"))??
        {
            OutputManagerResponse::AddKnownOneSidedPaymentScript => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::AddKnownOneSidedPaymentScript".to_string(),
            )),
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
        payment_id: MemoField,
    ) -> Result<
        (
            TxId,
            Transaction,
            MicroMinotari,
            MicroMinotari,
            CompressedPublicKey,
            CompressedPublicKey,
            CompressedPublicKey,
            CompressedPublicKey,
        ),
        OutputManagerError,
    > {
        match self
            .handle
            .call(OutputManagerRequest::EncumberAggregateUtxo {
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::EncumberAggregateUtxo({e})"))??
        {
            OutputManagerResponse::EncumberAggregateUtxo {
                tx_id,
                transaction,
                amount,
                fee,
                total_script_public_key,
                total_metadata_ephemeral_public_key,
                total_script_nonce,
                shared_secret_public_key,
            } => Ok((
                tx_id,
                *transaction,
                amount,
                fee,
                *total_script_public_key,
                *total_metadata_ephemeral_public_key,
                *total_script_nonce,
                *shared_secret_public_key,
            )),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::EncumberAggregateUtxo".to_string(),
            )),
        }
    }

    pub async fn spend_backup_pre_mine_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
    ) -> Result<(TxId, Transaction, MicroMinotari, MicroMinotari), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::SpendBackupPreMineUtxo {
                fee_per_gram,
                output_hash,
                expected_commitment,
                recipient_address,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::SpendBackupPreMineUtxo({e})"))??
        {
            OutputManagerResponse::SpendBackupPreMineUtxo((tx_id, transaction, amount, fee)) => {
                Ok((tx_id, transaction, amount, fee))
            },
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::SpendBackupPreMineUtxo".to_string(),
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_pay_to_self_transaction(
        &mut self,
        amount: MicroMinotari,
        utxo_selection: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        lock_height: Option<u64>,
        payment_id: MemoField,
        minimum_value_promise: MicroMinotari,
    ) -> Result<(MicroMinotari, Transaction, TxId), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreatePayToSelfTransaction {
                amount,
                selection_criteria: utxo_selection,
                output_features: Box::new(output_features),
                fee_per_gram,
                lock_height,
                payment_id,
                minimum_value_promise,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::CreatePayToSelfTransaction({e})"))??
        {
            OutputManagerResponse::PayToSelfTransaction(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::CreatePayToSelfTransaction".to_string(),
            )),
        }
    }

    pub async fn reinstate_cancelled_inbound_transaction_outputs(
        &mut self,
        tx_id: TxId,
    ) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ReinstateCancelledInboundTx(tx_id))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ReinstateCancelledInboundTx({e})"))??
        {
            OutputManagerResponse::ReinstatedCancelledInboundTx => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ReinstateCancelledInboundTx".to_string(),
            )),
        }
    }

    pub async fn get_output_info_for_tx_id(&mut self, tx_id: TxId) -> Result<OutputInfoByTxId, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetOutputInfoByTxId(tx_id))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::GetOutputInfoByTxId({e})"))??
        {
            OutputManagerResponse::OutputInfoByTxId(output_info_by_tx_id) => Ok(output_info_by_tx_id),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::GetOutputInfoByTxId".to_string(),
            )),
        }
    }

    pub async fn fetch_unspent_outputs_from_node(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::FetchUnspentOutputs(hashes))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::FetchUnspentOutputs({e})"))??
        {
            OutputManagerResponse::FetchUnspentOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::FetchUnspentOutputs".to_string(),
            )),
        }
    }

    pub async fn clear_short_term_encumberances(&mut self) -> Result<(), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ClearShortTermEncumberances)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "OutputManagerRequest::ClearShortTermEncumberances({e})"))??
        {
            OutputManagerResponse::ClearShortTermEncumberances => Ok(()),
            _ => Err(OutputManagerError::UnexpectedApiResponse(
                "OutputManagerRequest::ClearShortTermEncumberances".to_string(),
            )),
        }
    }
}
