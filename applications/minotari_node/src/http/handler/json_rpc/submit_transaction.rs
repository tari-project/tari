use std::sync::Arc;

use tari_core::{
    base_node::rpc::query_service,
    chain_storage::BlockchainBackend,
    mempool::service::MempoolHandle,
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc::submit_transaction";

pub async fn handle(
    mempool_service: &mut MempoolHandle,
    transaction: Transaction,
) -> Result<SubmitTransactionResponse, anyhow::Error> {
    let res = mempool_service.submit_transaction(transaction).await?;

    Ok(SubmitTransactionResponse {
        // Assuming the response contains some relevant data, adjust as necessary
    })
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SubmitTransactionResponse {}
