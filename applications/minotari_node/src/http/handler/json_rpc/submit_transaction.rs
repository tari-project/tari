use std::sync::Arc;

use log::{debug, error, warn};
use tari_core::{
    base_node::rpc::{models::TxSubmissionResponse, query_service},
    chain_storage::BlockchainBackend,
    mempool::service::MempoolHandle,
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc::submit_transaction";

pub async fn handle(
    mempool_service: &mut MempoolHandle,
    transaction: Transaction,
) -> Result<TxSubmissionResponse, anyhow::Error> {
    match mempool_service.submit_transaction(transaction).await {
        Ok(response) => {
            debug!(target: LOG_TARGET, "Transaction submitted successfully: {:?}", response);
            match response {}
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "Failed to submit transaction: {}", e);
            Ok(TxSubmissionResponse {
                accepted: false,
                rejection_reason: e.to_string(),
                is_synced: true,
            })
        },
    }
}
