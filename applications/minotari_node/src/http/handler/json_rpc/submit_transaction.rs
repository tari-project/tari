use std::sync::Arc;

use tari_core::{
    base_node::rpc::query_service,
    chain_storage::BlockchainBackend,
    transactions::transaction_components::Transaction,
};

pub async fn handle<B: BlockchainBackend + 'static>(
    query_service: Arc<query_service::Service<B>>,
    transaction: Transaction,
) -> Result<SubmitTransactionResponse, anyhow::Error> {
    todo!();
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SubmitTransactionResponse {}
