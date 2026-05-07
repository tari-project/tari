// Copyright 2025. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::Arc;

use log::{debug, error};
use tari_core::{
    base_node::rpc::{BaseNodeWalletQueryService, query_service},
    chain_storage::BlockchainBackend,
    mempool::{TxStorageResponse, TxStorageResponseWithRejectionReason, service::MempoolHandle},
};
use tari_transaction_components::{
    rpc::models::{TxSubmissionRejectionReason, TxSubmissionResponse},
    transaction_components::Transaction,
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
            error!(target: LOG_TARGET, "Failed to get tip info: {e}");
            anyhow::anyhow!("Failed to get tip info: {e}")
        })?
        .is_synced;
    let res = match mempool_service
        .submit_transaction_with_rejection_reason(transaction)
        .await
    {
        Ok(response) => {
            debug!(target: LOG_TARGET, "Transaction submitted successfully: {response:?}");
            tx_submission_response_from_mempool_response(response, is_synced)
        },
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to submit transaction: {e}"));
        },
    };
    Ok(res)
}

fn tx_submission_response_from_mempool_response(
    response: TxStorageResponseWithRejectionReason,
    is_synced: bool,
) -> TxSubmissionResponse {
    match response.storage_response {
        TxStorageResponse::UnconfirmedPool => TxSubmissionResponse {
            accepted: true,
            rejection_reason: TxSubmissionRejectionReason::None,
            rejection_reason_details: response.rejection_reason,
            is_synced,
        },
        TxStorageResponse::NotStoredOrphan => TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::Orphan,
            rejection_reason_details: response.rejection_reason,
            is_synced,
        },
        TxStorageResponse::NotStoredFeeTooLow => TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::FeeTooLow,
            rejection_reason_details: response.rejection_reason,
            is_synced,
        },
        TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::TimeLocked,
            rejection_reason_details: response.rejection_reason,
            is_synced,
        },
        TxStorageResponse::NotStoredConsensus | TxStorageResponse::NotStored => TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::ValidationFailed,
            rejection_reason_details: response.rejection_reason,
            is_synced,
        },
        TxStorageResponse::NotStoredAlreadySpent => TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::DoubleSpend,
            rejection_reason_details: response.rejection_reason,
            is_synced,
        },
        TxStorageResponse::ReorgPool | TxStorageResponse::NotStoredAlreadyMined => TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::AlreadyMined,
            rejection_reason_details: response.rejection_reason,
            is_synced,
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn includes_detailed_rejection_reason_in_http_response() {
        let detail = "Consensus rule violation: range proof verification failed".to_string();
        let response = tx_submission_response_from_mempool_response(
            TxStorageResponseWithRejectionReason::rejected(TxStorageResponse::NotStoredConsensus, detail.clone()),
            true,
        );

        assert!(!response.accepted);
        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::ValidationFailed);
        assert_eq!(response.rejection_reason_details, Some(detail));
        assert!(response.is_synced);
    }

    #[test]
    fn maps_already_spent_to_double_spend_for_http_wallet_feedback() {
        let response = tx_submission_response_from_mempool_response(
            TxStorageResponseWithRejectionReason::rejected(
                TxStorageResponse::NotStoredAlreadySpent,
                "Transaction contains input(s) that have already been spent".to_string(),
            ),
            true,
        );

        assert!(!response.accepted);
        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::DoubleSpend);
        assert_eq!(
            response.rejection_reason_details.as_deref(),
            Some("Transaction contains input(s) that have already been spent")
        );
    }
}
