use tari_app_grpc::tari_rpc::{wallet_server, GetCoinbaseRequest, GetCoinbaseResponse};
use tari_wallet::WalletSqlite;
use tonic::{Request, Response, Status};

pub struct WalletGrpcServer {
    wallet: WalletSqlite,
}

impl WalletGrpcServer {
    pub fn new(wallet: WalletSqlite) -> Self {
        Self { wallet }
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

        let mut tx_service = self.wallet.transaction_service.clone();
        let response = tx_service
            .generate_coinbase_transaction(request.reward.into(), request.fee.into(), request.height)
            .await;

        match response {
            Ok(resp) => Ok(Response::new(GetCoinbaseResponse {
                transaction: Some(resp.into()),
            })),
            Err(_err) => unimplemented!(),
        }
    }
}
