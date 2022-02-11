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

use aes_gcm::Aes256Gcm;
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, HashOutput, PublicKey},
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
        transaction_protocol::{sender::TransactionSenderMessage, RewindData},
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{script::TariScript, tari_utilities::hex::Hex};
use tari_service_framework::reply_channel::SenderService;
use tokio::sync::broadcast;
use tower::Service;

use crate::output_manager_service::{
    error::OutputManagerError,
    service::Balance,
    storage::{
        models::{KnownOneSidedPaymentScript, SpendingPriority},
        OutputStatus,
    },
};

/// API Request enum
#[allow(clippy::large_enum_variant)]
pub enum OutputManagerRequest {
    GetBalance,
    AddOutput((Box<UnblindedOutput>, Option<SpendingPriority>)),
    AddOutputWithTxId((TxId, Box<UnblindedOutput>, Option<SpendingPriority>)),
    AddRewindableOutputWithTxId((TxId, Box<UnblindedOutput>, Option<SpendingPriority>, Option<RewindData>)),
    AddUnvalidatedOutput((TxId, Box<UnblindedOutput>, Option<SpendingPriority>)),
    UpdateOutputMetadataSignature(Box<TransactionOutput>),
    GetRecipientTransaction(TransactionSenderMessage),
    GetCoinbaseTransaction((TxId, MicroTari, MicroTari, u64)),
    ConfirmPendingTransaction(TxId),
    PrepareToSendTransaction {
        tx_id: TxId,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
        script: TariScript,
        covenant: Covenant,
    },
    CreatePayToSelfTransaction {
        tx_id: TxId,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    },
    CreatePayToSelfWithOutputs {
        outputs: Vec<UnblindedOutputBuilder>,
        fee_per_gram: MicroTari,
        spending_unique_id: Option<Vec<u8>>,
        spending_parent_public_key: Option<PublicKey>,
    },
    CancelTransaction(TxId),
    GetSpentOutputs,
    GetUnspentOutputs,
    GetInvalidOutputs,
    GetSeedWords,
    ValidateUtxos,
    RevalidateTxos,
    CreateCoinSplit((MicroTari, usize, MicroTari, Option<u64>)),
    ApplyEncryption(Box<Aes256Gcm>),
    RemoveEncryption,
    GetPublicRewindKeys,
    FeeEstimate {
        amount: MicroTari,
        fee_per_gram: MicroTari,
        num_kernels: usize,
        num_outputs: usize,
    },
    ScanForRecoverableOutputs {
        outputs: Vec<TransactionOutput>,
        tx_id: TxId,
    },
    ScanOutputs {
        outputs: Vec<TransactionOutput>,
        tx_id: TxId,
    },
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
        use OutputManagerRequest::*;
        match self {
            GetBalance => write!(f, "GetBalance"),
            AddOutput((v, _)) => write!(f, "AddOutput ({})", v.value),
            AddOutputWithTxId((t, v, _)) => write!(f, "AddOutputWithTxId ({}: {})", t, v.value),
            AddRewindableOutputWithTxId((t, v, _, _)) => write!(f, "AddRewindableOutputWithTxId ({}: {})", t, v.value),
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
            GetInvalidOutputs => write!(f, "GetInvalidOutputs"),
            GetSeedWords => write!(f, "GetSeedWords"),
            ValidateUtxos => write!(f, "ValidateUtxos"),
            RevalidateTxos => write!(f, "RevalidateTxos"),
            CreateCoinSplit(v) => write!(f, "CreateCoinSplit ({})", v.0),
            ApplyEncryption(_) => write!(f, "ApplyEncryption"),
            RemoveEncryption => write!(f, "RemoveEncryption"),
            GetCoinbaseTransaction(_) => write!(f, "GetCoinbaseTransaction"),
            GetPublicRewindKeys => write!(f, "GetPublicRewindKeys"),
            FeeEstimate {
                amount,
                fee_per_gram,
                num_kernels,
                num_outputs,
            } => write!(
                f,
                "FeeEstimate(amount: {}, fee_per_gram: {}, num_kernels: {}, num_outputs: {})",
                amount, fee_per_gram, num_kernels, num_outputs
            ),
            ScanForRecoverableOutputs { .. } => write!(f, "ScanForRecoverableOutputs"),
            ScanOutputs { .. } => write!(f, "ScanOutputs"),
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
    InvalidOutputs(Vec<UnblindedOutput>),
    SeedWords(Vec<String>),
    BaseNodePublicKeySet,
    TxoValidationStarted(u64),
    Transaction((TxId, Transaction, MicroTari)),
    EncryptionApplied,
    EncryptionRemoved,
    PublicRewindKeys(Box<PublicRewindKeys>),
    FeeEstimate(MicroTari),
    RewoundOutputs(Vec<UnblindedOutput>),
    ScanOutputs(Vec<UnblindedOutput>),
    AddKnownOneSidedPaymentScript,
    CreateOutputWithFeatures {
        output: Box<UnblindedOutputBuilder>,
    },
    CreatePayToSelfWithOutputs {
        transaction: Box<Transaction>,
        tx_id: TxId,
    },
    ReinstatedCancelledInboundTx,
    CoinbaseAbandonedSet,
    ClaimHtlcTransaction((TxId, MicroTari, MicroTari, Transaction)),
    OutputStatusesByTxId {
        statuses: Vec<OutputStatus>,
        mined_height: Option<u64>,
        block_hash: Option<BlockHash>,
    },
}

pub type OutputManagerEventSender = broadcast::Sender<Arc<OutputManagerEvent>>;
pub type OutputManagerEventReceiver = broadcast::Receiver<Arc<OutputManagerEvent>>;

/// Events that can be published on the Output Manager Service Event Stream
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputManagerEvent {
    TxoValidationSuccess(u64),
    TxoValidationFailure(u64),
    Error(String),
}

impl fmt::Display for OutputManagerEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            OutputManagerEvent::TxoValidationSuccess(tx) => {
                write!(f, "TxoValidationSuccess for {}", tx)
            },
            OutputManagerEvent::TxoValidationFailure(tx) => {
                write!(f, "TxoValidationFailure for {}", tx)
            },
            OutputManagerEvent::Error(error) => {
                write!(f, "Error {}", error)
            },
        }
    }
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
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
        script: TariScript,
        covenant: Covenant,
    ) -> Result<SenderTransactionProtocol, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::PrepareToSendTransaction {
                tx_id,
                amount,
                unique_id,
                parent_public_key,
                fee_per_gram,
                lock_height,
                message,
                script,
                covenant,
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
        fee_per_gram: MicroTari,
        num_kernels: usize,
        num_outputs: usize,
    ) -> Result<MicroTari, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::FeeEstimate {
                amount,
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

    pub async fn validate_txos(&mut self) -> Result<u64, OutputManagerError> {
        match self.handle.call(OutputManagerRequest::ValidateUtxos).await?? {
            OutputManagerResponse::TxoValidationStarted(request_key) => Ok(request_key),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    /// Create a coin split transaction.
    /// Returns (tx_id, tx, utxos_total_value).
    pub async fn create_coin_split(
        &mut self,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
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

    pub async fn scan_for_recoverable_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
        tx_id: TxId,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ScanForRecoverableOutputs { outputs, tx_id })
            .await??
        {
            OutputManagerResponse::RewoundOutputs(outputs) => Ok(outputs),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }

    pub async fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
        tx_id: TxId,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::ScanOutputs { outputs, tx_id })
            .await??
        {
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
        spending_unique_id: Option<Vec<u8>>,
        spending_parent_public_key: Option<PublicKey>,
    ) -> Result<(TxId, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreatePayToSelfWithOutputs {
                outputs,
                fee_per_gram,
                spending_unique_id,
                spending_parent_public_key,
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
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<(MicroTari, Transaction), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::CreatePayToSelfTransaction {
                tx_id,
                amount,
                fee_per_gram,
                lock_height,
                message,
                unique_id,
                parent_public_key,
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
    ) -> Result<(Vec<OutputStatus>, Option<u64>, Option<BlockHash>), OutputManagerError> {
        match self
            .handle
            .call(OutputManagerRequest::GetOutputStatusesByTxId(tx_id))
            .await??
        {
            OutputManagerResponse::OutputStatusesByTxId {
                statuses,
                mined_height,
                block_hash,
            } => Ok((statuses, mined_height, block_hash)),
            _ => Err(OutputManagerError::UnexpectedApiResponse),
        }
    }
}
