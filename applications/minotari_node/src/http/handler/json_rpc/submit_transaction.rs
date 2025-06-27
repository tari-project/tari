use std::sync::Arc;

use log::{debug, error, warn};
use tari_core::{
    base_node::rpc::{
        models::{TxSubmissionRejectionReason, TxSubmissionResponse},
        query_service,
        BaseNodeWalletQueryService,
    },
    chain_storage::BlockchainBackend,
    mempool::{service::MempoolHandle, TxStorageResponse},
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc::submit_transaction";

pub async fn handle<T: BlockchainBackend + 'static>(
    query_service: Arc<query_service::Service<T>>,
    mempool_service: &mut MempoolHandle,
    transaction: Transaction,
) -> Result<TxSubmissionResponse, anyhow::Error> {
    let is_synced = query_service
        .get_tip_info()
        .await
        .map_err(|e| {
            error!(target: LOG_TARGET, "Failed to get tip info: {}", e);
            anyhow::anyhow!("Failed to get tip info: {}", e)
        })?
        .is_synced;
    let res = match mempool_service.submit_transaction(transaction).await {
        Ok(response) => {
            debug!(target: LOG_TARGET, "Transaction submitted successfully: {:?}", response);
            match response {
                TxStorageResponse::UnconfirmedPool => TxSubmissionResponse {
                    accepted: true,
                    rejection_reason: TxSubmissionRejectionReason::None.into(),
                    is_synced,
                },

                TxStorageResponse::NotStoredOrphan => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::Orphan.into(),
                    is_synced,
                },
                TxStorageResponse::NotStoredFeeTooLow => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::FeeTooLow.into(),
                    is_synced,
                },
                TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::TimeLocked.into(),
                    is_synced,
                },
                TxStorageResponse::NotStoredConsensus | TxStorageResponse::NotStored => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::ValidationFailed.into(),
                    is_synced,
                },
                TxStorageResponse::NotStoredAlreadySpent |
                TxStorageResponse::ReorgPool |
                TxStorageResponse::NotStoredAlreadyMined => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::AlreadyMined.into(),
                    is_synced,
                },
            }
        },
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to submit transaction: {}", e));
        },
    };
    Ok(res)
}
