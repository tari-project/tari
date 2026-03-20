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
use log::warn;
use tari_common_types::{
    burn_proof::PartialBurnClaimProof,
    epoch::VnEpoch,
    tari_address::TariAddress,
    transaction::{LegacyImportStatus, TransactionDirection, TxId},
    types::{CompressedCommitment, CompressedPublicKey, CompressedSignature, FixedHash, HashOutput, PrivateKey},
};
use tari_comms::types::CommsPublicKey;
use tari_max_size::MaxSizeString;
use tari_script::CompressedCheckSigSchnorrSignature;
use tari_service_framework::reply_channel::SenderService;
use tari_sidechain::EvictionProof;
use tari_transaction_components::{
    MicroMinotari,
    multisig::types::{CreateMultisigUtxo, GetMultisigUtxoDataOutput, WithdrawMultisigUtxo},
    offline_signing::models::{
        PrepareDepositMultisigTransactionResult,
        PrepareOneSidedTransactionForSigningResult,
        PrepareWithdrawMultisigTransactionResult,
        SignedOneSidedDepositMultisigTransactionResult,
        SignedOneSidedTransactionResult,
        SignedOneSidedWithdrawMultisigTransactionResult,
    },
    rpc::models::FeePerGramStat,
    transaction_components::{
        BuildInfo,
        CodeTemplateRegistration,
        MemoField,
        OutputFeatures,
        TemplateType,
        Transaction,
        TransactionOutput,
    },
};
use tari_transaction_key_manager::legacy_key_manager::wallet_types::FeeType;
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;
use tower::Service;

use crate::{
    OperationId,
    output_manager_service::{UtxoSelectionCriteria, service::UseOutput},
    storage::sqlite_db::models::DbBurnProof,
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
};

const LOG_TARGET: &str = "wallet::transaction_service::handle";

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
    GetCompletedTransactionsPaginated {
        offset: u64,
        limit: u64,
        status_filter: Option<u64>,
    },
    GetCancelledPendingInboundTransactions,
    GetCancelledPendingOutboundTransactions,
    GetCancelledCompletedTransactions(u64),
    GetCompletedTransaction(TxId),
    GetAnyTransaction(TxId),
    ImportTransaction(WalletTransaction),
    BurnTari {
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
        payment_id: MemoField,
    },
    SpendBackupPreMineUtxo {
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        payment_id: MemoField,
    },
    FetchUnspentOutputs {
        output_hashes: Vec<HashOutput>,
    },
    FinalizeSentAggregateTransaction {
        tx_id: u64,
        total_meta_data_signature: CompressedSignature,
        total_script_data_signature: CompressedSignature,
        script_offset: PrivateKey,
    },
    RegisterValidatorNode {
        amount: MicroMinotari,
        validator_node_public_key: CommsPublicKey,
        validator_node_signature: CompressedSignature,
        validator_node_claim_public_key: CommsPublicKey,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    },
    SubmitValidatorNodeExit {
        amount: MicroMinotari,
        validator_node_public_key: CommsPublicKey,
        validator_node_signature: CompressedSignature,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
        payment_id: MemoField,
        sidechain_deployment_key: Option<PrivateKey>,
    },
    PrepareOneSidedTransactionForSigning {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    },
    SignOneSidedTransaction {
        request: PrepareOneSidedTransactionForSigningResult,
    },

    SignOneSidedDepositMultisigTransaction {
        request: PrepareDepositMultisigTransactionResult,
    },
    SignOneSidedWithdrawMultisigTransaction {
        request: PrepareWithdrawMultisigTransactionResult,
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
        payment_id: MemoField,
    },
    SendManyOneSidedTransactions {
        destinations: Vec<(TariAddress, MicroMinotari, MemoField)>,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
    },
    SendRangeLimitedCoinJoinTransaction {
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee: FeeType,
        payment_id: MemoField,
    },
    SendOneSidedToStealthAddressTransaction {
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: Box<OutputFeatures>,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
        MemoField,
    ),
    CancelPendingTransaction(TxId),
    CancelCompletedTransaction(TxId),
    ImportUtxoWithStatus {
        amount: MicroMinotari,
        source_address: TariAddress,
        import_status: LegacyImportStatus,
        current_height: Option<u64>,
        mined_timestamp: Option<DateTime<Utc>>,
        scanned_output: TransactionOutput,
        payment_id: MemoField,
        optional_tx_id: Option<TxId>,
    },
    SubmitTransactionToSelf(TxId, Transaction, MicroMinotari, MicroMinotari, MemoField),
    RestartBroadcastProtocols,
    GetNumConfirmationsRequired,
    SetNumConfirmationsRequired(u64),
    ValidateTransactions,
    ReValidateRejectedTransactions,
    ReValidateTransactions,
    ReplaceByFee {
        tx_id: TxId,
        fee_increase: MicroMinotari,
    },
    UserPayForFee {
        tx_id: TxId,
        destination: TariAddress,
        fee: MicroMinotari,
    },
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
    PrepareDepositMultisigTransaction {
        request: CreateMultisigUtxo,
    },
    PrepareWithdrawMultisigTransaction {
        request: WithdrawMultisigUtxo,
    },
    CreateMultisigUtxo {
        request: CreateMultisigUtxo,
    },
    GetMultisigUtxoData {
        utxo_commitment: CompressedCommitment,
    },
    SendMultisigUtxo {
        utxo_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        signatures: Vec<CompressedCheckSigSchnorrSignature>,
    },
    ProcessReorg {
        height: u64,
    },
    GetBurnProof {
        output_hash: HashOutput,
    },
}

impl fmt::Display for TransactionServiceRequest {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProcessReorg { height } => write!(f, "ProcessReorg to height: {}", height),
            Self::GetPendingInboundTransactions => write!(f, "GetPendingInboundTransactions"),
            Self::GetPendingOutboundTransactions => write!(f, "GetPendingOutboundTransactions"),
            Self::GetCompletedTransactions { .. } => write!(f, "GetCompletedTransactions"),
            Self::GetCompletedTransactionsByAddresses { .. } => write!(f, "GetCompletedTransactionsByAddresses"),
            Self::GetCompletedTransactionsPaginated { .. } => write!(f, "GetCompletedTransactionsPaginated"),
            Self::ImportTransaction(tx) => write!(f, "ImportTransaction: {tx:?}"),
            Self::GetCancelledPendingInboundTransactions => write!(f, "GetCancelledPendingInboundTransactions"),
            Self::GetCancelledPendingOutboundTransactions => write!(f, "GetCancelledPendingOutboundTransactions"),
            Self::GetCancelledCompletedTransactions(_) => write!(f, "GetCancelledCompletedTransactions"),
            Self::SendManyOneSidedTransactions { .. } => write!(f, "SendManyOneSidedTransactions"),
            Self::GetCompletedTransaction(t) => write!(f, "GetCompletedTransaction({t})"),
            Self::SignOneSidedDepositMultisigTransaction { request } => {
                write!(f, "SignOneSidedDepositMultisigTransaction (request {request:?})")
            },
            Self::SignOneSidedWithdrawMultisigTransaction { request } => {
                write!(f, "SignOneSidedWithdrawMultisigTransaction (request {request:?})")
            },
            Self::ScrapeWallet {
                destination,
                fee_per_gram,
            } => {
                write!(
                    f,
                    "ScrapeWallet (destination: {destination}, fee_per_gram: {fee_per_gram})"
                )
            },
            Self::BurnTari { amount, payment_id, .. } => write!(f, "Burning Tari ({amount}, {payment_id})"),
            Self::SpendBackupPreMineUtxo {
                fee_per_gram,
                output_hash,
                expected_commitment,
                recipient_address,
                payment_id,
            } => f.write_str(&format!(
                "Spending backup pre-mine utxo with: fee_per_gram = {fee_per_gram}, output_hash = {output_hash}, \
                 commitment = {}, recipient = {recipient_address}, payment_id = {payment_id}",
                expected_commitment.to_hex()
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
                "Registering VN ({validator_node_public_key}, {payment_id}, {max_epoch})"
            ),
            Self::SubmitValidatorNodeExit {
                validator_node_public_key,
                payment_id,
                max_epoch,
                ..
            } => write!(
                f,
                "Submit VN Exit ({validator_node_public_key}, {payment_id}, {max_epoch})"
            ),
            Self::PrepareOneSidedTransactionForSigning {
                destination,
                amount,
                payment_id,
                ..
            } => write!(
                f,
                "PrepareOneSidedTransactionForSigning (to {destination}, {amount}, {payment_id})"
            ),
            Self::SignOneSidedTransaction { request } => write!(f, "SignOneSidedTransaction (request {request:?})"),
            Self::BroadcastSignedOneSidedTransaction { request } => {
                write!(f, "BroadcastSignedOneSidedTransaction (request {request:?})",)
            },
            Self::SendOneSidedTransaction {
                destination,
                amount,
                payment_id,
                ..
            } => write!(f, "SendOneSidedTransaction (to {destination}, {amount}, {payment_id})"),
            Self::SendRangeLimitedCoinJoinTransaction {
                selection_criteria,
                payment_id,
                ..
            } => write!(
                f,
                "SendRangeLimitedCoinJoinTransaction ({}, {})",
                selection_criteria
                    .range_limit
                    .clone()
                    .unwrap_or_default()
                    .target_minimum_amount,
                payment_id,
            ),
            Self::SendOneSidedToStealthAddressTransaction {
                destination,
                amount,
                payment_id,
                ..
            } => write!(
                f,
                "SendOneSidedToStealthAddressTransaction (to {destination}, {amount}, {payment_id})"
            ),
            Self::SendShaAtomicSwapTransaction(k, _, v, _, id) => {
                write!(f, "SendShaAtomicSwapTransaction (to {k}, {v}, {id})")
            },
            Self::CancelPendingTransaction(t) => write!(f, "CancelPendingTransaction ({t})"),
            Self::CancelCompletedTransaction(t) => write!(f, "CancelCompletedTransaction ({t})"),
            Self::ImportUtxoWithStatus {
                amount,
                source_address,
                import_status,
                current_height,
                mined_timestamp,
                payment_id,
                ..
            } => write!(
                f,
                "ImportUtxoWithStatus (amount: {amount}, from: {source_address}, payment_id: {payment_id}, import \
                 status: {import_status:?}, height: {current_height:?}, mined at: {mined_timestamp:?}"
            ),
            Self::SubmitTransactionToSelf(tx_id, _, _, _, _) => write!(f, "SubmitTransaction ({tx_id})"),
            Self::RestartBroadcastProtocols => write!(f, "RestartBroadcastProtocols"),
            Self::GetNumConfirmationsRequired => write!(f, "GetNumConfirmationsRequired"),
            Self::SetNumConfirmationsRequired(_) => write!(f, "SetNumConfirmationsRequired"),
            Self::GetAnyTransaction(t) => write!(f, "GetAnyTransaction({t})"),
            Self::ValidateTransactions => write!(f, "ValidateTransactions"),
            Self::ReValidateRejectedTransactions => write!(f, "ReValidateRejectedTransactions"),
            Self::ReValidateTransactions => write!(f, "ReValidateTransactions"),
            Self::ReplaceByFee { tx_id, fee_increase } => {
                write!(f, "ReplaceByFee(tx_id: {tx_id}, fee_increase: {fee_increase})")
            },
            Self::UserPayForFee {
                tx_id,
                destination,
                fee,
            } => {
                write!(
                    f,
                    "UserPayForFee(tx_id: {tx_id}, destination: {destination}, fee: {fee})"
                )
            },
            Self::GetFeePerGramStatsPerBlock { count } => {
                write!(f, "GetFeePerGramEstimatesPerBlock(count: {count})")
            },
            Self::RegisterCodeTemplate { template_name, .. } => {
                write!(f, "RegisterCodeTemplate: {template_name}")
            },
            Self::GetPaymentByReference { payref } => {
                write!(f, "GetPaymentByReference({payref})")
            },
            Self::GetTransactionByPaymentReference(payref) => {
                write!(f, "GetTransactionByPaymentReference({payref})")
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
            Self::CreateMultisigUtxo { request } => {
                write!(f, "CreateMultisigUtxo (request: {:?})", request)
            },

            Self::GetMultisigUtxoData { utxo_commitment } => {
                write!(f, "GetMultisigUtxoData (utxo_commitment: {:?})", utxo_commitment)
            },

            Self::SendMultisigUtxo {
                utxo_commitment,
                recipient_address,
                signatures,
            } => {
                write!(
                    f,
                    "SendMultisigUtxo (utxo_commitment: {:?}, recipient_address: {}, signatures: {:?})",
                    utxo_commitment, recipient_address, signatures
                )
            },
            Self::PrepareDepositMultisigTransaction { request } => {
                write!(f, "PrepareDepositMultisigTransaction (request: {:?})", request)
            },
            Self::PrepareWithdrawMultisigTransaction { request } => {
                write!(f, "PrepareWithdrawMultisigTransaction (request: {:?})", request)
            },
            Self::GetBurnProof { output_hash } => {
                write!(f, "GetBurnProof (output: {output_hash})")
            },
        }
    }
}

/// API Response enum
#[derive(Debug)]
pub enum TransactionServiceResponse {
    TransactionSent(TxId),
    TransactionsSent(Vec<TxId>),
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
        proof: Option<Box<PartialBurnClaimProof>>,
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
    ProtocolsRestarted,
    ReorgProcessed,
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
    SignedOneSidedDepositMultisigTransaction(Box<SignedOneSidedDepositMultisigTransactionResult>),
    SignedOneSidedWithdrawMultisigTransaction(Box<SignedOneSidedWithdrawMultisigTransactionResult>),
    TransactionReplaced(TxId),
    CodeRegistrationTransactionSent {
        tx_id: TxId,
        template_address: FixedHash,
    },
    ValidatorEvictionProofSent {
        tx_id: TxId,
    },

    PrepareDepositMultisigTransaction(Box<PrepareDepositMultisigTransactionResult>),
    PrepareWithdrawMultisigTransaction(Box<PrepareWithdrawMultisigTransactionResult>),
    CreateMultisigUtxo(TxId),
    GetMultisigUtxoData(Box<GetMultisigUtxoDataOutput>),
    SendMultisigUtxo(TxId),
    GetBurnProof {
        proof: Option<Box<DbBurnProof>>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    TransactionValidationStateChanged {
        faux: bool,
        id: OperationId,
    },
    TransactionValidationCompleted(OperationId),
    TransactionValidationFailed(OperationId, u64),
    TransactionBurnConfirmed {
        output_hash: HashOutput,
        commitment: Box<CompressedCommitment>,
    },
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
                write!(f, "TransactionCancelled for {tx}:{rejection:?}")
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
            TransactionEvent::TransactionValidationStateChanged { id: operation_id, .. } => {
                write!(f, "Transaction validation state changed: {operation_id}")
            },
            TransactionEvent::TransactionValidationCompleted(operation_id) => {
                write!(f, "Transaction validation(#{operation_id}) completed")
            },
            TransactionEvent::TransactionBurnConfirmed { output_hash, .. } => {
                write!(f, "Transaction Burn Confirmed for output hash {output_hash}")
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ScrapeWallet({e})"))??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ScrapeWallet".to_string(),
            )),
        }
    }

    pub async fn register_validator_node(
        &mut self,
        amount: MicroMinotari,
        validator_node_public_key: CompressedPublicKey,
        validator_node_signature: CompressedSignature,
        validator_node_claim_public_key: CompressedPublicKey,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::RegisterValidatorNode({e})"))??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::RegisterValidatorNode".to_string(),
            )),
        }
    }

    pub async fn submit_validator_node_exit(
        &mut self,
        amount: MicroMinotari,
        validator_node_public_key: CompressedPublicKey,
        validator_node_signature: CompressedSignature,
        sidechain_deployment_key: Option<PrivateKey>,
        max_epoch: VnEpoch,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SubmitValidatorNodeExit({e})"))??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SubmitValidatorNodeExit".to_string(),
            )),
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::RegisterCodeTemplate({e})"))??
        {
            TransactionServiceResponse::CodeRegistrationTransactionSent {
                tx_id,
                template_address,
            } => Ok((tx_id, template_address)),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::RegisterCodeTemplate".to_string(),
            )),
        }
    }

    pub async fn submit_validator_eviction_proof(
        &mut self,
        amount: MicroMinotari,
        proof: EvictionProof,
        fee_per_gram: MicroMinotari,
        sidechain_deployment_key: Option<PrivateKey>,
        payment_id: MemoField,
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
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SubmitValidatorEvictionProof({e})"),
            )?? {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SubmitValidatorEvictionProof".to_string(),
            )),
        }
    }

    pub async fn prepare_one_sided_transaction_for_signing(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::PrepareOneSidedTransactionForSigning({e})"),
            )?? {
            TransactionServiceResponse::OneSidedTransactionPreparedForSigning(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::PrepareOneSidedTransactionForSigning".to_string(),
            )),
        }
    }

    pub async fn sign_one_sided_transaction(
        &mut self,
        request: PrepareOneSidedTransactionForSigningResult,
    ) -> Result<SignedOneSidedTransactionResult, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SignOneSidedTransaction { request })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SignOneSidedTransaction({e})"))??
        {
            TransactionServiceResponse::SignedOneSidedTransaction(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SignOneSidedTransaction".to_string(),
            )),
        }
    }

    pub async fn sign_one_sided_deposit_multisig_transaction(
        &mut self,
        request: PrepareDepositMultisigTransactionResult,
    ) -> Result<SignedOneSidedDepositMultisigTransactionResult, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SignOneSidedDepositMultisigTransaction { request })
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SignOneSidedDepositMultisigTransaction({e})"),
            )?? {
            TransactionServiceResponse::SignedOneSidedDepositMultisigTransaction(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SignOneSidedDepositMultisigTransaction".to_string(),
            )),
        }
    }

    pub async fn sign_one_sided_withdraw_multisig_transaction(
        &mut self,
        request: PrepareWithdrawMultisigTransactionResult,
    ) -> Result<SignedOneSidedWithdrawMultisigTransactionResult, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SignOneSidedWithdrawMultisigTransaction { request })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SignOneSidedWithdrawMultisigTransaction({e})"))??
        {
            TransactionServiceResponse::SignedOneSidedWithdrawMultisigTransaction(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse("TransactionServiceRequest::SignOneSidedWithdrawMultisigTransaction".to_string())),
        }
    }

    pub async fn broadcast_signed_one_sided_transaction(
        &mut self,
        request: SignedOneSidedTransactionResult,
    ) -> Result<Vec<TxId>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::BroadcastSignedOneSidedTransaction { request })
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::BroadcastSignedOneSidedTransaction({e})"),
            )?? {
            TransactionServiceResponse::TransactionsSent(tx_ids) => Ok(tx_ids),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::BroadcastSignedOneSidedTransaction".to_string(),
            )),
        }
    }

    pub async fn send_one_sided_multi_recipient_transaction(
        &mut self,
        destinations: Vec<(TariAddress, MicroMinotari, MemoField)>,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
    ) -> Result<Vec<TxId>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendManyOneSidedTransactions {
                destinations,
                selection_criteria,
                output_features: Box::new(output_features),
                fee_per_gram,
            })
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SendManyOneSidedTransactions({e})"),
            )?? {
            TransactionServiceResponse::TransactionsSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SendManyOneSidedTransactions".to_string(),
            )),
        }
    }

    pub async fn send_one_sided_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SendOneSidedTransaction({e})"))??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SendOneSidedTransaction".to_string(),
            )),
        }
    }

    /// Burns the given amount of Tari from the wallet>
    /// If a claim_public_key is provided, a BurnClaimProof will be returned that can be used to claim the
    /// equivalent amount of tokens on a sidechain
    pub async fn burn_tari(
        &mut self,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
        claim_public_key: Option<CompressedPublicKey>,
        sidechain_deployment_key: Option<PrivateKey>,
    ) -> Result<(TxId, Option<PartialBurnClaimProof>), TransactionServiceError> {
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::BurnTari({e})"))??
        {
            TransactionServiceResponse::BurntTransactionSent { tx_id, proof } => Ok((tx_id, proof.map(|p| *p))),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::BurnTari".to_string(),
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::EncumberAggregateUtxo({e})"))??
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
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::EncumberAggregateUtxo".to_string(),
            )),
        }
    }

    pub async fn spend_backup_pre_mine_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        payment_id: MemoField,
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SpendBackupPreMineUtxo({e})"))??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SpendBackupPreMineUtxo".to_string(),
            )),
        }
    }

    pub async fn fetch_unspent_outputs(
        &mut self,
        output_hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::FetchUnspentOutputs { output_hashes })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::FetchUnspentOutputs({e})"))??
        {
            TransactionServiceResponse::UnspentOutputs(outputs) => Ok(outputs),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::FetchUnspentOutputs".to_string(),
            )),
        }
    }

    pub async fn finalize_aggregate_utxo(
        &mut self,
        tx_id: u64,
        total_meta_data_signature: CompressedSignature,
        total_script_data_signature: CompressedSignature,
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
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::FinalizeSentAggregateTransaction({e})"),
            )?? {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::FinalizeSentAggregateTransaction".to_string(),
            )),
        }
    }

    pub async fn send_range_limited_coin_join_transaction(
        &mut self,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee: FeeType,
        payment_id: MemoField,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendRangeLimitedCoinJoinTransaction {
                selection_criteria,
                output_features: Box::new(output_features),
                fee,
                payment_id,
            })
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest:SendRangeLimitedCoinJoinTransaction:({e})"),
            )?? {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SendRangeLimitedCoinJoinTransaction".to_string(),
            )),
        }
    }

    pub async fn send_one_sided_to_stealth_address_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest:SendOneSidedToStealthAddressTransaction:({e})"))??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse("TransactionServiceRequest::SendOneSidedToStealthAddressTransaction".to_string())),
        }
    }

    pub async fn cancel_pending_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::CancelPendingTransaction(tx_id))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::CancelTransaction({e})"))??
        {
            TransactionServiceResponse::TransactionCancelled => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::CancelTransaction".to_string(),
            )),
        }
    }

    pub async fn cancel_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::CancelCompletedTransaction(tx_id))
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::CancelCompletedTransaction({e})"),
            )?? {
            TransactionServiceResponse::TransactionCancelled => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::CancelCompletedTransaction".to_string(),
            )),
        }
    }

    pub async fn get_pending_inbound_transactions(
        &mut self,
    ) -> Result<Vec<InboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetPendingInboundTransactions)
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetPendingInboundTransactions({e})"),
            )?? {
            TransactionServiceResponse::PendingInboundTransactions(p) => Ok(p),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetPendingInboundTransactions".to_string(),
            )),
        }
    }

    pub async fn get_cancelled_pending_inbound_transactions(
        &mut self,
    ) -> Result<Vec<InboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCancelledPendingInboundTransactions)
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetCancelledPendingInboundTransactions({e})"),
            )?? {
            TransactionServiceResponse::PendingInboundTransactions(p) => Ok(p),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetCancelledPendingInboundTransactions".to_string(),
            )),
        }
    }

    pub async fn get_pending_outbound_transactions(
        &mut self,
    ) -> Result<Vec<OutboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetPendingOutboundTransactions)
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetPendingOutboundTransactions({e})"),
            )?? {
            TransactionServiceResponse::PendingOutboundTransactions(p) => Ok(p),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetPendingOutboundTransactions".to_string(),
            )),
        }
    }

    pub async fn get_cancelled_pending_outbound_transactions(
        &mut self,
    ) -> Result<Vec<OutboundTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCancelledPendingOutboundTransactions)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetCancelledPendingOutboundTransactions({e})"))??
        {
            TransactionServiceResponse::PendingOutboundTransactions(p) => Ok(p),
            _ => Err(TransactionServiceError::UnexpectedApiResponse("TransactionServiceRequest::GetCancelledPendingOutboundTransactions".to_string())),
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetCompletedTransactions({e})"))??
        {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetCompletedTransactions".to_string(),
            )),
        }
    }

    pub async fn get_completed_transactions_paginated(
        &mut self,
        offset: u64,
        limit: u64,
        status_filter: Option<u64>,
    ) -> Result<Vec<CompletedTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCompletedTransactionsPaginated {
                offset,
                limit,
                status_filter,
            })
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetCompletedTransactionsPaginated({e})"),
            )?? {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetCompletedTransactionsPaginated".to_string(),
            )),
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
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetCompletedTransactionsByAddresses({e})"),
            )?? {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetCompletedTransactionsByAddresses".to_string(),
            )),
        }
    }

    pub async fn get_cancelled_completed_transactions(
        &mut self,
        max_limit: u64,
    ) -> Result<Vec<CompletedTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCancelledCompletedTransactions(max_limit))
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetCancelledCompletedTransactions({e})"),
            )?? {
            TransactionServiceResponse::CompletedTransactions(c) => Ok(c),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetCancelledCompletedTransactions".to_string(),
            )),
        }
    }

    pub async fn get_completed_transaction(
        &mut self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetCompletedTransaction(tx_id))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetCompletedTransaction({e})"))??
        {
            TransactionServiceResponse::CompletedTransaction(t) => Ok(*t),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetCompletedTransaction".to_string(),
            )),
        }
    }

    pub async fn get_any_transaction(
        &mut self,
        tx_id: TxId,
    ) -> Result<Option<WalletTransaction>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetAnyTransaction(tx_id))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetAnyTransaction({e})"))??
        {
            TransactionServiceResponse::AnyTransaction(t) => Ok(*t),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetAnyTransaction".to_string(),
            )),
        }
    }

    pub async fn import_transaction(&mut self, tx: WalletTransaction) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ImportTransaction(tx))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ImportTransaction({e})"))??
        {
            TransactionServiceResponse::TransactionImported(t) => Ok(t),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ImportTransaction".to_string(),
            )),
        }
    }

    pub async fn import_utxo_with_status(
        &mut self,
        amount: MicroMinotari,
        source_address: TariAddress,
        import_status: LegacyImportStatus,
        current_height: Option<u64>,
        mined_timestamp: Option<DateTime<Utc>>,
        scanned_output: TransactionOutput,
        payment_id: MemoField,
        optional_tx_id: Option<TxId>,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ImportUtxoWithStatus {
                amount,
                source_address,
                import_status,
                current_height,
                mined_timestamp,
                scanned_output,
                payment_id,
                optional_tx_id,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ImportUtxoWithStatus({e})"))??
        {
            TransactionServiceResponse::UtxoImported(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ImportUtxoWithStatus".to_string(),
            )),
        }
    }

    pub async fn submit_transaction(
        &mut self,
        tx_id: TxId,
        tx: Transaction,
        amount: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<(), TransactionServiceError> {
        let fee = tx.body.get_total_fee()?;
        match self
            .handle
            .call(TransactionServiceRequest::SubmitTransactionToSelf(
                tx_id, tx, fee, amount, payment_id,
            ))
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SubmitTransactionToSelf({e})"))??
        {
            TransactionServiceResponse::TransactionSubmitted => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SubmitTransactionToSelf".to_string(),
            )),
        }
    }

    pub async fn revalidate_all_transactions(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ReValidateTransactions)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ReValidateTransactions({e})"))??
        {
            TransactionServiceResponse::ValidationStarted(_) => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ReValidateTransactions".to_string(),
            )),
        }
    }

    pub async fn revalidate_rejected_transactions(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ReValidateRejectedTransactions)
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ReValidateRejectedTransactions({e})"),
            )?? {
            TransactionServiceResponse::ValidationStarted(_) => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ReValidateRejectedTransactions".to_string(),
            )),
        }
    }

    pub async fn get_num_confirmations_required(&mut self) -> Result<u64, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetNumConfirmationsRequired)
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetNumConfirmationsRequired({e})"),
            )?? {
            TransactionServiceResponse::NumConfirmationsRequired(confirmations) => Ok(confirmations),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetNumConfirmationsRequired".to_string(),
            )),
        }
    }

    pub async fn set_num_confirmations_required(&mut self, number: u64) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SetNumConfirmationsRequired(number))
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SetNumConfirmationsRequired({e})"),
            )?? {
            TransactionServiceResponse::NumConfirmationsSet => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SetNumConfirmationsRequired".to_string(),
            )),
        }
    }

    pub async fn restart_broadcast_protocols(&mut self) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::RestartBroadcastProtocols)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::RestartBroadcastProtocols({e})"))??
        {
            TransactionServiceResponse::ProtocolsRestarted => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::RestartBroadcastProtocols".to_string(),
            )),
        }
    }

    pub async fn validate_transactions(&mut self) -> Result<OperationId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ValidateTransactions)
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ValidateTransactions({e})"))??
        {
            TransactionServiceResponse::ValidationStarted(id) => Ok(id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ValidateTransactions".to_string(),
            )),
        }
    }

    pub async fn prepare_deposit_multisig_transaction(
        &mut self,
        amount: MicroMinotari,
        party_number: u8,
        public_keys: Vec<CompressedPublicKey>,
        recipient_address: TariAddress,
    ) -> Result<PrepareDepositMultisigTransactionResult, TransactionServiceError> {
        let request = CreateMultisigUtxo {
            amount,
            party_number,
            public_keys,
            recipient_address,
        };
        match self
            .handle
            .call(TransactionServiceRequest::PrepareDepositMultisigTransaction { request })
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::PrepareDepositMultisigTransaction({e})"),
            )?? {
            TransactionServiceResponse::PrepareDepositMultisigTransaction(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::PrepareDepositMultisigTransaction".to_string(),
            )),
        }
    }

    pub async fn prepare_withdraw_multisig_transaction(
        &mut self,
        utxo_commitment: CompressedCommitment,
        signatures: Vec<CompressedCheckSigSchnorrSignature>,
        recipient_address: TariAddress,
    ) -> Result<PrepareWithdrawMultisigTransactionResult, TransactionServiceError> {
        let request = WithdrawMultisigUtxo {
            utxo_commitment,
            recipient_address,
            signatures,
        };
        match self
            .handle
            .call(TransactionServiceRequest::PrepareWithdrawMultisigTransaction { request })
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::PrepareWithdrawMultisigTransaction({e})"),
            )?? {
            TransactionServiceResponse::PrepareWithdrawMultisigTransaction(result) => Ok(*result),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::PrepareWithdrawMultisigTransaction".to_string(),
            )),
        }
    }

    pub async fn create_multisig_utxo(
        &mut self,
        amount: MicroMinotari,
        party_number: u8,
        public_keys: Vec<CompressedPublicKey>,
        recipient_address: TariAddress,
    ) -> Result<TxId, TransactionServiceError> {
        let request = CreateMultisigUtxo {
            amount,
            party_number,
            public_keys,
            recipient_address,
        };
        match self
            .handle
            .call(TransactionServiceRequest::CreateMultisigUtxo { request })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::CreateMultisigUtxo({e})"))??
        {
            TransactionServiceResponse::CreateMultisigUtxo(id) => Ok(id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::CreateMultisigUtxo".to_string(),
            )),
        }
    }

    pub async fn get_multisig_utxo_data(
        &mut self,
        utxo_commitment: CompressedCommitment,
    ) -> Result<GetMultisigUtxoDataOutput, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetMultisigUtxoData { utxo_commitment })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetMultisigUtxoData({e})"))??
        {
            TransactionServiceResponse::GetMultisigUtxoData(output) => Ok(*output),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetMultisigUtxoData".to_string(),
            )),
        }
    }

    pub async fn send_multisig_utxo(
        &mut self,
        utxo_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        signatures: Vec<CompressedCheckSigSchnorrSignature>,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::SendMultisigUtxo {
                utxo_commitment,
                recipient_address,
                signatures,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SendMultisigUtxo({e})"))??
        {
            TransactionServiceResponse::SendMultisigUtxo(output) => Ok(output),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SendMultisigUtxo".to_string(),
            )),
        }
    }

    pub async fn send_sha_atomic_swap_transaction(
        &mut self,
        destination: TariAddress,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
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
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::SendShaAtomicSwapTransaction({e})"),
            )?? {
            TransactionServiceResponse::ShaAtomicSwapTransactionSent(boxed) => {
                let (tx_id, pre_image, output) = *boxed;
                Ok((tx_id, pre_image, output))
            },
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::SendShaAtomicSwapTransaction".to_string(),
            )),
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
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetFeePerGramStatsPerBlock({e})"),
            )?? {
            TransactionServiceResponse::FeePerGramStatsPerBlock(resp) => Ok(resp),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetFeePerGramStatsPerBlock".to_string(),
            )),
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
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetPaymentByReference({e})"))??
        {
            TransactionServiceResponse::PaymentDetails(details) => Ok(details),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetPaymentByReference".to_string(),
            )),
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
            .await
            .inspect_err(
                |e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetTransactionByPaymentReference({e})"),
            )?? {
            TransactionServiceResponse::CompletedTransaction(tx) => Ok(*tx),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetTransactionByPaymentReference".to_string(),
            )),
        }
    }

    /// Replace a pending outbound transaction with a new one with higher fee
    ///
    /// # Arguments
    /// * `tx_id` - The transaction ID of the pending outbound transaction to replace
    /// * `fee_increase` - Fee increase
    ///
    /// # Returns
    /// The new transaction ID or an error
    pub async fn replace_by_fee(
        &mut self,
        tx_id: TxId,
        fee_increase: MicroMinotari,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ReplaceByFee { tx_id, fee_increase })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ReplaceByFee({e})"))??
        {
            TransactionServiceResponse::TransactionReplaced(new_tx_id) => Ok(new_tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ReplaceByFee".to_string(),
            )),
        }
    }

    /// Create a new transaction to pay for the fees of an existing transaction
    ///
    /// # Arguments
    /// * `tx_id` - The transaction ID of the transaction
    /// * `destination` - The destination address to receive remaining transaction outputs
    /// * `fee` - The fee amount to pay for this transaction
    ///
    /// # Returns
    /// The new transaction ID or an error
    pub async fn user_pay_for_fee(
        &mut self,
        tx_id: TxId,
        destination: TariAddress,
        fee: MicroMinotari,
    ) -> Result<TxId, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::UserPayForFee {
                tx_id,
                destination,
                fee,
            })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::UserPayForFee({e})"))??
        {
            TransactionServiceResponse::TransactionSent(tx_id) => Ok(tx_id),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::UserPayForFee".to_string(),
            )),
        }
    }

    pub async fn process_reorg(&mut self, height: u64) -> Result<(), TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::ProcessReorg { height })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::ProcessReorg({e})"))??
        {
            TransactionServiceResponse::ReorgProcessed => Ok(()),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::ProcessReorg".to_string(),
            )),
        }
    }

    pub async fn get_burn_proof(
        &mut self,
        output_hash: HashOutput,
    ) -> Result<Option<DbBurnProof>, TransactionServiceError> {
        match self
            .handle
            .call(TransactionServiceRequest::GetBurnProof { output_hash })
            .await
            .inspect_err(|e| warn!(target: LOG_TARGET, "TransactionServiceRequest::GetBurnProof({e})"))??
        {
            TransactionServiceResponse::GetBurnProof { proof } => Ok(proof.map(|p| *p)),
            _ => Err(TransactionServiceError::UnexpectedApiResponse(
                "TransactionServiceRequest::GetBurnProof".to_string(),
            )),
        }
    }
}
