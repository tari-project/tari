use futures::future;
use log::*;
use std::str::FromStr;
use tari_app_grpc::tari_rpc::{
    wallet_server,
    GetCoinbaseRequest,
    GetCoinbaseResponse,
    GetTransactionDetailsRequest,
    GetTransactionDetailsResponse,
    TransactionDetailsResult,
    TransferRequest,
    TransferResponse,
    TransferResult,
};
use tari_comms::types::CommsPublicKey;
use tari_core::tari_utilities::hex::Hex;
use tari_wallet::{
    transaction_service::{
        handle::TransactionServiceHandle,
        storage::models::WalletTransaction::{Completed, PendingInbound, PendingOutbound},
    },
    WalletSqlite,
};
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
}

#[tonic::async_trait]
impl wallet_server::Wallet for WalletGrpcServer {
    async fn get_coinbase(
        &self,
        request: Request<GetCoinbaseRequest>,
    ) -> Result<Response<GetCoinbaseResponse>, Status>
    {
        let request = request.into_inner();

        let mut tx_service = self.get_transaction_service();
        let response = tx_service
            .generate_coinbase_transaction(request.reward.into(), request.fee.into(), request.height)
            .await;

        match response {
            Ok(resp) => Ok(Response::new(GetCoinbaseResponse {
                transaction: Some(resp.into()),
            })),
            Err(err) => Err(Status::unknown(err.to_string())),
        }
    }

    async fn get_transaction_details(
        &self,
        request: Request<GetTransactionDetailsRequest>,
    ) -> Result<Response<GetTransactionDetailsResponse>, Status>
    {
        let request_inner = request.into_inner();
        let tx_id = match u64::from_str(&request_inner.tx_id.to_string()) {
            Ok(val) => val,
            Err(err) => {
                let msg = format!(
                    "Transaction ID '{}' malformed, should be an unsigned integer value. ({})",
                    &request_inner.tx_id.to_string(),
                    err.to_string()
                );
                info!(target: LOG_TARGET, "{}", msg);
                return Err(Status::unknown(msg));
            },
        };

        let mut tx_service = self.get_transaction_service();
        let tx_status_result: TransactionDetailsResult;
        let response = tx_service.get_any_transaction(tx_id).await;
        match response {
            Ok(found) => match found {
                None => return Err(Status::unknown(format!("Transaction '{}' not found.", tx_id))),
                Some(val) => {
                    tx_status_result = match val {
                        PendingInbound(tx) => TransactionDetailsResult {
                            txid: tx_id.to_string(),
                            source_pub_key: tx.source_public_key.to_hex(),
                            dest_pub_key: format!("{}", self.wallet.comms.node_identity().public_key()),
                            direction: "Inbound".to_string(),
                            amount: format!("{}", tx.amount),
                            fee: "Not known".to_string(),
                            status: format!("{}", tx.status),
                            cancelled: tx.cancelled,
                            message: tx.message,
                            time_stamp: tx.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                        },
                        PendingOutbound(tx) => TransactionDetailsResult {
                            txid: tx_id.to_string(),
                            source_pub_key: format!("{}", self.wallet.comms.node_identity().public_key()),
                            dest_pub_key: tx.destination_public_key.to_hex(),
                            direction: "Outbound".to_string(),
                            amount: format!("{}", tx.amount),
                            fee: format!("{}", tx.fee),
                            status: format!("{}", tx.status),
                            cancelled: tx.cancelled,
                            message: tx.message,
                            time_stamp: tx.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                        },
                        Completed(tx) => TransactionDetailsResult {
                            txid: tx_id.to_string(),
                            source_pub_key: tx.source_public_key.to_hex(),
                            dest_pub_key: tx.destination_public_key.to_hex(),
                            direction: format!("{}", tx.direction),
                            amount: format!("{}", tx.amount),
                            fee: format!("{}", tx.fee),
                            status: format!("{}", tx.status),
                            cancelled: tx.cancelled,
                            message: tx.message,
                            time_stamp: tx.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                        },
                    }
                },
            },
            Err(err) => return Err(Status::unknown(err.to_string())),
        };

        Ok(Response::new(GetTransactionDetailsResponse {
            tx_details: Some(tx_status_result),
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
                Ok((dest.address, pk, dest.amount, dest.fee_per_gram, dest.message))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::invalid_argument)?;

        let transfers = recipients
            .into_iter()
            .map(|(address, pk, amount, fee_per_gram, message)| {
                let mut transaction_service = self.get_transaction_service();
                async move {
                    (
                        address,
                        transaction_service
                            .send_transaction(pk, amount.into(), fee_per_gram.into(), message)
                            .await,
                    )
                }
            });

        let results = future::join_all(transfers).await;

        let results = results
            .into_iter()
            .map(|(address, result)| match result {
                Ok(tx_id) => TransferResult {
                    address,
                    transaction_id: tx_id,
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
}
