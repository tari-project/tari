use futures::future;
use log::*;
use tari_app_grpc::tari_rpc::{
    wallet_server,
    GetCoinbaseRequest,
    GetCoinbaseResponse,
    TransferRequest,
    TransferResponse,
    TransferResult,
};
use tari_comms::types::CommsPublicKey;
use tari_core::tari_utilities::hex::Hex;
use tari_wallet::{transaction_service::handle::TransactionServiceHandle, WalletSqlite};
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
