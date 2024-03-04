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

use std::convert::{TryFrom, TryInto};

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
    GetAddressResponse,
    GetBalanceRequest,
    GetBalanceResponse,
    GetCompletedTransactionsRequest,
    GetCompletedTransactionsResponse,
    GetConnectivityRequest,
    GetIdentityRequest,
    GetIdentityResponse,
    GetTransactionInfoRequest,
    GetTransactionInfoResponse,
    GetUnspentAmountsResponse,
    GetVersionRequest,
    GetVersionResponse,
    ImportUtxosRequest,
    ImportUtxosResponse,
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
    connectivity_service::{OnlineStatus, WalletConnectivityInterface},
    error::WalletStorageError,
    output_manager_service::{handle::OutputManagerHandle, UtxoSelectionCriteria},
    transaction_service::{
        handle::TransactionServiceHandle,
        storage::models::{self, WalletTransaction},
    },
    WalletSqlite,
};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{BlockHash, PublicKey, Signature},
};
use tari_comms::{multiaddr::Multiaddr, types::CommsPublicKey, CommsNode};
use tari_core::{
    consensus::{ConsensusBuilderError, ConsensusConstants, ConsensusManager},
    transactions::{
        tari_amount::{MicroMinotari, T},
        transaction_components::{
            CodeTemplateRegistration,
            OutputFeatures,
            OutputType,
            SideChainFeature,
            UnblindedOutput,
        },
    },
};
use tari_script::script;
use tari_utilities::{hex::Hex, ByteArray};
use tokio::{sync::broadcast, task};
use tonic::{Request, Response, Status};

use crate::{
    grpc::{convert_to_transaction_event, TransactionWrapper},
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
}

impl WalletGrpcServer {
    pub fn new(wallet: WalletSqlite) -> Result<Self, ConsensusBuilderError> {
        let rules = ConsensusManager::builder(wallet.network.as_network()).build()?;
        Ok(Self { wallet, rules })
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
        // spend more than we need to or the the transaction is rejected.
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
        let grpc_connectivity = match status {
            minotari_wallet::connectivity_service::OnlineStatus::Connecting => OnlineStatus::Connecting,
            minotari_wallet::connectivity_service::OnlineStatus::Online => OnlineStatus::Online,
            minotari_wallet::connectivity_service::OnlineStatus::Offline => OnlineStatus::Offline,
        };
        Ok(Response::new(CheckConnectivityResponse {
            status: grpc_connectivity as i32,
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
        let identity = self.wallet.comms.node_identity();
        Ok(Response::new(GetIdentityResponse {
            public_key: identity.public_key().to_vec(),
            public_address: identity.public_addresses().iter().map(|a| a.to_string()).collect(),
            node_id: identity.node_id().to_vec(),
        }))
    }

    async fn get_address(&self, _: Request<tari_rpc::Empty>) -> Result<Response<GetAddressResponse>, Status> {
        let network = self.wallet.network.as_network();
        let pk = self.wallet.comms.node_identity().public_key().clone();
        let address = TariAddress::new(pk, network);
        Ok(Response::new(GetAddressResponse {
            address: address.to_bytes().to_vec(),
        }))
    }

    async fn set_base_node(
        &self,
        request: Request<SetBaseNodeRequest>,
    ) -> Result<Response<SetBaseNodeResponse>, Status> {
        let message = request.into_inner();
        let public_key = PublicKey::from_hex(&message.public_key_hex)
            .map_err(|e| Status::invalid_argument(format!("Base node public key was not a valid pub key: {}", e)))?;
        let net_address = message
            .net_address
            .parse::<Multiaddr>()
            .map_err(|e| Status::invalid_argument(format!("Base node net address was not valid: {}", e)))?;

        println!("Setting base node peer...");
        println!("{}::{}", public_key, net_address);
        let mut wallet = self.wallet.clone();
        wallet
            .set_base_node_peer(public_key.clone(), net_address.clone())
            .await
            .map_err(|e| Status::internal(format!("{:?}", e)))?;

        Ok(Response::new(SetBaseNodeResponse {}))
    }

    async fn get_balance(&self, _request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        let mut output_service = self.get_output_manager_service();
        let balance = match output_service.get_balance().await {
            Ok(b) => b,
            Err(e) => return Err(Status::not_found(format!("GetBalance error! {}", e))),
        };
        Ok(Response::new(GetBalanceResponse {
            available_balance: balance.available_balance.0,
            pending_incoming_balance: balance.pending_incoming_balance.0,
            pending_outgoing_balance: balance.pending_outgoing_balance.0,
            timelocked_balance: balance.time_locked_balance.unwrap_or_default().0,
        }))
    }

    async fn get_unspent_amounts(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<GetUnspentAmountsResponse>, Status> {
        let mut output_service = self.get_output_manager_service();
        let unspent_amounts = match output_service.get_unspent_outputs().await {
            Ok(uo) => uo,
            Err(e) => return Err(Status::not_found(format!("GetUnspentAmounts error! {}", e))),
        };
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
        Ok(Response::new(RevalidateResponse {}))
    }

    async fn validate_all_transactions(
        &self,
        _request: Request<ValidateRequest>,
    ) -> Result<Response<ValidateResponse>, Status> {
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
        let address = TariAddress::from_hex(&message.address)
            .map_err(|_| Status::internal("Destination address is malformed".to_string()))?;

        let mut transaction_service = self.get_transaction_service();
        let response = match transaction_service
            .send_sha_atomic_swap_transaction(
                address.clone(),
                message.amount.into(),
                UtxoSelectionCriteria::default(),
                message.fee_per_gram.into(),
                message.message,
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
                        "Claiming HTLC transaction with pre-image".to_string(),
                    )
                    .await
                {
                    Ok(()) => TransferResult {
                        address: Default::default(),
                        transaction_id: tx_id.as_u64(),
                        is_success: true,
                        failure_message: Default::default(),
                    },
                    Err(e) => TransferResult {
                        address: Default::default(),
                        transaction_id: Default::default(),
                        is_success: false,
                        failure_message: e.to_string(),
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
                    .submit_transaction(tx_id, tx, amount, "Creating HTLC refund transaction".to_string())
                    .await
                {
                    Ok(()) => TransferResult {
                        address: Default::default(),
                        transaction_id: tx_id.as_u64(),
                        is_success: true,
                        failure_message: Default::default(),
                    },
                    Err(e) => TransferResult {
                        address: Default::default(),
                        transaction_id: Default::default(),
                        is_success: false,
                        failure_message: e.to_string(),
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
                }
            },
        };

        Ok(Response::new(ClaimHtlcRefundResponse {
            results: Some(response),
        }))
    }

    async fn transfer(&self, request: Request<TransferRequest>) -> Result<Response<TransferResponse>, Status> {
        let message = request.into_inner();
        let recipients = message
            .recipients
            .into_iter()
            .enumerate()
            .map(|(idx, dest)| -> Result<_, String> {
                let address = TariAddress::from_hex(&dest.address)
                    .map_err(|_| format!("Destination address at index {} is malformed", idx))?;
                Ok((
                    dest.address,
                    address,
                    dest.amount,
                    dest.fee_per_gram,
                    dest.message,
                    dest.payment_type,
                ))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::invalid_argument)?;

        let mut transfers = Vec::new();
        for (hex_address, address, amount, fee_per_gram, message, payment_type) in recipients {
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
                                message,
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
                                message,
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
                                message,
                            )
                            .await
                    },
                )
            });
        }

        let transfers_results = future::join_all(transfers).await;

        let results = transfers_results
            .into_iter()
            .map(|(address, result)| match result {
                Ok(tx_id) => TransferResult {
                    address,
                    transaction_id: tx_id.into(),
                    is_success: true,
                    failure_message: Default::default(),
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to send transaction for address `{}`: {}", address, err
                    );
                    TransferResult {
                        address,
                        transaction_id: Default::default(),
                        is_success: false,
                        failure_message: err.to_string(),
                    }
                },
            })
            .collect();

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
                message.message,
                if message.claim_public_key.is_empty() {
                    None
                } else {
                    Some(
                        PublicKey::from_canonical_bytes(&message.claim_public_key)
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

        let transactions = future::try_join_all(queries)
            .await
            .map(|tx| tx.into_iter())
            .map_err(|err| Status::unknown(err.to_string()))?;

        let wallet_pk = self.wallet.comms.node_identity_ref().public_key();
        let wallet_network = self.wallet.network.as_network();
        let wallet_address = TariAddress::new(wallet_pk.clone(), wallet_network);
        let transactions = transactions
            .map(|(tx_id, tx)| match tx {
                Some(tx) => convert_wallet_transaction_into_transaction_info(tx, &wallet_address),
                None => TransactionInfo::not_found(tx_id),
            })
            .collect();

        Ok(Response::new(GetTransactionInfoResponse { transactions }))
    }

    async fn stream_transaction_events(
        &self,
        _request: tonic::Request<TransactionEventRequest>,
    ) -> Result<Response<Self::StreamTransactionEventsStream>, Status> {
        let (mut sender, receiver) = mpsc::channel(100);

        // let event_listener = self.events_channel.clone();

        // let mut shutdown_signal = self.wallet;
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
                                        ReceivedTransaction(_) | ReceivedTransactionReply(_)  | TransactionBroadcast(_) | TransactionMinedRequestTimedOut(_) | TransactionImported(_) => {
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
                    }
            }
        });
        Ok(Response::new(receiver))
    }

    async fn get_completed_transactions(
        &self,
        _request: Request<GetCompletedTransactionsRequest>,
    ) -> Result<Response<Self::GetCompletedTransactionsStream>, Status> {
        debug!(
            target: LOG_TARGET,
            "GetAllCompletedTransactions: Incoming GRPC request"
        );
        let mut transaction_service = self.get_transaction_service();
        let transactions = transaction_service
            .get_completed_transactions()
            .await
            .map_err(|err| Status::not_found(format!("No completed transactions found: {:?}", err)))?;
        debug!(
            target: LOG_TARGET,
            "GetAllCompletedTransactions: Found {} completed transactions",
            transactions.len()
        );

        let (mut sender, receiver) = mpsc::channel(transactions.len());
        task::spawn(async move {
            for (i, (_, txn)) in transactions.iter().enumerate() {
                let response = GetCompletedTransactionsResponse {
                    transaction: Some(TransactionInfo {
                        tx_id: txn.tx_id.into(),
                        source_address: txn.source_address.to_bytes().to_vec(),
                        dest_address: txn.destination_address.to_bytes().to_vec(),
                        status: TransactionStatus::from(txn.status.clone()) as i32,
                        amount: txn.amount.into(),
                        is_cancelled: txn.cancelled.is_some(),
                        direction: TransactionDirection::from(txn.direction.clone()) as i32,
                        fee: txn.fee.into(),
                        timestamp: txn.timestamp.timestamp() as u64,
                        excess_sig: txn
                            .transaction
                            .first_kernel_excess_sig()
                            .unwrap_or(&Signature::default())
                            .get_signature()
                            .to_vec(),
                        message: txn.message.clone(),
                    }),
                };
                match sender.send(Ok(response)).await {
                    Ok(_) => {
                        debug!(
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

        Ok(Response::new(receiver))
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
                message.message,
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
                        "Imported via gRPC".to_string(),
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

        let mut peers = Vec::with_capacity(connected_peers.len());
        for conn in connected_peers {
            peers.push(
                peer_manager
                    .find_by_node_id(conn.peer_node_id())
                    .await
                    .map_err(|err| Status::internal(err.to_string()))?
                    .ok_or_else(|| Status::not_found(format!("Peer '{}' not found", conn.peer_node_id())))?,
            );
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

        let message = format!("Template registration {}", template_registration.template_name);
        let mut output = output_manager
            .create_output_with_features(1 * T, OutputFeatures {
                output_type: OutputType::CodeTemplateRegistration,
                sidechain_feature: Some(SideChainFeature::CodeTemplateRegistration(template_registration)),
                ..Default::default()
            })
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        output = output.with_script(script![Nop]);

        let (tx_id, transaction) = output_manager
            .create_send_to_self_with_output(vec![output], fee_per_gram.into(), UtxoSelectionCriteria::default())
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
            .submit_transaction(tx_id, transaction, 0.into(), message)
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
                request.message,
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
    match transaction_service.get_pending_outbound_transactions().await {
        Ok(mut txs) => {
            if let Some(tx) = txs.remove(&tx_id) {
                let transaction_event =
                    convert_to_transaction_event(event.to_string(), TransactionWrapper::Outbound(Box::new(tx)));
                send_transaction_event(transaction_event, sender).await;
            } else {
                error!(target: LOG_TARGET, "Not found in pending outbound set tx_id: {}", tx_id);
            }
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
        message: String::default(),
    }
}

fn convert_wallet_transaction_into_transaction_info(
    tx: models::WalletTransaction,
    wallet_address: &TariAddress,
) -> TransactionInfo {
    use models::WalletTransaction::{Completed, PendingInbound, PendingOutbound};
    match tx {
        PendingInbound(tx) => TransactionInfo {
            tx_id: tx.tx_id.into(),
            source_address: tx.source_address.to_bytes().to_vec(),
            dest_address: wallet_address.to_bytes().to_vec(),
            status: TransactionStatus::from(tx.status) as i32,
            amount: tx.amount.into(),
            is_cancelled: tx.cancelled,
            direction: TransactionDirection::Inbound as i32,
            fee: 0,
            excess_sig: Default::default(),
            timestamp: tx.timestamp.timestamp() as u64,
            message: tx.message,
        },
        PendingOutbound(tx) => TransactionInfo {
            tx_id: tx.tx_id.into(),
            source_address: wallet_address.to_bytes().to_vec(),
            dest_address: tx.destination_address.to_bytes().to_vec(),
            status: TransactionStatus::from(tx.status) as i32,
            amount: tx.amount.into(),
            is_cancelled: tx.cancelled,
            direction: TransactionDirection::Outbound as i32,
            fee: tx.fee.into(),
            excess_sig: Default::default(),
            timestamp: tx.timestamp.timestamp() as u64,
            message: tx.message,
        },
        Completed(tx) => TransactionInfo {
            tx_id: tx.tx_id.into(),
            source_address: tx.source_address.to_bytes().to_vec(),
            dest_address: tx.destination_address.to_bytes().to_vec(),
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
            message: tx.message,
        },
    }
}
