use futures::future;
use log::*;
use tari_app_grpc::{
    conversions::naive_datetime_to_timestamp,
    tari_rpc::{
        wallet_server,
        GetCoinbaseRequest,
        GetCoinbaseResponse,
        GetIdentityRequest,
        GetIdentityResponse,
        GetTransactionInfoRequest,
        GetTransactionInfoResponse,
        GetVersionRequest,
        GetVersionResponse,
        TransactionDirection,
        TransactionInfo,
        TransactionStatus,
        TransferRequest,
        TransferResponse,
        TransferResult,
    },
};
use tari_comms::types::CommsPublicKey;
use tari_core::tari_utilities::{hex::Hex, ByteArray};
use tari_wallet::{
    transaction_service::{handle::TransactionServiceHandle, storage::models},
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
    async fn get_version(&self, _: Request<GetVersionRequest>) -> Result<Response<GetVersionResponse>, Status> {
        Ok(Response::new(GetVersionResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }

    async fn identify(&self, request: Request<GetIdentityRequest>) -> Result<Response<GetIdentityResponse>, Status> {
        let _request = request.into_inner();

        let identity = self.wallet.comms.node_identity();
        Ok(Response::new(GetIdentityResponse {
            public_key: identity.public_key().to_string().as_bytes().to_vec(),
            public_address: identity.public_address().to_string(),
            node_id: identity.node_id().to_string().as_bytes().to_vec(),
        }))
    }

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

    async fn get_transaction_info(
        &self,
        request: Request<GetTransactionInfoRequest>,
    ) -> Result<Response<GetTransactionInfoResponse>, Status>
    {
        let message = request.into_inner();

        let queries = message.transaction_ids.into_iter().map(|tx_id| {
            let mut transaction_service = self.get_transaction_service();
            async move { transaction_service.get_any_transaction(tx_id).await }
        });

        let transactions = future::try_join_all(queries)
            .await
            .map_err(|err| Status::unknown(err.to_string()))
            .and_then(|transactions| {
                // If any of the transaction IDs are not known, this call fails
                if let Some(pos) = transactions.iter().position(Option::is_none) {
                    return Err(Status::not_found(format!(
                        "Transaction ID at position {} not found",
                        pos
                    )));
                }
                Ok(transactions.into_iter().map(Option::unwrap))
            })?;

        let wallet_pk = self.wallet.comms.node_identity_ref().public_key();

        let transactions = transactions
            .into_iter()
            .map(|tx| convert_wallet_transaction_into_transaction_info(tx, wallet_pk))
            .collect();

        Ok(Response::new(GetTransactionInfoResponse { transactions }))
    }
}

fn convert_wallet_transaction_into_transaction_info(
    tx: models::WalletTransaction,
    wallet_pk: &CommsPublicKey,
) -> TransactionInfo
{
    use models::WalletTransaction::*;
    match tx {
        PendingInbound(tx) => TransactionInfo {
            tx_id: tx.tx_id,
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
            tx_id: tx.tx_id,
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
            tx_id: tx.tx_id,
            source_pk: tx.source_public_key.to_vec(),
            dest_pk: tx.destination_public_key.to_vec(),
            status: TransactionStatus::from(tx.status) as i32,
            amount: tx.amount.into(),
            is_cancelled: tx.cancelled,
            direction: TransactionDirection::from(tx.direction) as i32,
            fee: tx.fee.into(),
            timestamp: Some(naive_datetime_to_timestamp(tx.timestamp)),
            excess_sig: tx
                .transaction
                .first_kernel_excess_sig()
                .expect("Complete transaction has no kernels")
                .get_signature()
                .to_vec(),
            message: tx.message,
        },
    }
}
