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

use futures::{channel::mpsc, future, SinkExt};
use log::*;
use tari_app_grpc::{
    conversions::naive_datetime_to_timestamp,
    tari_rpc::{
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
        CreateCommitteeDefinitionRequest,
        CreateCommitteeDefinitionResponse,
        CreateFollowOnAssetCheckpointRequest,
        CreateFollowOnAssetCheckpointResponse,
        CreateInitialAssetCheckpointRequest,
        CreateInitialAssetCheckpointResponse,
        GetBalanceRequest,
        GetBalanceResponse,
        GetCoinbaseRequest,
        GetCoinbaseResponse,
        GetCompletedTransactionsRequest,
        GetCompletedTransactionsResponse,
        GetConnectivityRequest,
        GetIdentityRequest,
        GetIdentityResponse,
        GetOwnedAssetsResponse,
        GetTransactionInfoRequest,
        GetTransactionInfoResponse,
        GetUnspentAmountsResponse,
        GetVersionRequest,
        GetVersionResponse,
        ImportUtxosRequest,
        ImportUtxosResponse,
        MintTokensRequest,
        MintTokensResponse,
        RegisterAssetRequest,
        RegisterAssetResponse,
        RevalidateRequest,
        RevalidateResponse,
        SendShaAtomicSwapRequest,
        SendShaAtomicSwapResponse,
        SetBaseNodeRequest,
        SetBaseNodeResponse,
        TransactionDirection,
        TransactionInfo,
        TransactionStatus,
        TransferRequest,
        TransferResponse,
        TransferResult,
    },
};
use tari_common_types::{
    array::copy_into_fixed_array,
    types::{BlockHash, PublicKey, Signature},
};
use tari_comms::{multiaddr::Multiaddr, types::CommsPublicKey, CommsNode};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction_components::{OutputFeatures, UnblindedOutput},
};
use tari_crypto::{ristretto::RistrettoPublicKey, tari_utilities::Hashable};
use tari_utilities::{hex::Hex, ByteArray};
use tari_wallet::{
    connectivity_service::{OnlineStatus, WalletConnectivityInterface},
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{handle::TransactionServiceHandle, storage::models},
    WalletSqlite,
};
use tokio::task;
use tonic::{Request, Response, Status};

const LOG_TARGET: &str = "wallet::ui::grpc";

pub struct WalletGrpcServer {
    wallet: WalletSqlite,
}

impl WalletGrpcServer {
    pub fn new(wallet: WalletSqlite) -> Self {
        Self { wallet }
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
}

#[tonic::async_trait]
impl wallet_server::Wallet for WalletGrpcServer {
    type GetCompletedTransactionsStream = mpsc::Receiver<Result<GetCompletedTransactionsResponse, Status>>;

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
            tari_wallet::connectivity_service::OnlineStatus::Connecting => OnlineStatus::Connecting,
            tari_wallet::connectivity_service::OnlineStatus::Online => OnlineStatus::Online,
            tari_wallet::connectivity_service::OnlineStatus::Offline => OnlineStatus::Offline,
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

        if let Some(ref update) = *self.wallet.get_software_updater().new_update_notifier().borrow() {
            resp.has_update = true;
            resp.version = update.version().to_string();
            resp.sha = update.to_hash_hex();
            resp.download_url = update.download_url().to_string();
        }

        Ok(Response::new(resp))
    }

    async fn identify(&self, _: Request<GetIdentityRequest>) -> Result<Response<GetIdentityResponse>, Status> {
        let identity = self.wallet.comms.node_identity();
        Ok(Response::new(GetIdentityResponse {
            public_key: identity.public_key().to_string().into_bytes(),
            public_address: identity.public_address().to_string(),
            node_id: identity.node_id().to_string().into_bytes(),
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
        let balance;
        match output_service.get_balance().await {
            Ok(b) => balance = b,
            Err(e) => return Err(Status::not_found(format!("GetBalance error! {}", e))),
        }
        Ok(Response::new(GetBalanceResponse {
            available_balance: balance.available_balance.0,
            pending_incoming_balance: balance.pending_incoming_balance.0,
            pending_outgoing_balance: balance.pending_outgoing_balance.0,
        }))
    }

    async fn get_unspent_amounts(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<GetUnspentAmountsResponse>, Status> {
        let mut output_service = self.get_output_manager_service();
        let unspent_amounts;
        match output_service.get_unspent_outputs().await {
            Ok(uo) => unspent_amounts = uo,
            Err(e) => return Err(Status::not_found(format!("GetUnspentAmounts error! {}", e))),
        }
        Ok(Response::new(GetUnspentAmountsResponse {
            amount: unspent_amounts
                .into_iter()
                .map(|o| o.value.as_u64())
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

    async fn get_coinbase(
        &self,
        request: Request<GetCoinbaseRequest>,
    ) -> Result<Response<GetCoinbaseResponse>, Status> {
        let request = request.into_inner();
        let mut tx_service = self.get_transaction_service();

        let coinbase = tx_service
            .generate_coinbase_transaction(request.reward.into(), request.fee.into(), request.height)
            .await
            .map_err(|err| Status::unknown(err.to_string()))?;

        let coinbase = coinbase.try_into().map_err(Status::internal)?;
        Ok(Response::new(GetCoinbaseResponse {
            transaction: Some(coinbase),
        }))
    }

    async fn send_sha_atomic_swap_transaction(
        &self,
        request: Request<SendShaAtomicSwapRequest>,
    ) -> Result<Response<SendShaAtomicSwapResponse>, Status> {
        let message = request
            .into_inner()
            .recipient
            .ok_or_else(|| Status::internal("Request is malformed".to_string()))?;
        let address = CommsPublicKey::from_hex(&message.address)
            .map_err(|_| Status::internal("Destination address is malformed".to_string()))?;

        let mut transaction_service = self.get_transaction_service();
        let response = match transaction_service
            .send_sha_atomic_swap_transaction(
                address.clone(),
                message.amount.into(),
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
                let pk = CommsPublicKey::from_hex(&dest.address)
                    .map_err(|_| format!("Destination address at index {} is malformed", idx))?;
                Ok((
                    dest.address,
                    pk,
                    dest.amount,
                    dest.fee_per_gram,
                    dest.message,
                    dest.payment_type,
                ))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::invalid_argument)?;

        let mut standard_transfers = Vec::new();
        let mut one_sided_transfers = Vec::new();
        for (address, pk, amount, fee_per_gram, message, payment_type) in recipients.into_iter() {
            let mut transaction_service = self.get_transaction_service();
            if payment_type == PaymentType::StandardMimblewimble as i32 {
                standard_transfers.push(async move {
                    (
                        address,
                        transaction_service
                            .send_transaction(pk, amount.into(), fee_per_gram.into(), message)
                            .await,
                    )
                });
            } else if payment_type == PaymentType::OneSided as i32 {
                one_sided_transfers.push(async move {
                    (
                        address,
                        transaction_service
                            .send_one_sided_transaction(pk, amount.into(), fee_per_gram.into(), message)
                            .await,
                    )
                });
            }
        }

        let standard_results = future::join_all(standard_transfers).await;
        let one_sided_results = future::join_all(one_sided_transfers).await;

        let results = standard_results
            .into_iter()
            .chain(one_sided_results.into_iter())
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

        let transactions = transactions
            .map(|(tx_id, tx)| match tx {
                Some(tx) => convert_wallet_transaction_into_transaction_info(tx, wallet_pk),
                None => TransactionInfo::not_found(tx_id),
            })
            .collect();

        Ok(Response::new(GetTransactionInfoResponse { transactions }))
    }

    async fn get_completed_transactions(
        &self,
        _request: Request<GetCompletedTransactionsRequest>,
    ) -> Result<Response<Self::GetCompletedTransactionsStream>, Status> {
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetAllCompletedTransactions"
        );
        let mut transaction_service = self.get_transaction_service();
        let transactions = transaction_service
            .get_completed_transactions()
            .await
            .map_err(|err| Status::not_found(format!("No completed transactions found: {:?}", err)))?;

        let (mut sender, receiver) = mpsc::channel(transactions.len());
        task::spawn(async move {
            for (_, txn) in transactions {
                let response = GetCompletedTransactionsResponse {
                    transaction: Some(TransactionInfo {
                        tx_id: txn.tx_id.into(),
                        source_pk: txn.source_public_key.to_vec(),
                        dest_pk: txn.destination_public_key.to_vec(),
                        status: TransactionStatus::from(txn.status) as i32,
                        amount: txn.amount.into(),
                        is_cancelled: txn.cancelled.is_some(),
                        direction: TransactionDirection::from(txn.direction) as i32,
                        fee: txn.fee.into(),
                        timestamp: Some(naive_datetime_to_timestamp(txn.timestamp)),
                        excess_sig: txn
                            .transaction
                            .first_kernel_excess_sig()
                            .unwrap_or(&Signature::default())
                            .get_signature()
                            .to_vec(),
                        message: txn.message,
                    }),
                };
                match sender.send(Ok(response)).await {
                    Ok(_) => (),
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

        let lock_height = if message.lock_height == 0 {
            None
        } else {
            Some(message.lock_height)
        };

        let mut wallet = self.wallet.clone();

        let tx_id = wallet
            .coin_split(
                MicroTari::from(message.amount_per_split),
                message.split_count as usize,
                MicroTari::from(message.fee_per_gram),
                message.message,
                lock_height,
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

        for o in unblinded_outputs.iter() {
            tx_ids.push(
                wallet
                    .import_unblinded_utxo(o.clone(), &CommsPublicKey::default(), "Imported via gRPC".to_string())
                    .await
                    .map_err(|e| Status::internal(format!("{:?}", e)))?
                    .into(),
            );
        }

        Ok(Response::new(ImportUtxosResponse { tx_ids }))
    }

    async fn register_asset(
        &self,
        request: Request<RegisterAssetRequest>,
    ) -> Result<Response<RegisterAssetResponse>, Status> {
        let mut manager = self.wallet.asset_manager.clone();
        let mut transaction_service = self.wallet.transaction_service.clone();
        let message = request.into_inner();
        let public_key = PublicKey::from_bytes(message.public_key.as_slice())
            .map_err(|e| Status::invalid_argument(format!("Asset public key was not a valid pub key: {}", e)))?;
        let (tx_id, transaction) = manager
            .create_registration_transaction(
                message.name,
                public_key,
                message.template_ids_implemented,
                Some(message.description),
                Some(message.image),
                message.template_parameters.into_iter().map(|tp| tp.into()).collect(),
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let asset_public_key = transaction
            .body
            .outputs()
            .iter()
            .filter_map(|tx| match tx.features.asset.clone() {
                Some(asset) => Some(asset.public_key),
                None => None,
            })
            .next()
            .unwrap();
        let message = format!("Asset registration for {}", asset_public_key);
        let _result = transaction_service
            .submit_transaction(tx_id, transaction, 0.into(), message)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(RegisterAssetResponse {
            public_key: Vec::from(asset_public_key.as_bytes()),
        }))
    }

    async fn get_owned_assets(&self, _: Request<tari_rpc::Empty>) -> Result<Response<GetOwnedAssetsResponse>, Status> {
        let mut asset_manager = self.wallet.asset_manager.clone();
        let owned = asset_manager
            .list_owned_assets()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let owned = owned
            .into_iter()
            .map(|asset| tari_rpc::Asset {
                name: asset.name().to_string(),
                registration_output_status: asset.registration_output_status().to_string(),
                public_key: Vec::from(asset.public_key().as_bytes()),
                owner_commitment: Vec::from(asset.owner_commitment().as_bytes()),
                description: asset.description().to_string(),
                image: asset.image().to_string(),
            })
            .collect();
        Ok(Response::new(tari_rpc::GetOwnedAssetsResponse { assets: owned }))
    }

    async fn create_initial_asset_checkpoint(
        &self,
        request: Request<CreateInitialAssetCheckpointRequest>,
    ) -> Result<Response<CreateInitialAssetCheckpointResponse>, Status> {
        let mut asset_manager = self.wallet.asset_manager.clone();
        let mut transaction_service = self.wallet.transaction_service.clone();
        let message = request.into_inner();

        let asset_public_key = PublicKey::from_bytes(message.asset_public_key.as_slice())
            .map_err(|e| Status::invalid_argument(format!("Asset public key was not a valid pub key:{}", e)))?;

        let merkle_root = copy_into_fixed_array(&message.merkle_root)
            .map_err(|_| Status::invalid_argument("Merkle root has an incorrect length"))?;

        let (tx_id, transaction) = asset_manager
            .create_initial_asset_checkpoint(&asset_public_key, merkle_root)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let message = format!("Initial asset checkpoint for {}", asset_public_key);
        let _result = transaction_service
            .submit_transaction(tx_id, transaction, 0.into(), message)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateInitialAssetCheckpointResponse {}))
    }

    async fn create_follow_on_asset_checkpoint(
        &self,
        request: Request<CreateFollowOnAssetCheckpointRequest>,
    ) -> Result<Response<CreateFollowOnAssetCheckpointResponse>, Status> {
        let mut asset_manager = self.wallet.asset_manager.clone();
        let mut transaction_service = self.wallet.transaction_service.clone();
        let message = request.into_inner();

        let asset_public_key = PublicKey::from_bytes(message.asset_public_key.as_slice())
            .map_err(|e| Status::invalid_argument(format!("Asset public key was not a valid pub key:{}", e)))?;

        let merkle_root = copy_into_fixed_array(&message.merkle_root)
            .map_err(|_| Status::invalid_argument("Incorrect merkle root length"))?;

        let (tx_id, transaction) = asset_manager
            .create_follow_on_asset_checkpoint(&asset_public_key, message.unique_id.as_slice(), merkle_root)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let message = format!("Asset state checkpoint for {}", asset_public_key);
        let _result = transaction_service
            .submit_transaction(tx_id, transaction, 0.into(), message)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateFollowOnAssetCheckpointResponse {}))
    }

    async fn create_committee_definition(
        &self,
        request: Request<CreateCommitteeDefinitionRequest>,
    ) -> Result<Response<CreateCommitteeDefinitionResponse>, Status> {
        let mut asset_manager = self.wallet.asset_manager.clone();
        let mut transaction_service = self.wallet.transaction_service.clone();
        let message = request.into_inner();

        let asset_public_key = PublicKey::from_bytes(message.asset_public_key.as_slice())
            .map_err(|e| Status::invalid_argument(format!("Asset public key was not a valid pub key:{}", e)))?;
        let committee_public_keys: Vec<RistrettoPublicKey> = message
            .committee
            .iter()
            .map(|c| PublicKey::from_bytes(c.as_slice()))
            .collect::<Result<_, _>>()
            .map_err(|err| Status::invalid_argument(format!("Committee did not contain valid pub keys:{}", err)))?;
        let effective_sidechain_height = message.effective_sidechain_height;

        let (tx_id, transaction) = asset_manager
            .create_committee_definition(&asset_public_key, &committee_public_keys, effective_sidechain_height)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let message = format!(
            "Committee checkpoint for asset {} with effective sidechain height {}",
            asset_public_key, effective_sidechain_height
        );
        let _result = transaction_service
            .submit_transaction(tx_id, transaction, 0.into(), message)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateCommitteeDefinitionResponse {}))
    }

    async fn mint_tokens(&self, request: Request<MintTokensRequest>) -> Result<Response<MintTokensResponse>, Status> {
        let mut asset_manager = self.wallet.asset_manager.clone();
        let mut transaction_service = self.wallet.transaction_service.clone();
        let message = request.into_inner();

        let asset_public_key =
            PublicKey::from_bytes(message.asset_public_key.as_slice()).map_err(|e| Status::internal(e.to_string()))?;
        let asset = asset_manager
            .get_owned_asset_by_pub_key(&asset_public_key)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut token_features = vec![];
        for tari_rpc::MintTokenInfo { unique_id, features } in message.tokens {
            let f: Option<OutputFeatures> = features
                .map(|f| f.try_into())
                .transpose()
                .map_err(Status::invalid_argument)?;
            token_features.push((unique_id, f));
        }

        let (tx_id, transaction) = asset_manager
            .create_minting_transaction(&asset_public_key, asset.owner_commitment(), token_features)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let owner_commitments: Vec<Vec<u8>> = transaction
            .body
            .outputs()
            .iter()
            .filter_map(|o| o.features.unique_id.as_ref().map(|_| o.commitment.to_vec()))
            .collect();

        let message = format!(
            "Minting {} tokens for asset {}",
            owner_commitments.len(),
            asset_public_key
        );
        let _result = transaction_service
            .submit_transaction(tx_id, transaction, 0.into(), message)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(MintTokensResponse { owner_commitments }))
    }

    async fn get_owned_tokens(
        &self,
        request: Request<tari_rpc::GetOwnedTokensRequest>,
    ) -> Result<Response<tari_rpc::GetOwnedTokensResponse>, Status> {
        let request = request.into_inner();
        let request_public_key = PublicKey::from_bytes(&request.asset_public_key)
            .map_err(|e| Status::invalid_argument(format!("asset_public key was not a valid public key: {}", e)))?;
        let mut token_manager = self.wallet.token_manager.clone();
        let owned = token_manager
            .list_owned_tokens()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let owned = owned
            .into_iter()
            .filter_map(|t| {
                if t.asset_public_key() == &request_public_key {
                    Some(tari_rpc::TokenUtxo {
                        name: t.name().to_string(),
                        output_status: t.output_status().to_string(),
                        asset_public_key: Vec::from(t.asset_public_key().as_bytes()),
                        unique_id: Vec::from(t.unique_id()),
                        commitment: Vec::from(t.owner_commitment().as_bytes()),
                    })
                } else {
                    None
                }
            })
            .collect();
        Ok(Response::new(tari_rpc::GetOwnedTokensResponse { tokens: owned }))
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
            num_node_connections: status.num_connected_nodes() as u32,
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
}

fn convert_wallet_transaction_into_transaction_info(
    tx: models::WalletTransaction,
    wallet_pk: &CommsPublicKey,
) -> TransactionInfo {
    use models::WalletTransaction::*;
    match tx {
        PendingInbound(tx) => TransactionInfo {
            tx_id: tx.tx_id.into(),
            source_pk: tx.source_public_key.to_vec(),
            dest_pk: wallet_pk.to_vec(),
            status: TransactionStatus::from(tx.status) as i32,
            amount: tx.amount.into(),
            is_cancelled: tx.cancelled,
            direction: TransactionDirection::Inbound as i32,
            fee: 0,
            excess_sig: Default::default(),
            timestamp: Some(naive_datetime_to_timestamp(tx.timestamp)),
            message: tx.message,
        },
        PendingOutbound(tx) => TransactionInfo {
            tx_id: tx.tx_id.into(),
            source_pk: wallet_pk.to_vec(),
            dest_pk: tx.destination_public_key.to_vec(),
            status: TransactionStatus::from(tx.status) as i32,
            amount: tx.amount.into(),
            is_cancelled: tx.cancelled,
            direction: TransactionDirection::Outbound as i32,
            fee: tx.fee.into(),
            excess_sig: Default::default(),
            timestamp: Some(naive_datetime_to_timestamp(tx.timestamp)),
            message: tx.message,
        },
        Completed(tx) => TransactionInfo {
            tx_id: tx.tx_id.into(),
            source_pk: tx.source_public_key.to_vec(),
            dest_pk: tx.destination_public_key.to_vec(),
            status: TransactionStatus::from(tx.status) as i32,
            amount: tx.amount.into(),
            is_cancelled: tx.cancelled.is_some(),
            direction: TransactionDirection::from(tx.direction) as i32,
            fee: tx.fee.into(),
            timestamp: Some(naive_datetime_to_timestamp(tx.timestamp)),
            excess_sig: tx
                .transaction
                .first_kernel_excess_sig()
                .map(|s| s.get_signature().to_vec())
                .unwrap_or_default(),
            message: tx.message,
        },
    }
}
