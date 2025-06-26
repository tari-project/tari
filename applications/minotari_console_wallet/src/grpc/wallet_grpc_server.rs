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
    CoinSplitRequest,
    CoinSplitResponse,
    CommitmentSignature,
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
    PrepareOneSidedTransactionForSigningRequest,
    PrepareOneSidedTransactionForSigningResponse,
    RegisterValidatorNodeRequest,
    RegisterValidatorNodeResponse,
    RevalidateRequest,
    RevalidateResponse,
    SendShaAtomicSwapRequest,
    SendShaAtomicSwapResponse,
    SetBaseNodeRequest,
    SetBaseNodeResponse,
    TransactionDirection,
    TransactionEvent,
    TransactionEventRequest,
    TransactionEventResponse,
    TransactionInfo,
    TransactionStatus,
    TransferRequest,
    TransferResponse,
    TransferResult,
    ValidateRequest,
    ValidateResponse,
};
use minotari_wallet::{
    connectivity_service::WalletConnectivityInterface,
    error::WalletStorageError,
    output_manager_service::{handle::OutputManagerHandle, UtxoSelectionCriteria},
    transaction_service::{
        handle::TransactionServiceHandle,
        offline_signing::models::{SignedOneSidedTransactionResult, TransactionResult},
        storage::models::{self, WalletTransaction},
    },
    WalletSqlite,
};
use tari_common_types::{
    payment_reference::generate_payment_reference,
    tari_address::TariAddress,
    transaction::TxId,
    types::{BlockHash, CompressedPublicKey, Signature},
};
use tari_comms::{multiaddr::Multiaddr, types::CommsPublicKey, CommsNode};
use tari_core::{
    consensus::{ConsensusBuilderError, ConsensusConstants, ConsensusManager},
    transactions::{
        tari_amount::{MicroMinotari, T},
        transaction_components::{
            payment_id::{PaymentId, TxType},
            CodeTemplateRegistration,
            OutputFeatures,
            OutputType,
            SideChainFeature,
            UnblindedOutput,
        },
        transaction_key_manager::TransactionKeyManagerInterface,
        transaction_protocol::recipient::RecipientState,
    },
};
use tari_script::script;
use tari_utilities::{hex::Hex, ByteArray};
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
        warn!(target: LOG_TARGET, "Error sending transaction via GRPC:  {}", err);
        if let Err(send_err) = sender.send(Err(Status::unknown("Error sending data"))).await {
            warn!(target: LOG_TARGET, "Error sending error to GRPC client: {}", send_err)
        }
    }
}

pub struct WalletGrpcServer {
    wallet: WalletSqlite,
    rules: ConsensusManager,
    debouncer: Arc<Mutex<WalletDebouncer>>,
}

impl WalletGrpcServer {
    #[allow(dead_code)]
    pub fn new(wallet: WalletSqlite) -> Result<Self, ConsensusBuilderError> {
        let rules = ConsensusManager::builder(wallet.network.as_network()).build()?;
        let debouncer = WalletDebouncer::new(
            wallet.output_manager_service.clone(),
            wallet.transaction_service.clone(),
            wallet.wallet_connectivity.clone(),
            wallet.utxo_scanner_service.clone(),
            wallet.comms.shutdown_signal(),
        );
        Ok(Self {
            wallet,
            rules,
            debouncer: Arc::new(Mutex::new(debouncer)),
        })
    }

    pub async fn start_balance_debouncer_event_monitor(&self) {
        self.debouncer.lock().await.start_event_monitor_if_needed().await
    }

    fn get_transaction_service(&self) -> TransactionServiceHandle {
        self.wallet.transaction_service.clone()
    }

    fn get_output_manager_service(&self) -> OutputManagerHandle {
        self.wallet.output_manager_service.clone()
    }

    fn comms(&self) -> &CommsNode {
        &self.wallet.comms
    }

    fn get_consensus_constants(&self) -> Result<&ConsensusConstants, WalletStorageError> {
        // If we don't have the chain metadata, we hope that VNReg consensus constants did not change - worst case, we
        // spend more than we need to or the transaction is rejected.
        let height = self
            .wallet
            .db
            .get_chain_metadata()?
            .map(|m| m.best_block_height())
            .unwrap_or_default();
        Ok(self.rules.consensus_constants(height))
    }
}

#[tonic::async_trait]
impl wallet_server::Wallet for WalletGrpcServer {
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
        let mut connectivity = self.wallet.wallet_connectivity.clone();
        let status = connectivity.get_connectivity_status();
        Ok(Response::new(CheckConnectivityResponse { status: status as i32 }))
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
        let identity = self.wallet.comms.node_identity();
        Ok(Response::new(GetIdentityResponse {
            public_key: identity.public_key().to_vec(),
            public_address: identity.public_addresses().iter().map(|a| a.to_string()).collect(),
            node_id: identity.node_id().to_vec(),
        }))
    }

    async fn get_address(&self, _: Request<tari_rpc::Empty>) -> Result<Response<GetAddressResponse>, Status> {
        let interactive_address = self
            .wallet
            .get_wallet_interactive_address()
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
        let one_sided_address = self
            .wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
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

        let interactive_address = self
            .wallet
            .get_wallet_interactive_address()
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
        let interactive_address = interactive_address
            .with_payment_id_user_data(message.payment_id.clone())
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
        let one_sided_address = self
            .wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
        let one_sided_address = one_sided_address
            .with_payment_id_user_data(message.payment_id)
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
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
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
        let one_sided_address = self
            .wallet
            .get_wallet_one_sided_address()
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;

        Ok(Response::new(GetCompleteAddressResponse {
            interactive_address: interactive_address.to_vec(),
            one_sided_address: one_sided_address.to_vec(),
            interactive_address_base58: interactive_address.to_base58(),
            one_sided_address_base58: one_sided_address.to_base58(),
            interactive_address_emoji: interactive_address.to_emoji_string(),
            one_sided_address_emoji: one_sided_address.to_emoji_string(),
        }))
    }

    async fn set_base_node(
        &self,
        request: Request<SetBaseNodeRequest>,
    ) -> Result<Response<SetBaseNodeResponse>, Status> {
        let message = request.into_inner();
        let public_key = CompressedPublicKey::from_hex(&message.public_key_hex)
            .map_err(|e| Status::invalid_argument(format!("Base node public key was not a valid pub key: {}", e)))?;
        let net_address = message
            .net_address
            .parse::<Multiaddr>()
            .map_err(|e| Status::invalid_argument(format!("Base node net address was not valid: {}", e)))?;

        println!("Setting base node peer...");
        println!("{}::{}", public_key, net_address);
        let mut wallet = self.wallet.clone();
        wallet
            .set_base_node_peer(public_key.clone(), Some(net_address.clone()), None)
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;

        Ok(Response::new(SetBaseNodeResponse {}))
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
                .map_err(|e| Status::not_found(format!("WalletDebouncer error! {}", e)))?;
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
                Err(e) => return Err(Status::not_found(format!("WalletDebouncer error! {}", e))),
            }
        };
        trace!(target: LOG_TARGET, "'get_balance' completed in {:.2?}", start.elapsed());
        Ok(Response::new(balance))
    }

    async fn get_state(&self, _request: Request<GetStateRequest>) -> Result<Response<GetStateResponse>, Status> {
        let start = std::time::Instant::now();
        let (balance, scanned_height) = {
            let mut debouncer = self.debouncer.lock().await;
            let balance = match debouncer.get_balance().await {
                Ok(b) => b,
                Err(e) => return Err(Status::not_found(format!("WalletDebouncer error! {}", e))),
            };
            let scanned_height = debouncer.get_scanned_height().await;
            (Some(balance), scanned_height)
        };

        let status = self
            .comms()
            .connectivity()
            .get_connectivity_status()
            .await
            .map_err(|err| Status::internal(err.to_string()))?;
        let mut base_node_service = self.wallet.base_node_service.clone();

        let network = Some(tari_rpc::NetworkStatusResponse {
            status: tari_rpc::ConnectivityStatus::from(status) as i32,
            avg_latency_ms: base_node_service
                .get_base_node_latency()
                .await
                .map_err(|err| Status::internal(err.to_string()))?
                .map(|d| u32::try_from(d.as_millis()).unwrap_or(u32::MAX))
                .unwrap_or_default(),
            num_node_connections: u32::try_from(status.num_connected_nodes())
                .map_err(|_| Status::internal("Count not convert u64 to usize".to_string()))?,
        });

        trace!(target: LOG_TARGET, "'get_state' completed in {:.2?}", start.elapsed());
        Ok(Response::new(GetStateResponse {
            scanned_height,
            balance,
            network,
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
            Err(e) => return Err(Status::not_found(format!("GetUnspentAmounts error! {}", e))),
        };
        trace!(target: LOG_TARGET, "'get_unspent_amounts' completed in {:.2?}", start.elapsed());
        Ok(Response::new(GetUnspentAmountsResponse {
            amount: unspent_amounts
                .into_iter()
                .map(|o| o.wallet_output.value.as_u64())
                .filter(|&a| a > 0)
                .collect(),
        }))
    }

    async fn revalidate_all_transactions(
        &self,
        _request: Request<RevalidateRequest>,
    ) -> Result<Response<RevalidateResponse>, Status> {
        let start = std::time::Instant::now();
        let mut output_service = self.get_output_manager_service();
        output_service
            .revalidate_all_outputs()
            .await
            .map_err(|e| Status::unknown(e.to_string()))?;
        let mut tx_service = self.get_transaction_service();
        tx_service
            .revalidate_all_transactions()
            .await
            .map_err(|e| Status::unknown(e.to_string()))?;
        trace!(target: LOG_TARGET, "'revalidate_all_transactions' completed in {:.2?}", start.elapsed());
        Ok(Response::new(RevalidateResponse {}))
    }

    async fn validate_all_transactions(
        &self,
        _request: Request<ValidateRequest>,
    ) -> Result<Response<ValidateResponse>, Status> {
        let start = std::time::Instant::now();
        let mut output_service = self.get_output_manager_service();
        output_service
            .validate_txos()
            .await
            .map_err(|e| Status::unknown(e.to_string()))?;
        let mut tx_service = self.get_transaction_service();
        tx_service
            .validate_transactions()
            .await
            .map_err(|e| Status::unknown(e.to_string()))?;
        trace!(target: LOG_TARGET, "'validate_all_transactions' completed in {:.2?}", start.elapsed());
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
            PaymentId::from_bytes(&message.raw_payment_id)
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
            PaymentId::Open {
                user_data: bytes,
                tx_type: TxType::ClaimAtomicSwap,
            }
        } else {
            PaymentId::Empty
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
                    "Failed to send Sha - XTR atomic swap for address `{}`: {}", address, e
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
                        PaymentId::open_from_string(
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
                            .map_err(|e| Status::internal(format!("{:?}", e)))?;
                        let wallet_tx = self
                            .get_transaction_service()
                            .get_any_transaction(tx_id)
                            .await
                            .map_err(|e| Status::internal(format!("{:?}", e)))?
                            .ok_or_else(|| Status::not_found("Transaction not found".to_string()))?;
                        let final_tx = convert_wallet_transaction_into_transaction_info(
                            wallet_tx,
                            &wallet_address,
                            &self.wallet.key_manager_service,
                        )
                        .await;
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
                warn!(target: LOG_TARGET, "Failed to claim SHA - XTR atomic swap: {}", e);
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
                        PaymentId::open_from_string("Creating HTLC refund transaction", TxType::HtlcAtomicSwapRefund),
                    )
                    .await
                {
                    Ok(()) => {
                        let wallet_address = self
                            .wallet
                            .get_wallet_one_sided_address()
                            .await
                            .map_err(|e| Status::internal(format!("{:?}", e)))?;
                        let wallet_tx = self
                            .get_transaction_service()
                            .get_any_transaction(tx_id)
                            .await
                            .map_err(|e| Status::internal(format!("{:?}", e)))?
                            .ok_or_else(|| Status::not_found("Transaction not found".to_string()))?;
                        let final_tx = convert_wallet_transaction_into_transaction_info(
                            wallet_tx,
                            &wallet_address,
                            &self.wallet.key_manager_service,
                        )
                        .await;
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
                warn!(target: LOG_TARGET, "Failed to claim HTLC refund transaction: {}", e);
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
            PaymentId::from_bytes(&recipient.raw_payment_id)
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
            PaymentId::Open {
                user_data: bytes,
                tx_type: TxType::PaymentToOther,
            }
        } else {
            PaymentId::Empty
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
                    "Failed to lock transaction for address `{}`: {}", address, err
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
                    "Failed to broadcast a signed transaction: {}", err
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

    #[allow(clippy::too_many_lines)]
    async fn transfer(&self, request: Request<TransferRequest>) -> Result<Response<TransferResponse>, Status> {
        let message = request.into_inner();
        let recipients = message
            .recipients
            .into_iter()
            .enumerate()
            .map(|(idx, dest)| -> Result<_, String> {
                let address = TariAddress::from_str(&dest.address)
                    .map_err(|_| format!("Destination address at index {} is malformed", idx))?;
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
            let payment_id = if !raw_payment_id.is_empty() {
                PaymentId::open(raw_payment_id.to_vec(), TxType::PaymentToOther)
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
                PaymentId::Open {
                    user_data: bytes,
                    tx_type: TxType::PaymentToOther,
                }
            } else {
                PaymentId::Empty
            };
            let mut transaction_service = self.get_transaction_service();
            transfers.push(async move {
                (
                    hex_address,
                    if payment_type == PaymentType::StandardMimblewimble as i32 {
                        transaction_service
                            .send_transaction(
                                address,
                                amount.into(),
                                UtxoSelectionCriteria::default(),
                                OutputFeatures::default(),
                                fee_per_gram.into(),
                                payment_id,
                            )
                            .await
                    } else if payment_type == PaymentType::OneSided as i32 {
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
                        .map_err(|e| Status::internal(format!("{:?}", e)))?;
                    let wallet_tx = timeout(Duration::from_millis(100), async {
                        loop {
                            let tx = self
                                .get_transaction_service()
                                .get_any_transaction(tx_id)
                                .await
                                .map_err(|e| Status::internal(format!("{:?}", e)));

                            if let Ok(Some(tx)) = tx {
                                break tx;
                            }
                            sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .map_err(|_| {
                        error!(target: LOG_TARGET, "Transaction {} not found within timeout", tx_id);
                        Status::not_found(format!("Transaction {} not found within timeout", tx_id))
                    })?;
                    let final_tx = convert_wallet_transaction_into_transaction_info(
                        wallet_tx,
                        &wallet_address,
                        &self.wallet.key_manager_service,
                    )
                    .await;
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
                        "Failed to send transaction for address `{}`: {}", address, err
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

    async fn create_burn_transaction(
        &self,
        request: Request<CreateBurnTransactionRequest>,
    ) -> Result<Response<CreateBurnTransactionResponse>, Status> {
        let message = request.into_inner();

        let mut transaction_service = self.get_transaction_service();
        debug!(target: LOG_TARGET, "Trying to burn {} Minotari", message.amount);
        let response = match transaction_service
            .burn_tari(
                message.amount.into(),
                UtxoSelectionCriteria::default(),
                message.fee_per_gram.into(),
                PaymentId::from_bytes(&message.payment_id),
                if message.claim_public_key.is_empty() {
                    None
                } else {
                    Some(
                        CompressedPublicKey::from_canonical_bytes(&message.claim_public_key)
                            .map_err(|e| Status::invalid_argument(e.to_string()))?,
                    )
                },
            )
            .await
        {
            Ok((tx_id, proof)) => {
                debug!(target: LOG_TARGET, "Transaction broadcast: {}", tx_id,);
                CreateBurnTransactionResponse {
                    transaction_id: tx_id.as_u64(),
                    is_success: true,
                    failure_message: Default::default(),
                    commitment: proof.commitment.to_vec(),
                    ownership_proof: proof.ownership_proof.map(CommitmentSignature::from),
                    range_proof: proof.range_proof.to_vec(),
                    reciprocal_claim_public_key: proof.reciprocal_claim_public_key.to_vec(),
                }
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Failed to burn Tarid: {}", e);
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
            .map_err(|e| Status::internal(format!("{:?}", e)))?;
        let mut transactions = Vec::new();
        for (tx_id, tx) in all_transactions {
            transactions.push(match tx {
                Some(tx) => {
                    convert_wallet_transaction_into_transaction_info(
                        tx,
                        &wallet_address,
                        &self.wallet.key_manager_service,
                    )
                    .await
                },
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

        let mut shutdown_signal = self.wallet.comms.shutdown_signal();
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
                                            Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
                                            _ => error!(target: LOG_TARGET, "Transaction not found tx_id: {}", tx_id),
                                        }
                                    },
                                    TransactionCompletedImmediately(tx_id) => handle_pending_outbound(tx_id, SENT, &mut transaction_service, &mut sender).await,
                                    TransactionSendResult(tx_id, status) => {
                                        let is_sent = status.direct_send_result || status.store_and_forward_send_result;
                                        let event = if is_sent { SENT } else { QUEUED };
                                        handle_pending_outbound(tx_id, event, &mut transaction_service, &mut sender).await;
                                    },
                                    TransactionValidationStateChanged(_t_operation_id) => {
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
                                warn!(target: LOG_TARGET, "Missed {} from Transaction events", n);
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
            "GetAllCompletedTransactions: Incoming GRPC request"
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
            .map_err(|err| Status::not_found(format!("No completed transactions found: {:?}", err)))?;
        debug!(
            target: LOG_TARGET,
            "GetAllCompletedTransactions: Found {} completed transactions",
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
                            warn!(target: LOG_TARGET, "Failed to get input commitment: {}", e);
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
                            .unwrap_or(&Signature::default())
                            .get_signature()
                            .to_vec(),
                        raw_payment_id: txn.payment_id.to_bytes(),
                        user_payment_id: txn.payment_id.user_data_as_bytes(),
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
                            "GetAllCompletedTransactions: Sent transaction TxId: {} ({} of {})",
                            txn.tx_id,
                            i + 1,
                            transactions.len()
                        );
                    },
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Error sending transaction via GRPC:  {}", err);
                        match sender.send(Err(Status::unknown("Error sending data"))).await {
                            Ok(_) => (),
                            Err(send_err) => {
                                warn!(target: LOG_TARGET, "Error sending error to GRPC client: {}", send_err)
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

    #[allow(clippy::too_many_lines)]
    async fn get_all_completed_transactions(
        &self,
        request: Request<GetAllCompletedTransactionsRequest>,
    ) -> Result<Response<GetAllCompletedTransactionsResponse>, Status> {
        let start = std::time::Instant::now();
        let req = request.into_inner();
        trace!(
            target: LOG_TARGET,
            "GetAllCompletedTransactions: Incoming GRPC request"
        );
        let mut transaction_service = self.get_transaction_service();

        let mut completed_transactions = transaction_service
            .get_completed_transactions(None, None, None, 0)
            .await
            .map_err(|err| {
                Status::not_found(format!(
                    "GetAllCompletedTransactions: Error found for get_completed_transactions: {:?}",
                    err
                ))
            })?;
        completed_transactions.extend(
            transaction_service
                .get_cancelled_completed_transactions(0)
                .await
                .map_err(|err| {
                    Status::not_found(format!(
                        "GetAllCompletedTransactions: Error found for get_cancelled_completed_transactions: {:?}",
                        err
                    ))
                })?,
        );

        completed_transactions.sort_by(|a, b| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .expect("Should be able to compare timestamps")
        });

        let offset = usize::try_from(req.offset).unwrap_or(0);
        let limit = if req.limit > 0 {
            usize::try_from(req.limit).unwrap_or(usize::MAX)
        } else {
            usize::MAX
        };
        let mut transactions: Vec<TransactionInfo> = Vec::new();
        for txn in completed_transactions
            .into_iter()
            .filter(|tx| req.status_bitflag == 0 || (req.status_bitflag & (1 << (tx.status as u32))) != 0)
            .skip(offset)
            .take(limit)
        {
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
                        warn!(target: LOG_TARGET, "Failed to get input commitment: {}", e);
                        vec![]
                    },
                })
                .collect();

            transactions.push(TransactionInfo {
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
                    .unwrap_or(&Signature::default())
                    .get_signature()
                    .to_vec(),
                raw_payment_id: txn.payment_id.to_bytes(),
                user_payment_id: txn.payment_id.user_data_as_bytes(),
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

        trace!(target: LOG_TARGET, "'GetAllCompletedTransactions' completed in {:.2?}", start.elapsed());
        Ok(Response::new(GetAllCompletedTransactionsResponse { transactions }))
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
                    "GetBlockHeightTransactions: Error found at block height {}: {:?}",
                    block_height, err
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
                        warn!(target: LOG_TARGET, "Failed to get input commitment: {}", e);
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
                    .unwrap_or(&Signature::default())
                    .get_signature()
                    .to_vec(),
                raw_payment_id: txn.payment_id.to_bytes(),
                user_payment_id: txn.payment_id.user_data_as_bytes(),
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
                PaymentId::open_from_string("Creating coin-split transaction", TxType::CoinSplit),
            )
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;

        Ok(Response::new(CoinSplitResponse { tx_id: tx_id.into() }))
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
                        PaymentId::from_bytes(&message.payment_id),
                    )
                    .await
                    .map_err(|e| Status::internal(format!("{:?}", e)))?
                    .into(),
            );
        }

        Ok(Response::new(ImportUtxosResponse { tx_ids }))
    }

    async fn get_network_status(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::NetworkStatusResponse>, Status> {
        let status = self
            .comms()
            .connectivity()
            .get_connectivity_status()
            .await
            .map_err(|err| Status::internal(err.to_string()))?;
        let mut base_node_service = self.wallet.base_node_service.clone();

        let resp = tari_rpc::NetworkStatusResponse {
            status: tari_rpc::ConnectivityStatus::from(status) as i32,
            avg_latency_ms: base_node_service
                .get_base_node_latency()
                .await
                .map_err(|err| Status::internal(err.to_string()))?
                .map(|d| u32::try_from(d.as_millis()).unwrap_or(u32::MAX))
                .unwrap_or_default(),
            num_node_connections: u32::try_from(status.num_connected_nodes())
                .map_err(|_| Status::internal("Count not convert u64 to usize".to_string()))?,
        };

        Ok(Response::new(resp))
    }

    async fn list_connected_peers(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::ListConnectedPeersResponse>, Status> {
        let mut connectivity = self.comms().connectivity();
        let peer_manager = self.comms().peer_manager();
        let connected_peers = connectivity
            .get_active_connections()
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let node_ids = connected_peers
            .iter()
            .map(|c| c.peer_node_id())
            .cloned()
            .collect::<Vec<_>>();
        let peers = peer_manager
            .get_peers_by_node_ids(&node_ids)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;
        if peers.len() != node_ids.len() {
            let mut error_response = Vec::new();
            node_ids.iter().for_each(|node_id| {
                if !peers.iter().any(|p| p.node_id == *node_id) {
                    warn!(target: LOG_TARGET, "Peer '{}' not found", node_id);
                    error_response.push(format!("'{}'", node_id));
                }
            });
            if !error_response.is_empty() {
                return Err(Status::not_found(format!(
                    "Peer(s) not found: {}",
                    error_response.join(", ")
                )));
            }
        }

        let resp = tari_rpc::ListConnectedPeersResponse {
            connected_peers: peers.into_iter().map(Into::into).collect(),
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
        let mut output_manager = self.wallet.output_manager_service.clone();
        let mut transaction_service = self.wallet.transaction_service.clone();
        let message = request.into_inner();

        let template_registration = CodeTemplateRegistration::try_from(
            message
                .template_registration
                .ok_or_else(|| Status::invalid_argument("template_registration is empty"))?,
        )
        .map_err(|e| Status::invalid_argument(format!("template_registration is invalid: {}", e)))?;
        let fee_per_gram = message.fee_per_gram;
        let template_name = template_registration.template_name.clone();

        let mut output = output_manager
            .create_output_with_features(1 * T, OutputFeatures {
                output_type: OutputType::CodeTemplateRegistration,
                sidechain_feature: Some(SideChainFeature::CodeTemplateRegistration(template_registration)),
                ..Default::default()
            })
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        output = output.with_script(script![Nop].map_err(|e| Status::invalid_argument(e.to_string()))?);
        let payment_id = PaymentId::open_from_string(
            &format!("Template registration '{}'", template_name),
            TxType::CodeTemplateRegistration,
        );

        let (tx_id, transaction) = output_manager
            .create_send_to_self_with_output(
                vec![output],
                fee_per_gram.into(),
                UtxoSelectionCriteria::default(),
                payment_id.clone(),
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        debug!(
            target: LOG_TARGET,
            "Template registration transaction: {:?}", transaction
        );

        let reg_output = transaction
            .body
            .outputs()
            .iter()
            .find(|o| o.features.output_type == OutputType::CodeTemplateRegistration)
            .ok_or_else(|| Status::internal("No code template registration output!"))?;
        let template_address = reg_output.hash();

        transaction_service
            .submit_transaction(tx_id, transaction, 0.into(), payment_id)
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

        let constants = self.get_consensus_constants().map_err(|e| {
            error!(target: LOG_TARGET, "Failed to get consensus constants: {}", e);
            Status::internal("failed to fetch consensus constants")
        })?;

        let response = match transaction_service
            .register_validator_node(
                constants.validator_node_registration_min_deposit_amount(),
                validator_node_public_key,
                validator_node_signature,
                UtxoSelectionCriteria::default(),
                request.fee_per_gram.into(),
                PaymentId::from_bytes(&request.payment_id),
            )
            .await
        {
            Ok(tx) => RegisterValidatorNodeResponse {
                transaction_id: tx.as_u64(),
                is_success: true,
                failure_message: Default::default(),
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Transaction service error: {}", e);
                RegisterValidatorNodeResponse {
                    transaction_id: Default::default(),
                    is_success: false,
                    failure_message: e.to_string(),
                }
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
                Err(e) => eprintln!("Could not import tx {}", e),
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
                            warn!(target: LOG_TARGET, "Failed to get input commitment: {}", e);
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
                        .unwrap_or(&Signature::default())
                        .get_signature()
                        .to_vec(),
                    raw_payment_id: txn.payment_id.to_bytes(),
                    user_payment_id: txn.payment_id.user_data_as_bytes(),
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
                Err(Status::internal(format!("Error looking up payment reference: {}", e)))
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
        let fee = oms
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
        let block_count = usize::try_from(message.block_count)
            .map_err(|_| Status::internal("Count not convert u64 to usize".to_string()))?;

        let mut transaction_service = self.get_transaction_service();
        let stats = transaction_service
            .get_fee_per_gram_stats_per_block(block_count)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let mut fee_stats = Vec::new();
        for stat in stats.stats {
            fee_stats.push(FeePerGramStat {
                average_fee_per_gram: stat.avg_fee_per_gram.as_u64(),
                min_fee_per_gram: stat.min_fee_per_gram.as_u64(),
                max_fee_per_gram: stat.max_fee_per_gram.as_u64(),
            });
        }
        Ok(Response::new(GetFeePerGramStatsResponse {
            fee_per_gram_stats: fee_stats,
        }))
    }
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
        Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
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
                error!(target: LOG_TARGET, "Not found in pending outbound set tx_id: {}", tx_id);
            },
        },
        Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
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
async fn convert_wallet_transaction_into_transaction_info<KM: TransactionKeyManagerInterface>(
    tx: models::WalletTransaction,
    wallet_address: &TariAddress,
    key_manager: &KM,
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
                user_payment_id: tx.payment_id.user_data_as_bytes(),
                mined_in_block_height: 0,
                output_commitments,
                input_commitments: vec![],
                payment_references_sent: vec![],
                payment_references_received: vec![],
                payment_references_change: vec![],
            }
        },
        PendingOutbound(tx) => {
            let output_commitments = match tx.sender_protocol.get_output_commitments(key_manager).await {
                Ok(v) => v.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to get output commitments: {}", e);
                    vec![]
                },
            };
            let input_commitments = match tx.sender_protocol.get_input_commitments(key_manager).await {
                Ok(v) => v.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to get output commitments: {}", e);
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
                user_payment_id: tx.payment_id.user_data_as_bytes(),
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
                        warn!(target: LOG_TARGET, "Failed to get input commitment: {}", e);
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
                user_payment_id: tx.payment_id.user_data_as_bytes(),
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
