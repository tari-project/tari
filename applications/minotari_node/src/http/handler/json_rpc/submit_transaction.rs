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
    mempool::{TxStorageResponse, service::MempoolHandle},
};
use tari_transaction_components::{
    rpc::models::{TxSubmissionRejectionReason, TxSubmissionResponse},
    transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc::submit_transaction";

#[cfg(test)]
mod tests {
    use tari_core::mempool::TxStorageResponse;
    use tari_transaction_components::rpc::models::{TxSubmissionRejectionReason, TxSubmissionResponse};

    fn make_response(storage: TxStorageResponse) -> TxSubmissionResponse {
        let is_synced = true;
        match storage {
            TxStorageResponse::UnconfirmedPool => TxSubmissionResponse {
                accepted: true,
                rejection_reason: TxSubmissionRejectionReason::None,
                is_synced,
                rejection_detail: None,
            },
            TxStorageResponse::NotStoredOrphan => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::Orphan,
                is_synced,
                rejection_detail: None,
            },
            TxStorageResponse::NotStoredFeeTooLow => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::FeeTooLow,
                is_synced,
                rejection_detail: None,
            },
            TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::TimeLocked,
                is_synced,
                rejection_detail: None,
            },
            TxStorageResponse::NotStoredConsensus(detail) | TxStorageResponse::NotStored(detail) => {
                TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::ValidationFailed,
                    is_synced,
                    rejection_detail: detail,
                }
            },
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredAlreadyMined => TxSubmissionResponse {
                accepted: false,
                rejection_reason: TxSubmissionRejectionReason::AlreadyMined,
                is_synced,
                rejection_detail: None,
            },
        }
    }

    #[test]
    fn test_rejection_detail_propagated_for_consensus_failure() {
        let reason = "double-spend of commitment abc123".to_string();
        let storage = TxStorageResponse::NotStoredConsensus(Some(reason.clone()));
        let resp = make_response(storage);
        assert!(!resp.accepted);
        assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::ValidationFailed);
        assert_eq!(resp.rejection_detail, Some(reason));
    }

    #[test]
    fn test_rejection_detail_propagated_for_generic_failure() {
        let reason = "invalid range proof".to_string();
        let storage = TxStorageResponse::NotStored(Some(reason.clone()));
        let resp = make_response(storage);
        assert!(!resp.accepted);
        assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::ValidationFailed);
        assert_eq!(resp.rejection_detail, Some(reason));
    }

    #[test]
    fn test_rejection_detail_absent_when_no_detail() {
        let storage = TxStorageResponse::NotStored(None);
        let resp = make_response(storage);
        assert!(!resp.accepted);
        assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::ValidationFailed);
        assert!(resp.rejection_detail.is_none());
    }

    #[test]
    fn test_non_validation_failures_have_no_detail() {
        for storage in [
            TxStorageResponse::NotStoredOrphan,
            TxStorageResponse::NotStoredFeeTooLow,
            TxStorageResponse::NotStoredTimeLocked,
            TxStorageResponse::NotStoredAlreadyMined,
        ] {
            let resp = make_response(storage);
            assert!(resp.rejection_detail.is_none(), "Expected no detail for {:?}", resp.rejection_reason);
        }
    }

    #[test]
    fn test_backward_compat_serialization_omits_null_detail() {
        let resp = TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::FeeTooLow,
            is_synced: true,
            rejection_detail: None,
        };
        let json = serde_json::to_string(&resp).expect("serialization failed");
        // rejection_detail must NOT appear in the JSON when None (backward compat)
        assert!(!json.contains("rejection_detail"), "JSON must not contain rejection_detail when None: {json}");
    }

    #[test]
    fn test_serialization_includes_detail_when_present() {
        let resp = TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::ValidationFailed,
            is_synced: true,
            rejection_detail: Some("insufficient fee: got 10, need 20".to_string()),
        };
        let json = serde_json::to_string(&resp).expect("serialization failed");
        assert!(json.contains("rejection_detail"), "JSON must contain rejection_detail when Some: {json}");
        assert!(json.contains("insufficient fee"), "JSON must contain the detail message: {json}");
    }
}

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
    let res = match mempool_service.submit_transaction(transaction).await {
        Ok(response) => {
            debug!(target: LOG_TARGET, "Transaction submitted successfully: {response:?}");
            match response {
                TxStorageResponse::UnconfirmedPool => TxSubmissionResponse {
                    accepted: true,
                    rejection_reason: TxSubmissionRejectionReason::None,
                    is_synced,
                    rejection_detail: None,
                },

                TxStorageResponse::NotStoredOrphan => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::Orphan,
                    is_synced,
                    rejection_detail: None,
                },
                TxStorageResponse::NotStoredFeeTooLow => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::FeeTooLow,
                    is_synced,
                    rejection_detail: None,
                },
                TxStorageResponse::NotStoredTimeLocked => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::TimeLocked,
                    is_synced,
                    rejection_detail: None,
                },
                TxStorageResponse::NotStoredConsensus(detail) |
                TxStorageResponse::NotStored(detail) => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::ValidationFailed,
                    is_synced,
                    rejection_detail: detail,
                },
                TxStorageResponse::NotStoredAlreadySpent |
                TxStorageResponse::ReorgPool |
                TxStorageResponse::NotStoredAlreadyMined => TxSubmissionResponse {
                    accepted: false,
                    rejection_reason: TxSubmissionRejectionReason::AlreadyMined,
                    is_synced,
                    rejection_detail: None,
                },
            }
        },
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to submit transaction: {e}"));
        },
    };
    Ok(res)
}
