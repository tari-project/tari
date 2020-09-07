use std::sync::Arc;
use tari_app_grpc::tari_rpc::{wallet_server, GetCoinbaseRequest, GetCoinbaseResponse};
use tari_wallet::{
    contacts_service::storage::sqlite_db::ContactsServiceSqliteDatabase,
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::sqlite_db::WalletSqliteDatabase,
    transaction_service::storage::sqlite_db::TransactionServiceSqliteDatabase,
    Wallet,
};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

pub struct WalletGrpcServer {
    wallet: Arc<
        RwLock<
            Wallet<
                WalletSqliteDatabase,
                TransactionServiceSqliteDatabase,
                OutputManagerSqliteDatabase,
                ContactsServiceSqliteDatabase,
            >,
        >,
    >,
}

impl WalletGrpcServer {
    pub fn new(
        wallet: Arc<
            RwLock<
                Wallet<
                    WalletSqliteDatabase,
                    TransactionServiceSqliteDatabase,
                    OutputManagerSqliteDatabase,
                    ContactsServiceSqliteDatabase,
                >,
            >,
        >,
    ) -> Self
    {
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
        let response = self
            .wallet
            .write()
            .await
            .transaction_service
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
