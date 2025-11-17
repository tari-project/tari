//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    cmp::max,
    collections::VecDeque,
    convert::{TryFrom, TryInto},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use futures::{
    channel::mpsc::{self, Sender},
    future,
    SinkExt,
};
use log::*;
use minotari_app_grpc::tari_rpc::{
    self,
    payment_recipient::PaymentType,
    wallet_server,
    BroadcastSignedOneSidedTransactionRequest,
    BroadcastSignedOneSidedTransactionResponse,
    CheckConnectivityResponse,
    ClaimHtlcRefundRequest,
    ClaimHtlcRefundResponse,
    ClaimShaAtomicSwapRequest,
    ClaimShaAtomicSwapResponse,
    CoinBucket,
    CoinHistogramRequest,
    CoinHistogramResponse,
    CoinSplitRequest,
    CoinSplitResponse,
    CreateBurnTransactionRequest,
    CreateBurnTransactionResponse,
    CreateTemplateRegistrationRequest,
    CreateTemplateRegistrationResponse,
    FeePerGramStat,
    GetAddressResponse,
    GetAllCompletedTransactionsRequest,
    GetAllCompletedTransactionsResponse,
    GetBalanceRequest,
    GetBalanceResponse,
    GetBlockHeightTransactionsRequest,
    GetBlockHeightTransactionsResponse,
    GetBurnClaimProofRequest,
    GetBurnClaimProofResponse,
    GetCompleteAddressResponse,
    GetCompletedTransactionsRequest,
    GetCompletedTransactionsResponse,
    GetConnectivityRequest,
    GetFeeEstimateRequest,
    GetFeeEstimateResponse,
    GetFeePerGramStatsRequest,
    GetFeePerGramStatsResponse,
    GetIdentityRequest,
    GetIdentityResponse,
    GetPaymentByReferenceRequest,
    GetPaymentByReferenceResponse,
    GetPaymentIdAddressRequest,
    GetStateRequest,
    GetStateResponse,
    GetTransactionInfoRequest,
    GetTransactionInfoResponse,
    GetTransactionPayRefsRequest,
    GetTransactionPayRefsResponse,
    GetUnspentAmountsResponse,
    GetVersionRequest,
    GetVersionResponse,
    ImportTransactionsRequest,
    ImportTransactionsResponse,
    ImportUtxosRequest,
    ImportUtxosResponse,
    PrepareDepositMultisigTransactionRequest,
    PrepareDepositMultisigTransactionResponse,
    PrepareOneSidedTransactionForSigningRequest,
    PrepareOneSidedTransactionForSigningResponse,
    PrepareWithdrawMultisigTransactionRequest,
    PrepareWithdrawMultisigTransactionResponse,
    RangeLimitedCoinJoinRequest,
    RegisterValidatorNodeRequest,
    RegisterValidatorNodeResponse,
    ReplaceByFeeRequest,
    ReplaceByFeeResponse,
    RescanWalletRequest,
    RescanWalletResponse,
    RevalidateRequest,
    RevalidateResponse,
    SendShaAtomicSwapRequest,
    SendShaAtomicSwapResponse,
    SignMessageRequest,
    SignMessageResponse,
    SubmitValidatorEvictionProofRequest,
    SubmitValidatorEvictionProofResponse,
    SubmitValidatorNodeExitRequest,
    SubmitValidatorNodeExitResponse,
    TransactionDirection,
    TransactionEvent,
    TransactionEventRequest,
    TransactionEventResponse,
    TransactionInfo,
    TransactionStatus,
    TransferRequest,
    TransferResponse,
    TransferResult,
    UserPayForFeeRequest,
    UserPayForFeeResponse,
    ValidateRequest,
    ValidateResponse,
};
use minotari_wallet::{
    connectivity_service::{OnlineStatus, WalletConnectivityInterface, UNKNOWN_LATENCY_MS},
    error::WalletStorageError,
    legacy_transaction_protocol::recipient::RecipientState,
    output_manager_service::{
        error::OutputManagerError,
        handle::OutputManagerHandle,
        RangeLimit,
        UtxoSelectionCriteria,
    },
    transaction_service::{
        error::TransactionServiceError,
        handle::TransactionServiceHandle,
        storage::models::{self, WalletTransaction},
    },
    WalletKeyManager,
    WalletSqlite,
};
use rand::rngs::OsRng;
use tari_common_types::{
    payment_reference::generate_payment_reference,
    tari_address::TariAddress,
    transaction::{LegacyTransactionStatus, TxId},
    types::{
        BlockHash,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        PrivateKey,
        SignatureWithDomain,
    },
    wallet_types::FeeType,
};
use tari_comms::{connectivity::ConnectivityStatus, types::CommsPublicKey};
use tari_hashing::WalletMessageSigningDomain;
use tari_script::CompressedCheckSigSchnorrSignature;
use tari_transaction_components::{
    consensus::{ConsensusConstants, ConsensusManager},
    offline_signing::models::SignedOneSidedTransactionResult,
    transaction_components::{
        memo_field::{MemoField, TxType},
        OutputFeatures,
        UnblindedOutput,
    },
    MicroMinotari,
};
use tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray};
use tokio::{
    sync::{broadcast, Mutex},
    task,
    time::{sleep, timeout},
};
use tonic::{Request, Response, Status};

use crate::{
    grpc::{convert_to_transaction_event, wallet_debouncer::WalletDebouncer, TransactionWrapper},
    notifier::{CANCELLED, CONFIRMATION, MINED, QUEUED, RECEIVED, SENT},
};

const LOG_TARGET: &str = "wallet::ui::grpc";

async fn send_transaction_event(
    transaction_event: TransactionEvent,
    sender: &mut Sender<Result<TransactionEventResponse, Status>>,
) {
    let response = TransactionEventResponse {
        transaction: Some(transaction_event),
    };
    if let Err(err) = sender.send(Ok(response)).await {
        warn!(target: LOG_TARGET, "Error sending transaction via GRPC:  {err}");
        if let Err(send_err) = sender.send(Err(Status::unknown("Error sending data"))).await {
            warn!(target: LOG_TARGET, "Error sending error to GRPC client: {send_err}")
        }
    }
}

const AVG_LATENCIES_CAPACITY: usize = 10;

pub struct WalletGrpcServer {
    wallet: WalletSqlite,
    rules: ConsensusManager,
    debouncer: Arc<Mutex<WalletDebouncer<WalletKeyManager>>>,
    // Average latencies in ms with fixed/bounded queue
    avg_latencies_ms: Arc<Mutex<VecDeque<u64>>>,
}

impl WalletGrpcServer {
    #[allow(dead_code)]
    pub fn new(wallet: WalletSqlite) -> Self {
        let scanned_height = wallet
            .db
            .get_last_scanned_height()
            .unwrap_or_default()
            .unwrap_or_default();
        let debouncer = WalletDebouncer::new(
            wallet.output_manager_service.clone(),
            wallet.transaction_service.clone(),
            wallet.utxo_scanner_service.clone(),
            wallet.clone(),
            wallet.shutdown_signal.clone(),
            scanned_height,
        );
        let rules = ConsensusManager::builder(wallet.network.as_network()).build();
        Self {
            wallet,
            debouncer: Arc::new(Mutex::new(debouncer)),
            rules,
            avg_latencies_ms: Arc::new(Mutex::new(VecDeque::with_capacity(AVG_LATENCIES_CAPACITY))),
        }
    }

    fn get_consensus_constants(&self) -> Result<&ConsensusConstants, WalletStorageError> {
        let height = self.wallet.db.get_last_scanned_height()?.unwrap_or_default();
        Ok(self.rules.consensus_constants(height))
    }

    pub async fn start_balance_debouncer_event_monitor(&self) {
        self.debouncer.lock().await.start_event_monitor_if_needed().await
    }

    fn get_transaction_service(&self) -> TransactionServiceHandle {
        self.wallet.transaction_service.clone()
    }

    fn get_output_manager_service(&self) -> OutputManagerHandle<WalletKeyManager> {
        self.wallet.output_manager_service.clone()
    }

    async fn transfer_single_tx(
        &self,
        recipients: Vec<minotari_app_grpc::tari_rpc::PaymentRecipient>,
    ) -> Result<Response<minotari_app_grpc::tari_rpc::TransferResponse>, Status> {
        let fee_per_gram = recipients.first().expect("already checked").fee_per_gram;
        let recipients = recipients
            .into_iter()
            .enumerate()
            .map(|(idx, dest)| -> Result<_, String> {
                let address = TariAddress::from_str(&dest.address)
                    .map_err(|_| format!("Destination address at index {idx} is malformed"))?;
                let payment_id = if !dest.raw_payment_id.is_empty() {
                    MemoField::new_open(dest.raw_payment_id.to_vec(), TxType::PaymentToOther)?
                } else if let Some(user_pay_id) = dest.user_payment_id {
                    let bytes = match (
                        user_pay_id.u256.is_empty(),
                        user_pay_id.utf8_string.is_empty(),
                        user_pay_id.user_bytes.is_empty(),
                    ) {
                        (false, true, true) => user_pay_id.u256,
                        (true, false, true) => user_pay_id.utf8_string.as_bytes().to_vec(),
                        (true, true, false) => user_pay_id.user_bytes,
                        _ => {
                            return Err("user_payment_id must be one of u256, utf8_string or user_bytes".to_string());
                        },
                    };
                    MemoField::new_open(bytes, TxType::PaymentToOther)?
                } else {
                    MemoField::new_empty()
                };
                Ok((address, MicroMinotari(dest.amount), payment_id))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::invalid_argument)?;
        let mut transaction_service = self.get_transaction_service();
        let ids = transaction_service
            .send_one_sided_multi_recipient_transaction(
                recipients,
                UtxoSelectionCriteria::default(),
                OutputFeatures::default(),
                fee_per_gram.into(),
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to send transaction: {e}")))?;
        let mut results = Vec::new();
        for id in ids {
            let wallet_address = self
                .wallet
                .get_wallet_one_sided_address()
                .await
                .map_err(|e| Status::internal(format!("{e:?}")))?;
            let wallet_tx = timeout(Duration::from_millis(self.wallet.config.grpc_db_write_timeout), async {
                loop {
                    let tx = self
                        .get_transaction_service()
                        .get_any_transaction(id)
                        .await
                        .map_err(|e| Status::internal(format!("{e:?}")));

                    if let Ok(Some(tx)) = tx {
                        break tx;
                    }
                    sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .map_err(|_| {
                error!(target: LOG_TARGET, "Transaction {id} not found within timeout");
                Status::not_found(format!("Transaction {id} not found within timeout"))
            })?;
            let address = wallet_tx.destination_address().expect("cannot fail").to_string();
            let final_tx = convert_wallet_transaction_into_transaction_info(wallet_tx, &wallet_address);
            results.push(minotari_app_grpc::tari_rpc::TransferResult {
                address,
                transaction_id: id.into(),
                is_success: true,
                failure_message: Default::default(),
                transaction_info: Some(final_tx),
            });
        }
        Ok(Response::new(minotari_app_grpc::tari_rpc::TransferResponse { results }))
    }
}

#[tonic::async_trait]
impl wallet_server::Wallet for WalletGrpcServer {
    type GetAllCompletedTransactionsStreamStream = mpsc::Receiver<Result<GetCompletedTransactionsResponse, Status>>;
    type GetCompletedTransactionsStream = mpsc::Receiver<Result<GetCompletedTransactionsResponse, Status>>;
    type StreamTransactionEventsStream = mpsc::Receiver<Result<TransactionEventResponse, Status>>;

    async fn get_version(&self, _: Request<GetVersionRequest>) -> Result<Response<GetVersionResponse>, Status> {
        Ok(Response::new(GetVersionResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }

    async fn check_connectivity(
        &self,
        _: Request<GetConnectivityRequest>,
    ) -> Result<Response<CheckConnectivityResponse>, Status> {
        let debouncer = self.debouncer.lock().await;
        let connection_status = debouncer.get_connection_status().await;
        Ok(Response::new(CheckConnectivityResponse {
            status: i32::from(connection_status.as_u8()),
        }))
    }

    async fn check_for_updates(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SoftwareUpdate>, Status> {
        let mut resp = tari_rpc::SoftwareUpdate::default();

        if let Some(ref updater) = self.wallet.get_software_updater() {
            if let Some(ref update) = *updater.latest_update() {
                resp.has_update = true;
                resp.version = update.version().to_string();
                resp.sha = update.to_hash_hex();
                resp.download_url = update.download_url().to_string();
            }
        }

        Ok(Response::new(resp))
    }

    async fn identify(&self, _: Request<GetIdentityRequest>) -> Result<Response<GetIdentityResponse>, Status> {
        let identity = self.wallet.node_identity.clone();
        Ok(Response::new(GetIdentityResponse {
            public_key: identity.public_key().to_vec(),
            public_address: String::new(), // Note: Without comms this will always resolve to be empty
            node_id: identity.node_id().to_vec(),
        }))
    }

    async fn get_address(&self, _: Request<tari_rpc::Empty>) -> Result<Response<GetAddressResponse>, Status> {
        let interactive_address = self
            .wallet
            .get_wallet_interactive_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        let one_sided_address = self
            .wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        Ok(Response::new(GetAddressResponse {
            interactive_address: interactive_address.to_vec(),
            one_sided_address: one_sided_address.to_vec(),
        }))
    }

    async fn get_payment_id_address(
        &self,
        request: Request<GetPaymentIdAddressRequest>,
    ) -> Result<Response<GetCompleteAddressResponse>, Status> {
        let message = request.into_inner();
        trace!(
            target: LOG_TARGET,
            "get_payment_id_address: payment_id: '{:?}' / '{}'",
            message.payment_id, String::from_utf8_lossy(&message.payment_id),
        );

        let interactive_address = self
            .wallet
            .get_wallet_interactive_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        trace!(target: LOG_TARGET, "get_payment_id_address: interactive:      '{}'", interactive_address.to_base58());
        let interactive_address = interactive_address
            .with_memo_field_payment_id(message.payment_id.clone())
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        trace!(target: LOG_TARGET, "get_payment_id_address: interactive + id: '{}'", interactive_address.to_base58());
        let one_sided_address = self
            .wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        trace!(target: LOG_TARGET, "get_payment_id_address: one_sided:        '{}'", one_sided_address.to_base58());
        let one_sided_address = one_sided_address
            .with_memo_field_payment_id(message.payment_id)
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        trace!(target: LOG_TARGET, "get_payment_id_address: one_sided + id:   '{}'", one_sided_address.to_base58());
        Ok(Response::new(GetCompleteAddressResponse {
            interactive_address: interactive_address.to_vec(),
            one_sided_address: one_sided_address.to_vec(),
            interactive_address_base58: interactive_address.to_base58(),
            one_sided_address_base58: one_sided_address.to_base58(),
            interactive_address_emoji: interactive_address.to_emoji_string(),
            one_sided_address_emoji: one_sided_address.to_emoji_string(),
        }))
    }

    async fn get_complete_address(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<GetCompleteAddressResponse>, Status> {
        let interactive_address = self
            .wallet
            .get_wallet_interactive_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        let one_sided_address = self
            .wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;

        Ok(Response::new(GetCompleteAddressResponse {
            interactive_address: interactive_address.to_vec(),
            one_sided_address: one_sided_address.to_vec(),
            interactive_address_base58: interactive_address.to_base58(),
            one_sided_address_base58: one_sided_address.to_base58(),
            interactive_address_emoji: interactive_address.to_emoji_string(),
            one_sided_address_emoji: one_sided_address.to_emoji_string(),
        }))
    }

    async fn get_balance(&self, request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        let message = request.into_inner();
        let start = std::time::Instant::now();
        if let Some(user_payment_id) = message.payment_id {
            let bytes = match (
                user_payment_id.u256.is_empty(),
                user_payment_id.utf8_string.is_empty(),
                user_payment_id.user_bytes.is_empty(),
            ) {
                (false, true, true) => user_payment_id.u256,
                (true, false, true) => user_payment_id.utf8_string.as_bytes().to_vec(),
                (true, true, false) => user_payment_id.user_bytes,
                _ => {
                    return Err(Status::invalid_argument(
                        "user_payment_id must be one of u256, utf8_string or user_bytes".to_string(),
                    ));
                },
            };
            let mut oms = self.get_output_manager_service();
            let balance = oms
                .get_balance_for_payment_id(bytes)
                .await
                .map_err(|e| Status::not_found(format!("WalletDebouncer error! {e}")))?;
            return Ok(Response::new(GetBalanceResponse {
                available_balance: balance.available_balance.into(),
                pending_incoming_balance: balance.pending_incoming_balance.into(),
                pending_outgoing_balance: balance.pending_outgoing_balance.into(),
                timelocked_balance: balance.time_locked_balance.unwrap_or_default().into(),
            }));
        }
        let balance = {
            let mut get_balance = self.debouncer.lock().await;
            match get_balance.get_balance().await {
                Ok(b) => b,
                Err(e) => return Err(Status::not_found(format!("WalletDebouncer error! {e}"))),
            }
        };
        trace!(target: LOG_TARGET, "'get_balance' completed in {:.2?}", start.elapsed());
        Ok(Response::new(balance))
    }

    async fn get_state(&self, _request: Request<GetStateRequest>) -> Result<Response<GetStateResponse>, Status> {
        let start = std::time::Instant::now();
        let (balance, scanned_height, is_initial_validation_done) = {
            let mut debouncer = self.debouncer.lock().await;
            let balance = match debouncer.get_balance().await {
                Ok(b) => b,
                Err(e) => return Err(Status::not_found(format!("WalletDebouncer error! {e}"))),
            };
            let scanned_height = debouncer.get_scanned_height().await;
            let is_initial_validation_done = debouncer.is_initial_validation_done();
            (Some(balance), scanned_height, is_initial_validation_done)
        };

        let status = self.get_network_status(Request::new(tari_rpc::Empty {})).await?;
        let network = Some(status.into_inner());

        trace!(target: LOG_TARGET, "'get_state' completed in {:.2?}", start.elapsed());
        Ok(Response::new(GetStateResponse {
            scanned_height,
            balance,
            network,
            has_done_initial_validation: is_initial_validation_done,
        }))
    }

    async fn get_unspent_amounts(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<GetUnspentAmountsResponse>, Status> {
        let start = std::time::Instant::now();
        let mut output_service = self.get_output_manager_service();
        let unspent_amounts = match output_service.get_unspent_outputs().await {
            Ok(uo) => uo,
            Err(e) => return Err(Status::not_found(format!("GetUnspentAmounts error! {e}"))),
        };
        trace!(target: LOG_TARGET, "'get_unspent_amounts' completed in {:.2?}", start.elapsed());
        Ok(Response::new(GetUnspentAmountsResponse {
            amount: unspent_amounts
                .into_iter()
                .map(|o| o.wallet_output.value().as_u64())
                .filter(|&a| a > 0)
                .collect(),
        }))
    }

    async fn revalidate_all_transactions(
        &self,
        _request: Request<RevalidateRequest>,
    ) -> Result<Response<RevalidateResponse>, Status> {
        Ok(Response::new(RevalidateResponse {}))
    }

    async fn validate_all_transactions(
        &self,
        _request: Request<ValidateRequest>,
    ) -> Result<Response<ValidateResponse>, Status> {
        Ok(Response::new(ValidateResponse {}))
    }

    async fn send_sha_atomic_swap_transaction(
        &self,
        request: Request<SendShaAtomicSwapRequest>,
    ) -> Result<Response<SendShaAtomicSwapResponse>, Status> {
        let message = request
            .into_inner()
            .recipient
            .ok_or_else(|| Status::internal("Request is malformed".to_string()))?;
        let address = TariAddress::from_str(&message.address)
            .map_err(|_| Status::internal("Destination address is malformed".to_string()))?;
        let payment_id = if !message.raw_payment_id.is_empty() {
            MemoField::from_bytes(&message.raw_payment_id)
        } else if let Some(user_pay_id) = message.user_payment_id {
            let bytes = match (
                user_pay_id.u256.is_empty(),
                user_pay_id.utf8_string.is_empty(),
                user_pay_id.user_bytes.is_empty(),
            ) {
                (false, true, true) => user_pay_id.u256,
                (true, false, true) => user_pay_id.utf8_string.as_bytes().to_vec(),
                (true, true, false) => user_pay_id.user_bytes,
                _ => {
                    return Err(Status::invalid_argument(
                        "user_payment_id must be one of u256, utf8_string or user_bytes".to_string(),
                    ));
                },
            };
            MemoField::new_open(bytes, TxType::ClaimAtomicSwap).map_err(|e| Status::internal(e.to_string()))?
        } else {
            MemoField::new_empty()
        };
        let mut transaction_service = self.get_transaction_service();
        let response = match transaction_service
            .send_sha_atomic_swap_transaction(
                address.clone(),
                message.amount.into(),
                UtxoSelectionCriteria::default(),
                message.fee_per_gram.into(),
                payment_id,
            )
            .await
        {
            Ok((tx_id, pre_image, output)) => {
                debug!(
                    target: LOG_TARGET,
                    "Transaction broadcast: {}, preimage_hex: {}, hash {}",
                    tx_id,
                    pre_image.to_hex(),
                    output.hash().to_hex()
                );
                SendShaAtomicSwapResponse {
                    transaction_id: tx_id.as_u64(),
                    pre_image: pre_image.to_hex(),
                    output_hash: output.hash().to_hex(),
                    is_success: true,
                    failure_message: Default::default(),
                }
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to send Sha - XTR atomic swap for address `{address}`: {e}"
                );
                SendShaAtomicSwapResponse {
                    transaction_id: Default::default(),
                    pre_image: "".to_string(),
                    output_hash: "".to_string(),
                    is_success: false,
                    failure_message: e.to_string(),
                }
            },
        };

        Ok(Response::new(response))
    }

    async fn claim_sha_atomic_swap_transaction(
        &self,
        request: Request<ClaimShaAtomicSwapRequest>,
    ) -> Result<Response<ClaimShaAtomicSwapResponse>, Status> {
        let message = request.into_inner();
        let pre_image = CommsPublicKey::from_hex(&message.pre_image)
            .map_err(|_| Status::internal("pre_image is malformed".to_string()))?;
        let output = BlockHash::from_hex(&message.output)
            .map_err(|_| Status::internal("Output hash is malformed".to_string()))?;
        debug!(target: LOG_TARGET, "Trying to claim HTLC with hash {}", output.to_hex());
        let mut transaction_service = self.get_transaction_service();
        let mut output_manager_service = self.get_output_manager_service();
        let response = match output_manager_service
            .create_claim_sha_atomic_swap_transaction(output, pre_image, message.fee_per_gram.into())
            .await
        {
            Ok((tx_id, _fee, amount, tx)) => {
                match transaction_service
                    .submit_transaction(
                        tx_id,
                        tx,
                        amount,
                        MemoField::open_from_string(
                            "Claiming HTLC transaction with pre-image",
                            TxType::ClaimAtomicSwap,
                        ),
                    )
                    .await
                {
                    Ok(()) => {
                        let wallet_address = self
                            .wallet
                            .get_wallet_one_sided_address()
                            .await
                            .map_err(|e| Status::internal(format!("{e:?}")))?;
                        let wallet_tx = self
                            .get_transaction_service()
                            .get_any_transaction(tx_id)
                            .await
                            .map_err(|e| Status::internal(format!("{e:?}")))?
                            .ok_or_else(|| Status::not_found("Transaction not found".to_string()))?;
                        let final_tx = convert_wallet_transaction_into_transaction_info(wallet_tx, &wallet_address);
                        TransferResult {
                            address: Default::default(),
                            transaction_id: tx_id.as_u64(),
                            is_success: true,
                            failure_message: Default::default(),
                            transaction_info: Some(final_tx),
                        }
                    },
                    Err(e) => TransferResult {
                        address: Default::default(),
                        transaction_id: Default::default(),
                        is_success: false,
                        failure_message: e.to_string(),
                        transaction_info: None,
                    },
                }
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Failed to claim SHA - XTR atomic swap: {e}");
                TransferResult {
                    address: Default::default(),
                    transaction_id: Default::default(),
                    is_success: false,
                    failure_message: e.to_string(),
                    transaction_info: None,
                }
            },
        };

        Ok(Response::new(ClaimShaAtomicSwapResponse {
            results: Some(response),
        }))
    }

    async fn claim_htlc_refund_transaction(
        &self,
        request: Request<ClaimHtlcRefundRequest>,
    ) -> Result<Response<ClaimHtlcRefundResponse>, Status> {
        let message = request.into_inner();
        let output = BlockHash::from_hex(&message.output_hash)
            .map_err(|_| Status::internal("Output hash is malformed".to_string()))?;

        let mut transaction_service = self.get_transaction_service();
        let mut output_manager_service = self.get_output_manager_service();
        debug!(target: LOG_TARGET, "Trying to claim HTLC with hash {}", output.to_hex());
        let response = match output_manager_service
            .create_htlc_refund_transaction(output, message.fee_per_gram.into())
            .await
        {
            Ok((tx_id, _fee, amount, tx)) => {
                match transaction_service
                    .submit_transaction(
                        tx_id,
                        tx,
                        amount,
                        MemoField::open_from_string("Creating HTLC refund transaction", TxType::HtlcAtomicSwapRefund),
                    )
                    .await
                {
                    Ok(()) => {
                        let wallet_address = self
                            .wallet
                            .get_wallet_one_sided_address()
                            .await
                            .map_err(|e| Status::internal(format!("{e:?}")))?;
                        let wallet_tx = self
                            .get_transaction_service()
                            .get_any_transaction(tx_id)
                            .await
                            .map_err(|e| Status::internal(format!("{e:?}")))?
                            .ok_or_else(|| Status::not_found("Transaction not found".to_string()))?;
                        let final_tx = convert_wallet_transaction_into_transaction_info(wallet_tx, &wallet_address);
                        TransferResult {
                            address: Default::default(),
                            transaction_id: tx_id.as_u64(),
                            is_success: true,
                            failure_message: Default::default(),
                            transaction_info: Some(final_tx),
                        }
                    },
                    Err(e) => TransferResult {
                        address: Default::default(),
                        transaction_id: Default::default(),
                        is_success: false,
                        failure_message: e.to_string(),
                        transaction_info: None,
                    },
                }
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Failed to claim HTLC refund transaction: {e}");
                TransferResult {
                    address: Default::default(),
                    transaction_id: Default::default(),
                    is_success: false,
                    failure_message: e.to_string(),
                    transaction_info: None,
                }
            },
        };

        Ok(Response::new(ClaimHtlcRefundResponse {
            results: Some(response),
        }))
    }

    async fn prepare_one_sided_transaction_for_signing(
        &self,
        request: Request<PrepareOneSidedTransactionForSigningRequest>,
    ) -> Result<Response<PrepareOneSidedTransactionForSigningResponse>, Status> {
        let message = request.into_inner();

        let recipient = message.recipient.ok_or(Status::invalid_argument("Missing recipient"))?;
        let address = TariAddress::from_str(&recipient.address)
            .map_err(|_| Status::invalid_argument("Destination address is malformed"))?;

        let payment_id = if !recipient.raw_payment_id.is_empty() {
            MemoField::from_bytes(&recipient.raw_payment_id)
        } else if let Some(user_pay_id) = recipient.user_payment_id {
            let bytes = match (
                user_pay_id.u256.is_empty(),
                user_pay_id.utf8_string.is_empty(),
                user_pay_id.user_bytes.is_empty(),
            ) {
                (false, true, true) => user_pay_id.u256,
                (true, false, true) => user_pay_id.utf8_string.as_bytes().to_vec(),
                (true, true, false) => user_pay_id.user_bytes,
                _ => {
                    return Err(Status::invalid_argument(
                        "user_payment_id must be one of u256, utf8_string or user_bytes".to_string(),
                    ));
                },
            };
            MemoField::new_open(bytes, TxType::PaymentToOther).map_err(|e| Status::internal(e.to_string()))?
        } else {
            MemoField::new_empty()
        };

        let mut transaction_service = self.get_transaction_service();
        let response = match transaction_service
            .prepare_one_sided_transaction_for_signing(
                address.clone(),
                recipient.amount.into(),
                UtxoSelectionCriteria::default(),
                OutputFeatures::default(),
                recipient.fee_per_gram.into(),
                payment_id,
            )
            .await
        {
            Ok(data) => {
                let json_data = data.to_json().map_err(|e| Status::internal(e.to_string()))?;
                PrepareOneSidedTransactionForSigningResponse {
                    is_success: true,
                    result: json_data,
                    failure_message: Default::default(),
                }
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to lock transaction for address `{address}`: {err}"
                );
                PrepareOneSidedTransactionForSigningResponse {
                    is_success: false,
                    result: Default::default(),
                    failure_message: err.to_string(),
                }
            },
        };

        Ok(Response::new(response))
    }

    async fn broadcast_signed_one_sided_transaction(
        &self,
        request: Request<BroadcastSignedOneSidedTransactionRequest>,
    ) -> Result<Response<BroadcastSignedOneSidedTransactionResponse>, Status> {
        let message = request.into_inner();

        let mut transaction_service = self.get_transaction_service();
        let request = SignedOneSidedTransactionResult::from_json(&message.request)
            .map_err(|err| Status::internal(err.to_string()))?;
        let response = match transaction_service
            .broadcast_signed_one_sided_transaction(request)
            .await
        {
            Ok(result) => BroadcastSignedOneSidedTransactionResponse {
                is_success: true,
                transaction_id: result.as_u64(),
                failure_message: Default::default(),
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to broadcast a signed transaction: {err}"
                );
                BroadcastSignedOneSidedTransactionResponse {
                    is_success: false,
                    transaction_id: Default::default(),
                    failure_message: err.to_string(),
                }
            },
        };

        Ok(Response::new(response))
    }

    async fn prepare_deposit_multisig_transaction(
        &self,
        request: Request<PrepareDepositMultisigTransactionRequest>,
    ) -> Result<Response<PrepareDepositMultisigTransactionResponse>, Status> {
        debug!(target: LOG_TARGET, "prepare_deposit_multisig_transaction called");
        let message = request.into_inner();

        let recipient = TariAddress::from_bytes(message.recipient_address.as_slice())
            .map_err(|e| Status::invalid_argument(format!("Invalid recipient address: {e}")))?;

        let public_keys = message
            .public_keys
            .into_iter()
            .map(|pk_bytes| {
                CompressedPublicKey::from_canonical_bytes(&pk_bytes)
                    .map_err(|e| Status::invalid_argument(format!("Invalid public key: {e}")))
            })
            .collect::<Result<Vec<_>, Status>>()?;

        // Semantic validation
        if message.amount == 0 {
            return Err(Status::invalid_argument("amount must be greater than 0".to_string()));
        }
        if public_keys.is_empty() {
            return Err(Status::invalid_argument("public_keys cannot be empty".to_string()));
        }
        let party_number_u8 = u8::try_from(message.party_number)
            .map_err(|_| Status::invalid_argument("party_number_u8 must be in 1..=255".to_string()))?;
        if party_number_u8 == 0 {
            return Err(Status::invalid_argument(
                "party_number_u8 must be greater than 0".to_string(),
            ));
        }
        if (party_number_u8 as usize) > public_keys.len() {
            return Err(Status::invalid_argument(
                "party_number_u8 must be less than or equal to the number of public keys".to_string(),
            ));
        }
        // Ensure unique signers
        {
            let mut set = std::collections::HashSet::new();
            if !public_keys.iter().all(|pk| set.insert(pk.as_bytes().to_vec())) {
                return Err(Status::invalid_argument("public_keys must be unique".to_string()));
            }
        }

        let mut transaction_service = self.get_transaction_service();

        let response = match transaction_service
            .prepare_deposit_multisig_transaction(
                MicroMinotari::from(message.amount),
                party_number_u8,
                public_keys,
                recipient.clone(),
            )
            .await
        {
            Ok(data) => {
                let json_data = data.to_json().map_err(|e| Status::internal(e.to_string()))?;
                PrepareDepositMultisigTransactionResponse {
                    is_success: true,
                    result: json_data,
                    failure_message: Default::default(),
                }
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to lock transaction for address `{recipient}`: {err}"
                );
                PrepareDepositMultisigTransactionResponse {
                    is_success: false,
                    result: Default::default(),
                    failure_message: err.to_string(),
                }
            },
        };

        Ok(Response::new(response))
    }

    async fn prepare_withdraw_multisig_transaction(
        &self,
        request: Request<PrepareWithdrawMultisigTransactionRequest>,
    ) -> Result<Response<PrepareWithdrawMultisigTransactionResponse>, Status> {
        debug!(target: LOG_TARGET, "prepare_withdraw_multisig_transaction called");
        let message = request.into_inner();

        let recipient = TariAddress::from_bytes(message.recipient_address.as_slice())
            .map_err(|e| Status::invalid_argument(format!("Invalid recipient address: {e}")))?;

        let signatures = message
            .signatures
            .into_iter()
            .map(|signature_bytes| {
                CompressedCheckSigSchnorrSignature::from_binary(&signature_bytes)
                    .map_err(|e| Status::invalid_argument(format!("Invalid signature: {e}")))
            })
            .collect::<Result<Vec<_>, Status>>()?;

        if signatures.is_empty() {
            return Err(Status::invalid_argument("signatures cannot be empty".to_string()));
        }

        let commitment = CompressedCommitment::from_hex(&message.utxo_commitment)
            .map_err(|e| Status::invalid_argument(format!("Invalid UTXO commitment hash: {e}")))?;

        let mut transaction_service = self.get_transaction_service();

        let response = match transaction_service
            .prepare_withdraw_multisig_transaction(commitment, signatures, recipient.clone())
            .await
        {
            Ok(data) => {
                let json_data = data.to_json().map_err(|e| Status::internal(e.to_string()))?;
                PrepareWithdrawMultisigTransactionResponse {
                    is_success: true,
                    result: json_data,
                    failure_message: Default::default(),
                }
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to lock transaction for address `{recipient}`: {err}"
                );
                PrepareWithdrawMultisigTransactionResponse {
                    is_success: false,
                    result: Default::default(),
                    failure_message: err.to_string(),
                }
            },
        };

        Ok(Response::new(response))
    }

    #[allow(clippy::too_many_lines)]
    async fn transfer(&self, request: Request<TransferRequest>) -> Result<Response<TransferResponse>, Status> {
        let message = request.into_inner();

        if message.recipients.is_empty() {
            return Err(Status::invalid_argument(
                "At least one recipient is required".to_string(),
            ));
        }

        if message.single_tx {
            return self.transfer_single_tx(message.recipients).await;
        }
        let recipients = message
            .recipients
            .into_iter()
            .enumerate()
            .map(|(idx, dest)| -> Result<_, String> {
                let address = TariAddress::from_str(&dest.address)
                    .map_err(|_| format!("Destination address at index {idx} is malformed"))?;
                Ok((
                    dest.address,
                    address,
                    dest.amount,
                    dest.fee_per_gram,
                    dest.payment_type,
                    dest.user_payment_id,
                    dest.raw_payment_id,
                ))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::invalid_argument)?;
        let mut transfers = Vec::new();
        for (hex_address, address, amount, fee_per_gram, payment_type, user_payment_id, raw_payment_id) in recipients {
            if payment_type == PaymentType::StandardMimblewimble as i32 {
                return Err(Status::invalid_argument(
                    "Standard Mimblewimble transactions are not supported in this method".to_string(),
                ));
            }
            let payment_id = if !raw_payment_id.is_empty() {
                MemoField::new_open(raw_payment_id.to_vec(), TxType::PaymentToOther)
                    .map_err(|e| Status::internal(e.to_string()))?
            } else if let Some(user_pay_id) = user_payment_id {
                let bytes = match (
                    user_pay_id.u256.is_empty(),
                    user_pay_id.utf8_string.is_empty(),
                    user_pay_id.user_bytes.is_empty(),
                ) {
                    (false, true, true) => user_pay_id.u256,
                    (true, false, true) => user_pay_id.utf8_string.as_bytes().to_vec(),
                    (true, true, false) => user_pay_id.user_bytes,
                    _ => {
                        return Err(Status::invalid_argument(
                            "user_payment_id must be one of u256, utf8_string or user_bytes".to_string(),
                        ));
                    },
                };
                MemoField::new_open(bytes, TxType::PaymentToOther).map_err(|e| Status::internal(e.to_string()))?
            } else {
                MemoField::new_empty()
            };
            let mut transaction_service = self.get_transaction_service();
            transfers.push(async move {
                (
                    hex_address,
                    if payment_type == PaymentType::OneSided as i32 {
                        transaction_service
                            .send_one_sided_transaction(
                                address,
                                amount.into(),
                                UtxoSelectionCriteria::default(),
                                OutputFeatures::default(),
                                fee_per_gram.into(),
                                payment_id,
                            )
                            .await
                    } else {
                        transaction_service
                            .send_one_sided_to_stealth_address_transaction(
                                address,
                                amount.into(),
                                UtxoSelectionCriteria::default(),
                                OutputFeatures::default(),
                                fee_per_gram.into(),
                                payment_id,
                            )
                            .await
                    },
                )
            });
        }

        let transfers_results = future::join_all(transfers).await;
        let mut results = Vec::with_capacity(transfers_results.len());
        for (address, result) in transfers_results {
            match result {
                Ok(tx_id) => {
                    let wallet_address = self
                        .wallet
                        .get_wallet_one_sided_address()
                        .await
                        .map_err(|e| Status::internal(format!("{e:?}")))?;
                    let wallet_tx = timeout(Duration::from_millis(self.wallet.config.grpc_db_write_timeout), async {
                        loop {
                            let tx = self
                                .get_transaction_service()
                                .get_any_transaction(tx_id)
                                .await
                                .map_err(|e| Status::internal(format!("{e:?}")));

                            if let Ok(Some(tx)) = tx {
                                break tx;
                            }
                            sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .map_err(|_| {
                        error!(target: LOG_TARGET, "Transaction {tx_id} not found within timeout");
                        Status::not_found(format!("Transaction {tx_id} not found within timeout"))
                    })?;
                    let final_tx = convert_wallet_transaction_into_transaction_info(wallet_tx, &wallet_address);
                    results.push(TransferResult {
                        address,
                        transaction_id: tx_id.into(),
                        is_success: true,
                        failure_message: Default::default(),
                        transaction_info: Some(final_tx),
                    });
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to send transaction for address `{address}`: {err}"
                    );
                    results.push(TransferResult {
                        address,
                        transaction_id: Default::default(),
                        is_success: false,
                        failure_message: err.to_string(),
                        transaction_info: None,
                    });
                },
            }
        }

        Ok(Response::new(TransferResponse { results }))
    }

    #[allow(clippy::too_many_lines)]
    async fn range_limited_coin_join(
        &self,
        request: Request<RangeLimitedCoinJoinRequest>,
    ) -> Result<Response<TransferResponse>, Status> {
        let message = request.into_inner();
        debug!(target: LOG_TARGET, "range_limit_coin_join: {:?}", message);

        // Simple verification of range and target amount
        let range = message.lower_bound..message.upper_bound;
        let mut wallet = self.wallet.clone();
        let mut results = Vec::new();
        let buckets = wallet
            .output_manager_service
            .count_outputs_in_ranges(vec![range])
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let bucket = buckets.first().ok_or(Status::internal(format!(
            "The wallet does not have any funds in the specified range: {}..{}",
            message.lower_bound, message.upper_bound
        )))?;
        if bucket.total_value < message.target_amount {
            return Err(Status::internal(format!(
                "The wallet does not have sufficient funds in the specified range: {} uT < {} uT",
                bucket.total_value, message.target_amount
            )));
        }

        // Extract fee, payment id, and wallet address
        let fee = if let Some(val) = message.fee_per_gram {
            FeeType::FeePerGram(val.fee_per_gram.max(1))
        } else if let Some(val) = message.total_fee {
            FeeType::TotalFee(val.total_fee.max(1))
        } else {
            FeeType::FeePerGram(1)
        };
        let payment_id = if let Some(user_pay_id) = message.user_payment_id {
            let bytes = match (
                user_pay_id.u256.is_empty(),
                user_pay_id.utf8_string.is_empty(),
                user_pay_id.user_bytes.is_empty(),
            ) {
                (false, true, true) => user_pay_id.u256,
                (true, false, true) => user_pay_id.utf8_string.as_bytes().to_vec(),
                (true, true, false) => user_pay_id.user_bytes,
                _ => {
                    return Err(Status::invalid_argument(
                        "user_payment_id must be one of u256, utf8_string or user_bytes".to_string(),
                    ))
                },
            };
            MemoField::new_open(bytes, TxType::PaymentToSelf).map_err(|e| {
                error!(target: LOG_TARGET, "range_limit_coin_join: {}", e);
                Status::invalid_argument(format!("range_limit_coin_join: {}", e))
            })?
        } else {
            MemoField::new_empty()
        };
        let wallet_address = self
            .wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;

        // Start sending coin join transactions until we exhaust the range or reach the target amount
        // Note:
        //   This is done synchronously to ensure each transaction can be successfully processed and submitted to a base
        //   node before the next is created.
        let mut transaction_service = self.get_transaction_service();
        let batch_result = loop {
            let tx_result = transaction_service
                .send_range_limited_coin_join_transaction(
                    UtxoSelectionCriteria {
                        range_limit: Some(RangeLimit {
                            range: message.lower_bound..message.upper_bound,
                            transaction_input_limit: message.maximum_inputs_per_transaction,
                            target_minimum_amount: message.target_amount,
                        }),
                        ..Default::default()
                    },
                    OutputFeatures::default(),
                    fee,
                    payment_id.clone(),
                )
                .await;
            let tx_id = match tx_result {
                Ok(val) => val,
                Err(err) => {
                    if let TransactionServiceError::OutputManagerError(OutputManagerError::RangeLimitError {
                        range_exhausted,
                        ..
                    }) = err
                    {
                        if range_exhausted && !results.is_empty() {
                            break Ok(());
                        }
                    }
                    break Err(err);
                },
            };

            let wallet_tx = timeout(
                Duration::from_millis(self.wallet.config.grpc_broadcast_confirmation),
                async {
                    loop {
                        let tx = self.get_transaction_service().get_any_transaction(tx_id).await;

                        if let Ok(Some(tx)) = tx {
                            match tx.status() {
                                LegacyTransactionStatus::Broadcast |
                                LegacyTransactionStatus::MinedUnconfirmed |
                                LegacyTransactionStatus::MinedConfirmed |
                                LegacyTransactionStatus::OneSidedUnconfirmed |
                                LegacyTransactionStatus::OneSidedConfirmed => break Ok(tx),
                                LegacyTransactionStatus::Rejected => {
                                    let error = if let Some(reason) = tx.cancelled_reason() {
                                        TransactionServiceError::MempoolRejection {
                                            reason: format!("{}", reason),
                                        }
                                    } else {
                                        TransactionServiceError::MempoolRejection {
                                            reason: "Unknown reason".to_string(),
                                        }
                                    };
                                    break Err(error);
                                },
                                _ => {
                                    sleep(Duration::from_millis(10)).await;
                                    continue;
                                },
                            }
                        }
                    }
                },
            )
            .await;
            let wallet_tx = match wallet_tx {
                Ok(Ok(val)) => val,
                Ok(Err(e)) => break Err(e),
                Err(_) => {
                    break Err(TransactionServiceError::Other(format!(
                        "Transaction {tx_id} not found within timeout of {:.2?}",
                        self.wallet.config.grpc_db_write_timeout
                    )))
                },
            };

            let address = wallet_tx.destination_address().expect("cannot fail").to_string();
            let final_tx = convert_wallet_transaction_into_transaction_info(wallet_tx, &wallet_address);
            results.push(minotari_app_grpc::tari_rpc::TransferResult {
                address,
                transaction_id: tx_id.into(),
                is_success: true,
                failure_message: Default::default(),
                transaction_info: Some(final_tx),
            });
        };

        match batch_result {
            Ok(_) => Ok(Response::new(minotari_app_grpc::tari_rpc::TransferResponse { results })),
            Err(err) => {
                error!(target: LOG_TARGET, "range_limit_coin_join: {}", err);
                Err(Status::internal(format!("range_limit_coin_join: {}", err)))
            },
        }
    }

    async fn create_burn_transaction(
        &self,
        request: Request<CreateBurnTransactionRequest>,
    ) -> Result<Response<CreateBurnTransactionResponse>, Status> {
        let message = request.into_inner();

        let mut transaction_service = self.get_transaction_service();
        debug!(target: LOG_TARGET, "Trying to burn {} Minotari", message.amount);
        let result = transaction_service
            .burn_tari(
                message.amount.into(),
                UtxoSelectionCriteria::default(),
                message.fee_per_gram.into(),
                MemoField::from_bytes(&message.payment_id),
                Some(message.claim_public_key.as_slice())
                    .filter(|v| !v.is_empty())
                    .map(CompressedPublicKey::from_canonical_bytes)
                    .transpose()
                    .map_err(|e| Status::invalid_argument(e.to_string()))?,
                Some(message.sidechain_deployment_key.as_slice())
                    .filter(|v| !v.is_empty())
                    .map(PrivateKey::from_canonical_bytes)
                    .transpose()
                    .map_err(|e| Status::invalid_argument(e.to_string()))?,
            )
            .await;

        let response = match result {
            Ok((tx_id, Some(proof))) => {
                debug!(target: LOG_TARGET, "Burn transaction broadcast: {tx_id}",);
                CreateBurnTransactionResponse {
                    transaction_id: tx_id.as_u64(),
                    is_success: true,
                    failure_message: Default::default(),
                    commitment: proof.commitment.to_vec(),
                    ownership_proof: Some(proof.ownership_proof.into()),
                    reciprocal_claim_public_key: proof.reciprocal_claim_public_key.to_vec(),
                }
            },
            Ok((tx_id, None)) => {
                debug!(target: LOG_TARGET, "Burn transaction broadcast: {tx_id}",);
                CreateBurnTransactionResponse {
                    transaction_id: tx_id.as_u64(),
                    is_success: true,
                    failure_message: Default::default(),
                    ..Default::default()
                }
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Failed to burn Tari: {e}");
                CreateBurnTransactionResponse {
                    is_success: false,
                    failure_message: e.to_string(),
                    ..Default::default()
                }
            },
        };

        Ok(Response::new(response))
    }

    async fn get_transaction_info(
        &self,
        request: Request<GetTransactionInfoRequest>,
    ) -> Result<Response<GetTransactionInfoResponse>, Status> {
        let message = request.into_inner();

        let queries = message.transaction_ids.into_iter().map(|tx_id| {
            let tx_id = tx_id.into();
            let mut transaction_service = self.get_transaction_service();
            async move {
                transaction_service
                    .get_any_transaction(tx_id)
                    .await
                    .map(|tx| (tx_id, tx))
            }
        });

        let all_transactions = future::try_join_all(queries)
            .await
            .map(|tx| tx.into_iter())
            .map_err(|err| Status::unknown(err.to_string()))?;
        let wallet_address = self
            .wallet
            .get_wallet_interactive_address()
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;
        let mut transactions = Vec::new();
        for (tx_id, tx) in all_transactions {
            transactions.push(match tx {
                Some(tx) => convert_wallet_transaction_into_transaction_info(tx, &wallet_address),
                None => TransactionInfo::not_found(tx_id),
            });
        }

        Ok(Response::new(GetTransactionInfoResponse { transactions }))
    }

    async fn stream_transaction_events(
        &self,
        _request: tonic::Request<TransactionEventRequest>,
    ) -> Result<Response<Self::StreamTransactionEventsStream>, Status> {
        let (mut sender, receiver) = mpsc::channel(100);

        let mut shutdown_signal = self.wallet.shutdown_signal.clone();
        let mut transaction_service = self.wallet.transaction_service.clone();
        let mut transaction_service_events = self.wallet.transaction_service.get_event_stream();

        task::spawn(async move {
            loop {
                tokio::select! {
                    result = transaction_service_events.recv() => {
                        match result {
                            Ok(msg) => {
                                use minotari_wallet::transaction_service::handle::TransactionEvent::*;
                                match (*msg).clone() {
                                    ReceivedFinalizedTransaction(tx_id) => handle_completed_tx(tx_id, RECEIVED, &mut transaction_service, &mut sender).await,
                                    TransactionMinedUnconfirmed{tx_id, num_confirmations: _, is_valid: _} | DetectedTransactionUnconfirmed{tx_id, num_confirmations: _, is_valid: _}=> handle_completed_tx(tx_id, CONFIRMATION, &mut transaction_service, &mut sender).await,
                                    TransactionMined{tx_id, is_valid: _} | DetectedTransactionConfirmed{tx_id, is_valid: _} => handle_completed_tx(tx_id, MINED, &mut transaction_service, &mut sender).await,
                                    TransactionCancelled(tx_id, _) => {
                                        match transaction_service.get_any_transaction(tx_id).await{
                                            Ok(Some(wallet_tx)) => {
                                                use WalletTransaction::*;
                                                let transaction_event = match wallet_tx {
                                                    Completed(tx)  => convert_to_transaction_event(CANCELLED.to_string(), TransactionWrapper::Completed(Box::new(tx))),
                                                    PendingInbound(tx) => convert_to_transaction_event(CANCELLED.to_string(), TransactionWrapper::Inbound(Box::new(tx))),
                                                    PendingOutbound(tx) => convert_to_transaction_event(CANCELLED.to_string(), TransactionWrapper::Outbound(Box::new(tx))),
                                                };
                                                send_transaction_event(transaction_event, &mut sender).await;
                                            },
                                            Err(e) => error!(target: LOG_TARGET, "Transaction service error: {e}"),
                                            _ => error!(target: LOG_TARGET, "Transaction not found tx_id: {tx_id}"),
                                        }
                                    },
                                    TransactionCompletedImmediately(tx_id) => handle_pending_outbound(tx_id, SENT, &mut transaction_service, &mut sender).await,
                                    TransactionSendResult(tx_id, status) => {
                                        let is_sent = status.direct_send_result || status.store_and_forward_send_result;
                                        let event = if is_sent { SENT } else { QUEUED };
                                        handle_pending_outbound(tx_id, event, &mut transaction_service, &mut sender).await;
                                    },
                                    TransactionValidationStateChanged{..} => {
                                        send_transaction_event(simple_event("unknown"), &mut sender).await;
                                    },
                                    ReceivedTransaction(_) | ReceivedTransactionReply(_)  | TransactionBroadcast(_) => {
                                        send_transaction_event(simple_event("not_supported"), &mut sender).await;
                                    },
                                    // Only the above variants trigger state refresh
                                    _ => (),
                                }
                            },
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(target: LOG_TARGET, "Missed {n} from Transaction events");
                            }
                            Err(broadcast::error::RecvError::Closed) => {}
                        }
                    }
                    _ = shutdown_signal.wait() => {
                        info!(
                            target: LOG_TARGET,
                            "gRPC stream_transaction_events shutting down because the shutdown signal was received"
                        );
                        break;
                    },
                }
            }
        });
        Ok(Response::new(receiver))
    }

    #[allow(clippy::too_many_lines)]
    async fn get_completed_transactions(
        &self,
        request: Request<GetCompletedTransactionsRequest>,
    ) -> Result<Response<Self::GetCompletedTransactionsStream>, Status> {
        let start = std::time::Instant::now();
        trace!(
            target: LOG_TARGET,
            "GetCompletedTransactions: Incoming GRPC request"
        );
        let message = request.into_inner();
        let payment_id = if let Some(user_payment_id) = message.payment_id {
            let bytes = match (
                user_payment_id.u256.is_empty(),
                user_payment_id.utf8_string.is_empty(),
                user_payment_id.user_bytes.is_empty(),
            ) {
                (false, true, true) => user_payment_id.u256,
                (true, false, true) => user_payment_id.utf8_string.as_bytes().to_vec(),
                (true, true, false) => user_payment_id.user_bytes,
                _ => {
                    return Err(Status::invalid_argument(
                        "user_payment_id must be one of u256, utf8_string or user_bytes".to_string(),
                    ));
                },
            };
            Some(bytes)
        } else {
            None
        };
        let block_hash = if let Some(hash) = message.block_hash {
            Some(
                BlockHash::from_hex(&hash.hash)
                    .map_err(|_| Status::internal("Output hash is malformed".to_string()))?,
            )
        } else {
            None
        };
        let block_height = message.block_height.map(|height| height.block_height);

        let mut transaction_service = self.get_transaction_service();
        let transactions = transaction_service
            .get_completed_transactions(payment_id, block_hash, block_height, 0)
            .await
            .map_err(|err| Status::not_found(format!("No completed transactions found: {err:?}")))?;
        debug!(
            target: LOG_TARGET,
            "GetCompletedTransactions: Found {} completed transactions",
            transactions.len()
        );

        let (mut sender, receiver) = mpsc::channel(transactions.len());
        task::spawn(async move {
            for (i, txn) in transactions.iter().enumerate() {
                let output_commitments: Vec<Vec<u8>> = txn
                    .transaction
                    .body
                    .outputs()
                    .iter()
                    .map(|o| o.commitment().as_bytes().to_vec())
                    .collect();
                let input_commitments = txn
                    .transaction
                    .body
                    .inputs()
                    .iter()
                    .map(|i| match i.commitment() {
                        Ok(c) => c.as_bytes().to_vec(),
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to get input commitment: {e}");
                            vec![]
                        },
                    })
                    .collect();

                let response = GetCompletedTransactionsResponse {
                    transaction: Some(TransactionInfo {
                        tx_id: txn.tx_id.into(),
                        source_address: txn.source_address.to_vec(),
                        dest_address: txn.destination_address.to_vec(),
                        status: TransactionStatus::from(txn.status) as i32,
                        amount: txn.amount.into(),
                        is_cancelled: txn.cancelled.is_some(),
                        direction: TransactionDirection::from(txn.direction) as i32,
                        fee: txn.fee.into(),
                        timestamp: txn.timestamp.timestamp() as u64,
                        excess_sig: txn
                            .transaction
                            .first_kernel_excess_sig()
                            .unwrap_or(&CompressedSignature::default())
                            .get_signature()
                            .to_vec(),
                        raw_payment_id: txn.payment_id.to_bytes(),
                        user_payment_id: txn.payment_id.payment_id_as_bytes(),
                        mined_in_block_height: txn.mined_height.unwrap_or(0),
                        output_commitments,
                        input_commitments,
                        payment_references_sent: txn
                            .calculate_sent_payment_references()
                            .into_iter()
                            .map(|pr| pr.to_vec())
                            .collect(),
                        payment_references_received: txn
                            .calculate_received_payment_references()
                            .into_iter()
                            .map(|pr| pr.to_vec())
                            .collect(),
                        payment_references_change: txn
                            .calculate_change_payment_references()
                            .into_iter()
                            .map(|pr| pr.to_vec())
                            .collect(),
                    }),
                };
                match sender.send(Ok(response)).await {
                    Ok(_) => {
                        trace!(
                            target: LOG_TARGET,
                            "GetCompletedTransactions: Sent transaction TxId: {} ({} of {})",
                            txn.tx_id,
                            i + 1,
                            transactions.len()
                        );
                    },
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Error sending transaction via GRPC:  {err}");
                        match sender.send(Err(Status::unknown("Error sending data"))).await {
                            Ok(_) => (),
                            Err(send_err) => {
                                warn!(target: LOG_TARGET, "Error sending error to GRPC client: {send_err}")
                            },
                        }
                        return;
                    },
                }
            }
        });
        trace!(target: LOG_TARGET, "'get_completed_transactions' completed in {:.2?}", start.elapsed());

        Ok(Response::new(receiver))
    }

    // DEPRECATED: Use get_all_completed_transactions_stream for better performance and memory efficiency
    #[allow(clippy::too_many_lines)]
    async fn get_all_completed_transactions(
        &self,
        request: Request<GetAllCompletedTransactionsRequest>,
    ) -> Result<Response<GetAllCompletedTransactionsResponse>, Status> {
        let start = std::time::Instant::now();
        let req = request.into_inner();
        warn!(
            target: LOG_TARGET,
            "GetAllCompletedTransactions: DEPRECATED method called - consider migrating to GetAllCompletedTransactionsStream for better performance"
        );
        trace!(
            target: LOG_TARGET,
            "GetAllCompletedTransactions: Incoming GRPC request"
        );
        let mut transaction_service = self.get_transaction_service();

        let status_filter = if req.status_bitflag == 0 {
            None
        } else {
            Some(req.status_bitflag)
        };

        let total_requested = req.limit;
        let chunk_size = std::cmp::min(total_requested, 100); // Process in chunks of 100
        let mut all_transactions: Vec<TransactionInfo> =
            Vec::with_capacity(total_requested.try_into().unwrap_or(usize::MAX));
        let mut current_offset = req.offset;
        let mut remaining = total_requested;

        // Stream data in chunks to reduce memory usage
        while remaining > 0 {
            let current_limit = std::cmp::min(remaining, chunk_size);

            let chunk_transactions = transaction_service
                .get_completed_transactions_paginated(current_offset, current_limit, status_filter)
                .await
                .map_err(|err| {
                    Status::not_found(format!(
                        "GetAllCompletedTransactions: Error found for get_completed_transactions_paginated: {err:?}"
                    ))
                })?;

            // Break if we get no more results
            if chunk_transactions.is_empty() {
                break;
            }

            // Process this chunk
            for txn in chunk_transactions {
                let output_commitments: Vec<Vec<u8>> = txn
                    .transaction
                    .body
                    .outputs()
                    .iter()
                    .map(|o| o.commitment().as_bytes().to_vec())
                    .collect();
                let input_commitments: Vec<Vec<u8>> = txn
                    .transaction
                    .body
                    .inputs()
                    .iter()
                    .map(|i| match i.commitment() {
                        Ok(c) => c.as_bytes().to_vec(),
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to get input commitment: {e}");
                            vec![]
                        },
                    })
                    .collect();

                all_transactions.push(TransactionInfo {
                    tx_id: txn.tx_id.into(),
                    source_address: txn.source_address.to_vec(),
                    dest_address: txn.destination_address.to_vec(),
                    status: TransactionStatus::from(txn.status) as i32,
                    amount: txn.amount.into(),
                    is_cancelled: txn.cancelled.is_some(),
                    direction: TransactionDirection::from(txn.direction) as i32,
                    fee: txn.fee.into(),
                    timestamp: txn.timestamp.timestamp() as u64,
                    excess_sig: txn
                        .transaction
                        .first_kernel_excess_sig()
                        .unwrap_or(&CompressedSignature::default())
                        .get_signature()
                        .to_vec(),
                    raw_payment_id: txn.payment_id.to_bytes(),
                    user_payment_id: txn.payment_id.payment_id_as_bytes(),
                    mined_in_block_height: txn.mined_height.unwrap_or(0),
                    output_commitments,
                    input_commitments,
                    payment_references_sent: txn
                        .calculate_sent_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                    payment_references_received: txn
                        .calculate_received_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                    payment_references_change: txn
                        .calculate_change_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                });
            }

            // Update for next iteration
            current_offset += current_limit;
            remaining -= current_limit;
        }

        debug!(
            target: LOG_TARGET,
            "GetAllCompletedTransactions: Processed {} transactions in chunks",
            all_transactions.len()
        );

        trace!(target: LOG_TARGET, "'GetAllCompletedTransactions' completed in {:.2?}", start.elapsed());
        Ok(Response::new(GetAllCompletedTransactionsResponse {
            transactions: all_transactions,
        }))
    }

    #[allow(clippy::too_many_lines)]
    async fn get_all_completed_transactions_stream(
        &self,
        request: Request<GetAllCompletedTransactionsRequest>,
    ) -> Result<Response<Self::GetAllCompletedTransactionsStreamStream>, Status> {
        let start = std::time::Instant::now();
        let req = request.into_inner();

        trace!(
            target: LOG_TARGET,
            "GetAllCompletedTransactionsStreaming: Incoming GRPC request with offset={}, limit={}, status_bitflag={}",
            req.offset,
            req.limit,
            req.status_bitflag
        );

        let status_filter = if req.status_bitflag == 0 {
            None
        } else {
            Some(req.status_bitflag)
        };

        // Streaming parameters - smaller chunks for better responsiveness
        let total_requested = req.limit;
        let chunk_size = std::cmp::min(total_requested, 50);

        // Create GRPC streaming channel
        let buffer_size: usize = std::cmp::min(chunk_size, 10).try_into().unwrap_or(50);
        let (mut sender, receiver) = mpsc::channel(buffer_size);

        // Clone transaction service handle for async task
        let mut transaction_service = self.get_transaction_service();

        // Spawn async task to stream data
        task::spawn(async move {
            let mut current_offset = req.offset;
            let mut remaining = total_requested;
            let mut total_sent = 0u64;

            debug!(
                target: LOG_TARGET,
                "GetAllCompletedTransactionsStreaming: Starting to stream {total_requested} transactions in chunks of {chunk_size}"
            );

            while remaining > 0 {
                let current_limit = std::cmp::min(remaining, chunk_size);

                trace!(
                    target: LOG_TARGET,
                    "GetAllCompletedTransactionsStreaming: Fetching chunk at offset={current_offset}, limit={current_limit}"
                );

                // Fetch chunk from database
                let chunk_transactions = match transaction_service
                    .get_completed_transactions_paginated(current_offset, current_limit, status_filter)
                    .await
                {
                    Ok(transactions) => transactions,
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "GetAllCompletedTransactionsStreaming: Database error: {err:?}"
                        );
                        let _ = sender
                            .send(Err(Status::internal(format!(
                                "Database error while fetching transactions: {err:?}"
                            ))))
                            .await;
                        return;
                    },
                };

                // Break if no more results
                if chunk_transactions.is_empty() {
                    debug!(
                        target: LOG_TARGET,
                        "GetAllCompletedTransactionsStreaming: No more transactions found, ending stream"
                    );
                    break;
                }

                // Process and stream each transaction
                for txn in chunk_transactions {
                    let output_commitments: Vec<Vec<u8>> = txn
                        .transaction
                        .body
                        .outputs()
                        .iter()
                        .map(|o| o.commitment().as_bytes().to_vec())
                        .collect();

                    let input_commitments: Vec<Vec<u8>> = txn
                        .transaction
                        .body
                        .inputs()
                        .iter()
                        .map(|i| match i.commitment() {
                            Ok(c) => c.as_bytes().to_vec(),
                            Err(e) => {
                                warn!(target: LOG_TARGET, "Failed to get input commitment: {e}");
                                vec![]
                            },
                        })
                        .collect();

                    let transaction_info = TransactionInfo {
                        tx_id: txn.tx_id.into(),
                        source_address: txn.source_address.to_vec(),
                        dest_address: txn.destination_address.to_vec(),
                        status: TransactionStatus::from(txn.status) as i32,
                        amount: txn.amount.into(),
                        is_cancelled: txn.cancelled.is_some(),
                        direction: TransactionDirection::from(txn.direction) as i32,
                        fee: txn.fee.into(),
                        timestamp: txn.timestamp.timestamp() as u64,
                        excess_sig: txn
                            .transaction
                            .first_kernel_excess_sig()
                            .unwrap_or(&CompressedSignature::default())
                            .get_signature()
                            .to_vec(),
                        raw_payment_id: txn.payment_id.to_bytes(),
                        user_payment_id: txn.payment_id.payment_id_as_bytes(),
                        mined_in_block_height: txn.mined_height.unwrap_or(0),
                        output_commitments,
                        input_commitments,
                        payment_references_sent: txn
                            .calculate_sent_payment_references()
                            .into_iter()
                            .map(|pr| pr.to_vec())
                            .collect(),
                        payment_references_received: txn
                            .calculate_received_payment_references()
                            .into_iter()
                            .map(|pr| pr.to_vec())
                            .collect(),
                        payment_references_change: txn
                            .calculate_change_payment_references()
                            .into_iter()
                            .map(|pr| pr.to_vec())
                            .collect(),
                    };

                    let response = GetCompletedTransactionsResponse {
                        transaction: Some(transaction_info),
                    };

                    // Stream the transaction
                    match sender.send(Ok(response)).await {
                        Ok(_) => {
                            total_sent += 1;
                            trace!(
                                target: LOG_TARGET,
                                "GetAllCompletedTransactionsStreaming: Sent transaction TxId: {} ({} of {})",
                                txn.tx_id,
                                total_sent,
                                total_requested
                            );
                        },
                        Err(_) => {
                            warn!(
                                target: LOG_TARGET,
                                "GetAllCompletedTransactionsStreaming: Stream closed by client"
                            );
                            return;
                        },
                    }
                }

                // Update for next iteration
                current_offset += current_limit;
                remaining = remaining.saturating_sub(current_limit);

                trace!(
                    target: LOG_TARGET,
                    "GetAllCompletedTransactionsStreaming: Completed chunk, remaining={remaining}"
                );
            }

            debug!(
                target: LOG_TARGET,
                "GetAllCompletedTransactionsStreaming: Completed. Sent {} transactions in {:.2?}",
                total_sent,
                start.elapsed()
            );
        });

        trace!(
            target: LOG_TARGET,
            "GetAllCompletedTransactionsStreaming: Setup completed in {:.2?}",
            start.elapsed()
        );

        Ok(Response::new(receiver))
    }

    async fn get_block_height_transactions(
        &self,
        request: Request<GetBlockHeightTransactionsRequest>,
    ) -> Result<Response<GetBlockHeightTransactionsResponse>, Status> {
        let start = std::time::Instant::now();
        trace!(
            target: LOG_TARGET,
            "GetBlockHeightTransactions: Incoming GRPC request"
        );
        let message = request.into_inner();
        let block_height = message.block_height;

        let mut transaction_service = self.get_transaction_service();
        let transactions = transaction_service
            .get_completed_transactions(None, None, Some(block_height), 0)
            .await
            .map_err(|err| {
                Status::not_found(format!(
                    "GetBlockHeightTransactions: Error found at block height {block_height}: {err:?}"
                ))
            })?;
        debug!(
            target: LOG_TARGET,
            "GetBlockHeightTransactions: Found {} transactions at block height {}",
            transactions.len(),
            block_height
        );

        let mut result_transactions = Vec::new();
        for txn in &transactions {
            let output_commitments: Vec<Vec<u8>> = txn
                .transaction
                .body
                .outputs()
                .iter()
                .map(|o| o.commitment().as_bytes().to_vec())
                .collect();
            let input_commitments: Vec<Vec<u8>> = txn
                .transaction
                .body
                .inputs()
                .iter()
                .map(|i| match i.commitment() {
                    Ok(c) => c.as_bytes().to_vec(),
                    Err(e) => {
                        warn!(target: LOG_TARGET, "Failed to get input commitment: {e}");
                        vec![]
                    },
                })
                .collect();

            result_transactions.push(TransactionInfo {
                tx_id: txn.tx_id.into(),
                source_address: txn.source_address.to_vec(),
                dest_address: txn.destination_address.to_vec(),
                status: TransactionStatus::from(txn.status) as i32,
                amount: txn.amount.into(),
                is_cancelled: txn.cancelled.is_some(),
                direction: TransactionDirection::from(txn.direction) as i32,
                fee: txn.fee.into(),
                timestamp: txn.timestamp.timestamp() as u64,
                excess_sig: txn
                    .transaction
                    .first_kernel_excess_sig()
                    .unwrap_or(&CompressedSignature::default())
                    .get_signature()
                    .to_vec(),
                raw_payment_id: txn.payment_id.to_bytes(),
                user_payment_id: txn.payment_id.payment_id_as_bytes(),
                mined_in_block_height: txn.mined_height.unwrap_or(0),
                output_commitments,
                input_commitments,
                payment_references_sent: txn
                    .calculate_sent_payment_references()
                    .into_iter()
                    .map(|pr| pr.to_vec())
                    .collect(),
                payment_references_received: txn
                    .calculate_received_payment_references()
                    .into_iter()
                    .map(|pr| pr.to_vec())
                    .collect(),
                payment_references_change: txn
                    .calculate_change_payment_references()
                    .into_iter()
                    .map(|pr| pr.to_vec())
                    .collect(),
            });
        }

        trace!(target: LOG_TARGET, "'get_block_height_transactions' completed in {:.2?}", start.elapsed());

        Ok(Response::new(GetBlockHeightTransactionsResponse {
            transactions: result_transactions,
        }))
    }

    async fn coin_split(&self, request: Request<CoinSplitRequest>) -> Result<Response<CoinSplitResponse>, Status> {
        let message = request.into_inner();

        let mut wallet = self.wallet.clone();

        let tx_id = wallet
            .coin_split(
                vec![],
                MicroMinotari::from(message.amount_per_split),
                usize::try_from(message.split_count)
                    .map_err(|_| Status::internal("Count not convert u64 to usize".to_string()))?,
                MicroMinotari::from(message.fee_per_gram),
                MemoField::open_from_string("Creating coin-split transaction", TxType::CoinSplit),
            )
            .await
            .map_err(|e| Status::internal(format!("{e:?}")))?;

        Ok(Response::new(CoinSplitResponse { tx_id: tx_id.into() }))
    }

    async fn coin_histogram(
        &self,
        _request: Request<CoinHistogramRequest>,
    ) -> Result<Response<CoinHistogramResponse>, Status> {
        let mut wallet = self.wallet.clone();

        // These ranges are hard-coded for now - easy enough to change later if needed
        let bucket_ranges = vec![
            0..1_000u64,                             // 0 - < 1,000 uT
            1_000..100_000,                          // 1,000 uT - < 100,000 uT
            100_000..1_000_000,                      // 100,000 uT - < 1 T
            1_000_000..1_000_000_000,                // 1 T - < 1,000 T
            1_000_000_000..100_000_000_000,          // 1,000 T - < 100,000 T
            100_000_000_000..21_000_000_000_000_000, // 100,000 T - < 21,000,000 T (max supply)
        ];

        let buckets = wallet
            .output_manager_service
            .count_outputs_in_ranges(bucket_ranges.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut buckets_response = Vec::with_capacity(buckets.len());
        for bucket in &buckets {
            buckets_response.push(CoinBucket {
                count: bucket.number_of_outputs,
                total_amount: bucket.total_value,
                lower_bound: bucket.range.start,
                upper_bound: bucket.range.end,
            });
        }

        Ok(Response::new(CoinHistogramResponse {
            buckets: buckets_response,
        }))
    }

    async fn import_utxos(
        &self,
        request: Request<ImportUtxosRequest>,
    ) -> Result<Response<ImportUtxosResponse>, Status> {
        let message = request.into_inner();

        let mut wallet = self.wallet.clone();

        let unblinded_outputs: Vec<UnblindedOutput> = message
            .outputs
            .into_iter()
            .map(UnblindedOutput::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::invalid_argument)?;
        let mut tx_ids = Vec::new();

        for o in &unblinded_outputs {
            tx_ids.push(
                wallet
                    .import_unblinded_output_as_non_rewindable(
                        o.clone(),
                        TariAddress::default(),
                        MemoField::from_bytes(&message.payment_id),
                    )
                    .await
                    .map_err(|e| Status::internal(format!("{e:?}")))?
                    .into(),
            );
        }

        Ok(Response::new(ImportUtxosResponse { tx_ids }))
    }

    async fn get_network_status(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::NetworkStatusResponse>, Status> {
        // This mapping is to comply to the legacy interface
        let (status, avg_latency, num_node_connections) =
            match self.wallet.wallet_connectivity.get_connectivity_status().await {
                OnlineStatus::Connecting => (ConnectivityStatus::Initializing, UNKNOWN_LATENCY_MS, 0),
                OnlineStatus::Online { latency_ms, .. } => {
                    let mut avg_latencies = self.avg_latencies_ms.lock().await;
                    let latency_ms = update_and_average_latency(&mut avg_latencies, latency_ms);
                    (ConnectivityStatus::Online(1), latency_ms, 1)
                },
                OnlineStatus::Offline => (ConnectivityStatus::Offline, u64::MAX, 0),
                OnlineStatus::Degraded { latency_ms, .. } => {
                    let mut avg_latencies = self.avg_latencies_ms.lock().await;
                    let latency_ms = update_and_average_latency(&mut avg_latencies, latency_ms);
                    (ConnectivityStatus::Degraded(1), latency_ms, 1)
                },
            };

        let resp = tari_rpc::NetworkStatusResponse {
            status: tari_rpc::ConnectivityStatus::from(status) as i32,
            avg_latency_ms: u32::try_from(avg_latency).unwrap_or(u32::MAX),
            num_node_connections,
        };

        Ok(Response::new(resp))
    }

    async fn get_connected_http_peer(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::GetConnectedHttpPeerResponse>, Status> {
        let url = self.wallet.wallet_connectivity.get_address().await;
        let (is_online, last_latency) = match self.wallet.wallet_connectivity.get_connectivity_status().await {
            OnlineStatus::Connecting => (false, UNKNOWN_LATENCY_MS),
            OnlineStatus::Offline => (false, u64::MAX),
            OnlineStatus::Online { latency_ms, .. } | OnlineStatus::Degraded { latency_ms, .. } => (true, latency_ms),
        };

        let peer = tari_rpc::HttpPeer {
            url,
            last_latency,
            is_online,
        };
        let resp = tari_rpc::GetConnectedHttpPeerResponse {
            connected_peer: Some(peer),
        };

        Ok(Response::new(resp))
    }

    async fn cancel_transaction(
        &self,
        request: Request<tari_rpc::CancelTransactionRequest>,
    ) -> Result<Response<tari_rpc::CancelTransactionResponse>, Status> {
        let message = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming gRPC request to Cancel Transaction (TxId: {})", message.tx_id,
        );
        let mut transaction_service = self.get_transaction_service();

        match transaction_service.cancel_transaction(message.tx_id.into()).await {
            Ok(_) => {
                return Ok(Response::new(tari_rpc::CancelTransactionResponse {
                    is_success: true,
                    failure_message: "".to_string(),
                }))
            },
            Err(e) => {
                return Ok(Response::new(tari_rpc::CancelTransactionResponse {
                    is_success: false,
                    failure_message: e.to_string(),
                }))
            },
        }
    }

    async fn create_template_registration(
        &self,
        request: Request<CreateTemplateRegistrationRequest>,
    ) -> Result<Response<CreateTemplateRegistrationResponse>, Status> {
        let mut transaction_service = self.wallet.transaction_service.clone();
        let message = request.into_inner();

        let fee_per_gram = message.fee_per_gram.into();

        let (tx_id, template_address) = transaction_service
            .register_code_template(
                message
                    .template_name
                    .try_into()
                    .map_err(|_| Status::invalid_argument("template name is too long"))?,
                message
                    .template_version
                    .try_into()
                    .map_err(|_| Status::invalid_argument("template version is too large for a u16"))?,
                if let Some(tt) = message.template_type {
                    tt.try_into()
                        .map_err(|_| Status::invalid_argument("template type is invalid"))?
                } else {
                    return Err(Status::invalid_argument("template type is missing"));
                },
                if let Some(bi) = message.build_info {
                    bi.try_into()
                        .map_err(|_| Status::invalid_argument("build info is invalid"))?
                } else {
                    return Err(Status::invalid_argument("build info is missing"));
                },
                message
                    .binary_sha
                    .try_into()
                    .map_err(|_| Status::invalid_argument("binary sha is malformed"))?,
                message
                    .binary_url
                    .try_into()
                    .map_err(|_| Status::invalid_argument("binary URL is too long"))?,
                fee_per_gram,
                if message.sidechain_deployment_key.is_empty() {
                    None
                } else {
                    Some(
                        PrivateKey::from_canonical_bytes(&message.sidechain_deployment_key)
                            .map_err(|_| Status::invalid_argument("sidechain_deployment_key is malformed"))?,
                    )
                },
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateTemplateRegistrationResponse {
            tx_id: tx_id.as_u64(),
            template_address: template_address.to_vec(),
        }))
    }

    async fn register_validator_node(
        &self,
        request: Request<RegisterValidatorNodeRequest>,
    ) -> Result<Response<RegisterValidatorNodeResponse>, Status> {
        let request = request.into_inner();
        let mut transaction_service = self.get_transaction_service();
        let validator_node_public_key = CommsPublicKey::from_canonical_bytes(&request.validator_node_public_key)
            .map_err(|_| Status::internal("Destination address is malformed".to_string()))?;
        let validator_node_signature = request
            .validator_node_signature
            .ok_or_else(|| Status::invalid_argument("Validator node signature is missing!"))?
            .try_into()
            .map_err(|_| Status::invalid_argument("Validator node signature is malformed!"))?;
        let validator_node_claim_public_key =
            CompressedPublicKey::from_canonical_bytes(&request.validator_node_claim_public_key)
                .map_err(|_| Status::invalid_argument("Claim public key is malformed"))?;

        let sidechain_key = if request.sidechain_deployment_key.is_empty() {
            None
        } else {
            Some(
                PrivateKey::from_canonical_bytes(&request.sidechain_deployment_key)
                    .map_err(|_| Status::invalid_argument("sidechain_id is malformed"))?,
            )
        };

        let constants = self.get_consensus_constants().map_err(|e| {
            error!(target: LOG_TARGET, "Failed to get consensus constants: {e}");
            Status::internal("failed to fetch consensus constants")
        })?;

        let response = match transaction_service
            .register_validator_node(
                constants.validator_node_registration_min_deposit_amount(),
                validator_node_public_key,
                validator_node_signature,
                validator_node_claim_public_key,
                sidechain_key,
                request.max_epoch.into(),
                UtxoSelectionCriteria::default(),
                request.fee_per_gram.into(),
                MemoField::from_bytes(&request.payment_id),
            )
            .await
        {
            Ok(tx) => RegisterValidatorNodeResponse {
                transaction_id: tx.as_u64(),
                is_success: true,
                failure_message: Default::default(),
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Transaction service error: {e}");
                RegisterValidatorNodeResponse {
                    transaction_id: Default::default(),
                    is_success: false,
                    failure_message: e.to_string(),
                }
            },
        };
        Ok(Response::new(response))
    }

    async fn submit_validator_node_exit(
        &self,
        request: Request<SubmitValidatorNodeExitRequest>,
    ) -> Result<Response<SubmitValidatorNodeExitResponse>, Status> {
        let request = request.into_inner();
        let mut transaction_service = self.get_transaction_service();
        let validator_node_public_key = CommsPublicKey::from_canonical_bytes(&request.validator_node_public_key)
            .map_err(|_| Status::internal("Destination address is malformed".to_string()))?;
        let validator_node_signature = request
            .validator_node_signature
            .ok_or_else(|| Status::invalid_argument("Validator node signature is missing!"))?
            .try_into()
            .map_err(|_| Status::invalid_argument("Validator node signature is malformed!"))?;

        let sidechain_key = if request.sidechain_deployment_key.is_empty() {
            None
        } else {
            Some(
                PrivateKey::from_canonical_bytes(&request.sidechain_deployment_key)
                    .map_err(|_| Status::invalid_argument("sidechain_id is malformed"))?,
            )
        };

        let constants = self.get_consensus_constants().map_err(|e| {
            error!(target: LOG_TARGET, "Failed to get consensus constants: {e}");
            Status::internal("failed to fetch consensus constants")
        })?;

        let response = match transaction_service
            .submit_validator_node_exit(
                constants.validator_node_registration_min_deposit_amount(),
                validator_node_public_key,
                validator_node_signature,
                sidechain_key,
                request.max_epoch.into(),
                UtxoSelectionCriteria::default(),
                request.fee_per_gram.into(),
                MemoField::new_open(request.message, TxType::PaymentToSelf)
                    .map_err(|e| Status::internal(e.to_string()))?,
            )
            .await
        {
            Ok(tx) => SubmitValidatorNodeExitResponse {
                transaction_id: tx.as_u64(),
                is_success: true,
                failure_message: Default::default(),
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Transaction service error: {e}");
                SubmitValidatorNodeExitResponse {
                    transaction_id: Default::default(),
                    is_success: false,
                    failure_message: e.to_string(),
                }
            },
        };
        Ok(Response::new(response))
    }

    async fn submit_validator_eviction_proof(
        &self,
        request: Request<SubmitValidatorEvictionProofRequest>,
    ) -> Result<Response<SubmitValidatorEvictionProofResponse>, Status> {
        let request = request.into_inner();
        let mut transaction_service = self.get_transaction_service();

        let sidechain_key = Some(request.sidechain_deployment_key)
            .filter(|k| !k.is_empty())
            .map(|k| PrivateKey::from_canonical_bytes(&k))
            .transpose()
            .map_err(|_| Status::invalid_argument("sidechain_deployment_key is malformed"))?;

        let proof = request
            .proof
            .map(TryInto::try_into)
            .ok_or_else(|| Status::invalid_argument("Proof is missing"))?
            .map_err(|e| {
                error!(target: LOG_TARGET, "Failed to convert proof: {e}");
                Status::invalid_argument(format!("Invalid proof: {e}"))
            })?;

        let constants = self.get_consensus_constants().map_err(|e| {
            error!(target: LOG_TARGET, "Failed to get consensus constants: {e}");
            Status::internal("failed to fetch consensus constants")
        })?;

        let response = match transaction_service
            .submit_validator_eviction_proof(
                constants.validator_node_registration_min_deposit_amount(),
                proof,
                request.fee_per_gram.into(),
                sidechain_key,
                MemoField::new_open(request.message.into_bytes(), TxType::PaymentToSelf)
                    .map_err(|e| Status::internal(e.to_string()))?,
            )
            .await
        {
            Ok(tx) => SubmitValidatorEvictionProofResponse { tx_id: tx.as_u64() },
            Err(e) => {
                error!(target: LOG_TARGET, "Transaction service error: {e}");
                return Err(Status::unknown(e.to_string()));
            },
        };
        Ok(Response::new(response))
    }

    async fn import_transactions(
        &self,
        request: Request<ImportTransactionsRequest>,
    ) -> Result<Response<ImportTransactionsResponse>, Status> {
        let request = request.into_inner();
        let txs: Vec<WalletTransaction> = serde_json::from_str(&request.txs)
            .map_err(|_| Status::invalid_argument("Could not parse transactions. Use valid JSON format."))?;
        info!(target: LOG_TARGET, "Importing {:?} transactions", txs.len());

        let mut transaction_service = self.get_transaction_service();
        let mut tx_ids = Vec::new();
        for tx in txs {
            match transaction_service.import_transaction(tx).await {
                Ok(id) => {
                    tx_ids.push(id.into());
                },
                Err(e) => eprintln!("Could not import tx {e}"),
            };
        }
        Ok(Response::new(ImportTransactionsResponse { tx_ids }))
    }

    async fn get_payment_by_reference(
        &self,
        request: Request<GetPaymentByReferenceRequest>,
    ) -> Result<Response<GetPaymentByReferenceResponse>, Status> {
        let message = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "get_payment_by_reference: Looking up PayRef: {}",
            message.payment_reference.to_hex()
        );

        if message.payment_reference.len() != 32 {
            return Err(Status::invalid_argument(
                "payment_reference must be exactly 32 bytes".to_string(),
            ));
        }

        let payment_ref = message
            .payment_reference
            .try_into()
            .map_err(|_| Status::invalid_argument("payment_reference must be exactly 32 bytes".to_string()))?;
        let mut tms = self.get_transaction_service();

        match tms.get_transaction_by_payref(payment_ref).await {
            Ok(txn) => {
                let output_commitments: Vec<Vec<u8>> = txn
                    .transaction
                    .body
                    .outputs()
                    .iter()
                    .map(|o| o.commitment().as_bytes().to_vec())
                    .collect();
                let input_commitments: Vec<Vec<u8>> = txn
                    .transaction
                    .body
                    .inputs()
                    .iter()
                    .map(|i| match i.commitment() {
                        Ok(c) => c.as_bytes().to_vec(),
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to get input commitment: {e}");
                            vec![]
                        },
                    })
                    .collect();
                let transaction_info = TransactionInfo {
                    tx_id: txn.tx_id.into(),
                    source_address: txn.source_address.to_vec(),
                    dest_address: txn.destination_address.to_vec(),
                    status: TransactionStatus::from(txn.status) as i32,
                    amount: txn.amount.into(),
                    is_cancelled: txn.cancelled.is_some(),
                    direction: TransactionDirection::from(txn.direction) as i32,
                    fee: txn.fee.into(),
                    timestamp: txn.timestamp.timestamp() as u64,
                    excess_sig: txn
                        .transaction
                        .first_kernel_excess_sig()
                        .unwrap_or(&CompressedSignature::default())
                        .get_signature()
                        .to_vec(),
                    raw_payment_id: txn.payment_id.to_bytes(),
                    user_payment_id: txn.payment_id.payment_id_as_bytes(),
                    mined_in_block_height: txn.mined_height.unwrap_or(0),
                    output_commitments,
                    input_commitments,
                    payment_references_sent: txn
                        .calculate_sent_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                    payment_references_received: txn
                        .calculate_received_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                    payment_references_change: txn
                        .calculate_change_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                };
                Ok(Response::new(GetPaymentByReferenceResponse {
                    transaction: Some(transaction_info),
                }))
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "get_transaction_by_payref: Error looking up PayRef {}: {}",
                    payment_ref.to_hex(),
                    e
                );
                Err(Status::internal(format!("Error looking up payment reference: {e}")))
            },
        }
    }

    async fn get_transaction_pay_refs(
        &self,
        request: Request<GetTransactionPayRefsRequest>,
    ) -> Result<Response<GetTransactionPayRefsResponse>, Status> {
        let req = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "get_transaction_pay_refs: Getting PayRefs for transaction ID: {}",
            req.transaction_id
        );

        let mut transaction_service = self.get_transaction_service();
        let tx_id = TxId::from(req.transaction_id);

        match transaction_service.get_completed_transaction(tx_id).await {
            Ok(completed_tx) => {
                // Only return PayRefs if transaction is mined and has block hash
                if let Some(block_hash) = &completed_tx.mined_in_block {
                    let mut payment_references = Vec::new();

                    // Generate PayRefs from sent output hashes
                    for output_hash in &completed_tx.sent_output_hashes {
                        let payref = generate_payment_reference(block_hash, output_hash);
                        payment_references.push(payref.to_vec());
                    }

                    // Generate PayRefs from received output hashes
                    for output_hash in &completed_tx.received_output_hashes {
                        let payref = generate_payment_reference(block_hash, output_hash);
                        payment_references.push(payref.to_vec());
                    }

                    // Generate PayRefs from change output hashes (per-output approach)
                    for output_hash in &completed_tx.change_output_hashes {
                        let payref = generate_payment_reference(block_hash, output_hash);
                        payment_references.push(payref.to_vec());
                    }

                    debug!(
                        target: LOG_TARGET,
                        "get_transaction_pay_refs: Generated {} PayRefs for transaction {} (including change outputs)",
                        payment_references.len(),
                        req.transaction_id
                    );

                    Ok(Response::new(GetTransactionPayRefsResponse { payment_references }))
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "get_transaction_pay_refs: Transaction {} is not mined yet",
                        req.transaction_id
                    );
                    Ok(Response::new(GetTransactionPayRefsResponse {
                        payment_references: vec![],
                    }))
                }
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "get_transaction_pay_refs: Failed to get transaction {}: {}",
                    req.transaction_id,
                    e
                );
                Err(Status::not_found(format!(
                    "Transaction {} not found",
                    req.transaction_id
                )))
            },
        }
    }

    async fn get_fee_estimate(
        &self,
        request: Request<GetFeeEstimateRequest>,
    ) -> Result<Response<GetFeeEstimateResponse>, Status> {
        let message = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "get_fee_estimation: Incoming GRPC request with fee_per_gram: {}",
            message.fee_per_gram
        );

        let mut oms = self.get_output_manager_service();
        let fee_per_gram = message.fee_per_gram;
        let amount = message.amount;
        let output_count = usize::try_from(message.output_count)
            .map_err(|_| Status::internal("Count not convert u64 to usize".to_string()))?;
        let selection_criteria = UtxoSelectionCriteria::default();
        let (fee, inputs_selected, change) = oms
            .fee_estimate(
                amount.into(),
                selection_criteria,
                fee_per_gram.into(),
                1, // We assume 1 kernel for simplicity
                output_count,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GetFeeEstimateResponse {
            estimated_fee: fee.as_u64(),
            input_count: inputs_selected as u64,
            change_required: change,
        }))
    }

    async fn get_fee_per_gram_stats(
        &self,
        request: Request<GetFeePerGramStatsRequest>,
    ) -> Result<Response<GetFeePerGramStatsResponse>, Status> {
        let message = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "get_fee_per_gram_stats: Incoming GRPC request with count: {}",
            message.block_count
        );
        let block_count = message.block_count;

        let mut transaction_service = self.get_transaction_service();
        let stat = transaction_service
            .get_fee_per_gram_stats_per_block(block_count)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let fee_stats = vec![FeePerGramStat {
            average_fee_per_gram: stat.avg_fee_per_gram.as_u64(),
            min_fee_per_gram: stat.min_fee_per_gram.as_u64(),
            max_fee_per_gram: stat.max_fee_per_gram.as_u64(),
        }];
        Ok(Response::new(GetFeePerGramStatsResponse {
            fee_per_gram_stats: fee_stats,
        }))
    }

    async fn user_pay_for_fee(
        &self,
        request: Request<UserPayForFeeRequest>,
    ) -> Result<Response<UserPayForFeeResponse>, Status> {
        let message = request.into_inner();
        let recipients = message
            .recipients
            .into_iter()
            .enumerate()
            .map(|(index, transfer_with_id)| -> Result<_, String> {
                let dest = transfer_with_id.destination;
                let address = TariAddress::from_str(&dest)
                    .map_err(|_| format!("Destination address at index {index} is malformed"))?;
                Ok((address, transfer_with_id.fee, transfer_with_id.tx_id))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::invalid_argument)?;

        let transfer_results: Vec<(TariAddress, Result<TxId, TransactionServiceError>)> =
            future::join_all(recipients.iter().map(|(address, fee, tx_id)| async {
                (
                    address.to_owned(),
                    self.get_transaction_service()
                        .user_pay_for_fee(
                            TxId::from(tx_id.to_owned()),
                            address.to_owned(),
                            MicroMinotari::from(fee.to_owned()),
                        )
                        .await,
                )
            }))
            .await;

        let mut results = Vec::new();
        for (address, result) in transfer_results {
            match result {
                Ok(tx_id) => {
                    let wallet_address = self
                        .wallet
                        .get_wallet_one_sided_address()
                        .await
                        .map_err(|e| Status::internal(format!("{e:?}")))?;
                    let wallet_tx = timeout(Duration::from_millis(self.wallet.config.grpc_db_write_timeout), async {
                        loop {
                            let tx = self
                                .get_transaction_service()
                                .get_any_transaction(tx_id)
                                .await
                                .map_err(|e| Status::internal(format!("{e:?}")));

                            if let Ok(Some(tx)) = tx {
                                break tx;
                            }
                            sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .map_err(|_| {
                        error!(target: LOG_TARGET, "Transaction {tx_id} not found within timeout");
                        Status::not_found(format!("Transaction {tx_id} not found within timeout"))
                    })?;
                    let final_tx = convert_wallet_transaction_into_transaction_info(wallet_tx, &wallet_address);
                    results.push(TransferResult {
                        address: address.to_string(),
                        transaction_id: tx_id.into(),
                        is_success: true,
                        failure_message: Default::default(),
                        transaction_info: Some(final_tx),
                    });
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to send transaction for address `{address}`: {err}"
                    );
                    results.push(TransferResult {
                        address: address.to_string(),
                        transaction_id: Default::default(),
                        is_success: false,
                        failure_message: err.to_string(),
                        transaction_info: None,
                    });
                },
            }
        }
        Ok(Response::new(UserPayForFeeResponse { results }))
    }

    async fn replace_by_fee(
        &self,
        request: Request<ReplaceByFeeRequest>,
    ) -> Result<Response<ReplaceByFeeResponse>, Status> {
        let request = request.into_inner();
        let mut transaction_service = self.get_transaction_service();
        let tx_id = transaction_service
            .replace_by_fee(request.transaction_id.into(), MicroMinotari::from(request.fee_increase))
            .await
            .map_err(|e| Status::internal(format!("Failed to replace by fee: {e}")))?;
        Ok(Response::new(ReplaceByFeeResponse {
            transaction_id: tx_id.into(),
        }))
    }

    async fn sign_message(
        &self,
        request: Request<SignMessageRequest>,
    ) -> Result<Response<SignMessageResponse>, Status> {
        let message = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "sign_message: Incoming GRPC request with message length: {}",
            message.message.len()
        );

        let secret = self.wallet.node_identity.secret_key().clone();
        let message_str =
            String::from_utf8(message.message).map_err(|_| Status::invalid_argument("Message must be valid UTF-8"))?;

        let signature =
            SignatureWithDomain::<WalletMessageSigningDomain>::sign(&secret, message_str.as_bytes(), &mut OsRng)
                .map_err(|e| Status::internal(format!("Failed to sign message: {e}")))?;

        let hex_sig = signature.get_signature().to_hex();
        let hex_nonce = signature.get_public_nonce().to_hex();

        Ok(Response::new(SignMessageResponse {
            signature: hex_sig,
            public_nonce: hex_nonce,
        }))
    }

    async fn get_burn_claim_proof(
        &self,
        request: Request<GetBurnClaimProofRequest>,
    ) -> Result<Response<GetBurnClaimProofResponse>, Status> {
        let req = request.into_inner();
        let commitment = CompressedCommitment::from_canonical_bytes(&req.commitment)
            .map_err(|_| Status::invalid_argument("Commitment is malformed".to_string()))?;

        let proof = self
            .wallet
            .db
            .get_burn_proof_by_commitment(&commitment)
            .map_err(|e| {
                Status::internal(format!(
                    "Failed to get burn claim proof for commitment {}: {}",
                    commitment.to_compressed_key(),
                    e
                ))
            })?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "No burn claim proof found for commitment {}",
                    commitment.to_compressed_key()
                ))
            })?;

        let output = self
            .get_output_manager_service()
            .get_many_outputs(vec![proof.output_hash])
            .await
            .map_err(|e| {
                Status::internal(format!(
                    "Failed to get output for commitment {}: {}",
                    commitment.to_compressed_key(),
                    e
                ))
            })?
            .pop()
            .ok_or_else(|| {
                Status::not_found(format!(
                    "No output found for commitment {}",
                    commitment.to_compressed_key()
                ))
            })?;

        Ok(Response::new(GetBurnClaimProofResponse {
            claim_proof: Some(tari_rpc::BurnClaimProof {
                commitment: commitment.as_bytes().to_vec(),
                ownership_proof: Some(proof.burn_proof.ownership_proof.into()),
                reciprocal_claim_public_key: proof.burn_proof.reciprocal_claim_public_key.to_vec(),
            }),
            merkle_proof: proof.kernel_merkle_proof.map(|p| tari_rpc::EncodedMerkleProof {
                block_hash: p.block_hash.to_vec(),
                encoded_proof: p.encoded_merkle_proof,
                leaf_index: p.leaf_index,
            }),
            kernel: Some(proof.kernel.into()),
            encrypted_data: output.encrypted_data().to_byte_vec(),
            value: output.value().as_u64(),
        }))
    }

    async fn rescan_wallet(
        &self,
        request: Request<RescanWalletRequest>,
    ) -> Result<Response<RescanWalletResponse>, Status> {
        let message = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "rescan_wallet: Incoming GRPC request to rescan wallet with from_height: {}",
            message.from_height
        );

        if message.from_height == 0 {
            self.wallet
                .db
                .clear_scanned_blocks()
                .map_err(|e| Status::internal(format!("Failed to rescan wallet: {e}")))?;
        } else {
            self.wallet
                .db
                .clear_scanned_blocks_from_and_higher(message.from_height)
                .map_err(|e| Status::internal(format!("Failed to rescan wallet: {e}")))?;
        }

        Ok(Response::new(RescanWalletResponse {}))
    }
}

// Helper function to update the latency history and compute the average latency
fn update_and_average_latency(latencies: &mut VecDeque<u64>, new_latency: u64) -> u64 {
    latencies.push_front(new_latency);
    while latencies.len() > AVG_LATENCIES_CAPACITY {
        latencies.pop_back();
    }
    latencies.iter().sum::<u64>() / max(latencies.len() as u64, 1)
}

async fn handle_completed_tx(
    tx_id: TxId,
    event: &str,
    transaction_service: &mut TransactionServiceHandle,
    sender: &mut Sender<Result<TransactionEventResponse, Status>>,
) {
    match transaction_service.get_completed_transaction(tx_id).await {
        Ok(completed) => {
            let transaction_event =
                convert_to_transaction_event(event.to_string(), TransactionWrapper::Completed(Box::new(completed)));
            send_transaction_event(transaction_event, sender).await;
        },
        Err(e) => error!(target: LOG_TARGET, "Transaction service error: {e}"),
    }
}

async fn handle_pending_outbound(
    tx_id: TxId,
    event: &str,
    transaction_service: &mut TransactionServiceHandle,
    sender: &mut Sender<Result<TransactionEventResponse, Status>>,
) {
    use models::WalletTransaction::PendingOutbound;
    match transaction_service.get_any_transaction(tx_id).await {
        Ok(tx) => match tx {
            Some(PendingOutbound(tx)) => {
                let transaction_event =
                    convert_to_transaction_event(event.to_string(), TransactionWrapper::Outbound(Box::new(tx.clone())));
                send_transaction_event(transaction_event, sender).await;
            },
            _ => {
                error!(target: LOG_TARGET, "Not found in pending outbound set tx_id: {tx_id}");
            },
        },
        Err(e) => error!(target: LOG_TARGET, "Transaction service error: {e}"),
    }
}

fn simple_event(event: &str) -> TransactionEvent {
    TransactionEvent {
        event: event.to_string(),
        tx_id: String::default(),
        source_address: vec![],
        dest_address: vec![],
        status: event.to_string(),
        direction: event.to_string(),
        amount: 0,
        raw_payment_id: vec![],
        user_payment_id: vec![],
    }
}

#[allow(clippy::too_many_lines)]
fn convert_wallet_transaction_into_transaction_info(
    tx: models::WalletTransaction,
    wallet_address: &TariAddress,
) -> TransactionInfo {
    use models::WalletTransaction::{Completed, PendingInbound, PendingOutbound};
    match tx {
        PendingInbound(tx) => {
            let output_commitments = match tx.receiver_protocol.state {
                RecipientState::Finalized(data) => vec![data.output.commitment.as_bytes().to_vec()],
                _ => vec![],
            };
            TransactionInfo {
                tx_id: tx.tx_id.into(),
                source_address: tx.source_address.to_vec(),
                dest_address: wallet_address.to_vec(),
                status: TransactionStatus::from(tx.status) as i32,
                amount: tx.amount.into(),
                is_cancelled: tx.cancelled,
                direction: TransactionDirection::Inbound as i32,
                fee: 0,
                excess_sig: Default::default(),
                timestamp: tx.timestamp.timestamp() as u64,
                raw_payment_id: tx.payment_id.to_bytes(),
                user_payment_id: tx.payment_id.payment_id_as_bytes(),
                mined_in_block_height: 0,
                output_commitments,
                input_commitments: vec![],
                payment_references_sent: vec![],
                payment_references_received: vec![],
                payment_references_change: vec![],
            }
        },
        PendingOutbound(tx) => {
            let output_commitments = match tx.sender_protocol.get_output_commitments() {
                Ok(v) => v.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to get output commitments: {e}");
                    vec![]
                },
            };
            let input_commitments = match tx.sender_protocol.get_input_commitments() {
                Ok(v) => v.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to get output commitments: {e}");
                    vec![]
                },
            };
            TransactionInfo {
                tx_id: tx.tx_id.into(),
                source_address: wallet_address.to_vec(),
                dest_address: tx.destination_address.to_vec(),
                status: TransactionStatus::from(tx.status) as i32,
                amount: tx.amount.into(),
                is_cancelled: tx.cancelled,
                direction: TransactionDirection::Outbound as i32,
                fee: tx.fee.into(),
                excess_sig: Default::default(),
                timestamp: tx.timestamp.timestamp() as u64,
                raw_payment_id: tx.payment_id.to_bytes(),
                user_payment_id: tx.payment_id.payment_id_as_bytes(),
                mined_in_block_height: 0,
                output_commitments,
                input_commitments,
                payment_references_sent: vec![],
                payment_references_received: vec![],
                payment_references_change: vec![],
            }
        },
        Completed(tx) => {
            let output_commitments: Vec<Vec<u8>> = tx
                .transaction
                .body
                .outputs()
                .iter()
                .map(|o| o.commitment().as_bytes().to_vec())
                .collect();
            let input_commitments = tx
                .transaction
                .body
                .inputs()
                .iter()
                .map(|i| match i.commitment() {
                    Ok(c) => c.as_bytes().to_vec(),
                    Err(e) => {
                        warn!(target: LOG_TARGET, "Failed to get input commitment: {e}");
                        vec![]
                    },
                })
                .collect();
            TransactionInfo {
                tx_id: tx.tx_id.into(),
                source_address: tx.source_address.to_vec(),
                dest_address: tx.destination_address.to_vec(),
                status: TransactionStatus::from(tx.status) as i32,
                amount: tx.amount.into(),
                is_cancelled: tx.cancelled.is_some(),
                direction: TransactionDirection::from(tx.direction) as i32,
                fee: tx.fee.into(),
                timestamp: tx.timestamp.timestamp() as u64,
                excess_sig: tx
                    .transaction
                    .first_kernel_excess_sig()
                    .map(|s| s.get_signature().to_vec())
                    .unwrap_or_default(),
                raw_payment_id: tx.payment_id.to_bytes(),
                user_payment_id: tx.payment_id.payment_id_as_bytes(),
                mined_in_block_height: tx.mined_height.unwrap_or(0),
                output_commitments: output_commitments.clone(),
                input_commitments,
                payment_references_sent: tx
                    .calculate_sent_payment_references()
                    .into_iter()
                    .map(|pr| pr.to_vec())
                    .collect(),
                payment_references_received: tx
                    .calculate_received_payment_references()
                    .into_iter()
                    .map(|pr| pr.to_vec())
                    .collect(),
                payment_references_change: tx
                    .calculate_change_payment_references()
                    .into_iter()
                    .map(|pr| pr.to_vec())
                    .collect(),
            }
        },
    }
}
