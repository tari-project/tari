use futures::future;
use log::*;
use tari_app_grpc::{
    conversions::naive_datetime_to_timestamp,
    tari_rpc::{
        payment_recipient::PaymentType,
        wallet_server,
        CoinSplitRequest,
        CoinSplitResponse,
        GetBalanceRequest,
        GetBalanceResponse,
        GetCoinbaseRequest,
        GetCoinbaseResponse,
        GetCompletedTransactionsRequest,
        GetCompletedTransactionsResponse,
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
use tari_core::{
    tari_utilities::{hex::Hex, ByteArray},
    transactions::tari_amount::MicroTari,
};
use tari_wallet::{
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{handle::TransactionServiceHandle, storage::models},
    WalletSqlite,
};
use tokio::{sync::mpsc, task};
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
}

#[tonic::async_trait]
impl wallet_server::Wallet for WalletGrpcServer {
    type GetCompletedTransactionsStream = mpsc::Receiver<Result<GetCompletedTransactionsResponse, Status>>;

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
    ) -> Result<Response<Self::GetCompletedTransactionsStream>, Status>
    {
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
                        tx_id: txn.tx_id,
                        source_pk: txn.source_public_key.to_vec(),
                        dest_pk: txn.destination_public_key.to_vec(),
                        status: TransactionStatus::from(txn.status) as i32,
                        amount: txn.amount.into(),
                        is_cancelled: txn.cancelled,
                        direction: TransactionDirection::from(txn.direction) as i32,
                        fee: txn.fee.into(),
                        timestamp: Some(naive_datetime_to_timestamp(txn.timestamp)),
                        excess_sig: txn
                            .transaction
                            .first_kernel_excess_sig()
                            .expect("Complete transaction has no kernels")
                            .get_signature()
                            .to_vec(),
                        message: txn.message,
                        valid: txn.valid,
                        is_found: true,
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

        Ok(Response::new(CoinSplitResponse { tx_id }))
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
            valid: true,
            is_found: true,
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
            valid: true,
            is_found: true,
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
                .map(|s| s.get_signature().to_vec())
                .unwrap_or_default(),
            message: tx.message,
            valid: tx.valid,
            is_found: true,
        },
    }
}
