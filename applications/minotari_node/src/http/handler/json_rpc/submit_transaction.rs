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
    rpc::models::{Signature, TxLocation, TxSubmissionRejectionReason, TxSubmissionResponse},
    transaction_components::Transaction,
};
use tari_utilities::ByteArray;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc::submit_transaction";

pub async fn handle<T: BlockchainBackend + 'static>(
    query_service: Arc<query_service::Service<T>>,
    mempool_service: &mut MempoolHandle,
    transaction: Transaction,
) -> Result<TxSubmissionResponse, anyhow::Error> {
    let transaction_signature = transaction.first_kernel_excess_sig().map(|excess_sig| Signature {
        public_nonce: excess_sig.get_compressed_public_nonce().as_bytes().to_vec(),
        signature: excess_sig.get_signature().as_bytes().to_vec(),
    });

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
                TxStorageResponse::UnconfirmedPool => {
                    submission_response(true, TxSubmissionRejectionReason::None, is_synced, None)
                },

                TxStorageResponse::NotStoredOrphan => submission_response(
                    false,
                    TxSubmissionRejectionReason::Orphan,
                    is_synced,
                    Some("Transaction refers to inputs that are not available to this node."),
                ),
                TxStorageResponse::NotStoredFeeTooLow => submission_response(
                    false,
                    TxSubmissionRejectionReason::FeeTooLow,
                    is_synced,
                    Some("Transaction fee is below the minimum fee per gram accepted by this mempool."),
                ),
                TxStorageResponse::NotStoredTimeLocked => submission_response(
                    false,
                    TxSubmissionRejectionReason::TimeLocked,
                    is_synced,
                    Some("Transaction is timelocked and cannot be accepted at the current chain height."),
                ),
                TxStorageResponse::NotStoredConsensus => submission_response(
                    false,
                    TxSubmissionRejectionReason::ValidationFailed,
                    is_synced,
                    Some("Transaction failed consensus validation rules."),
                ),
                TxStorageResponse::NotStored => submission_response(
                    false,
                    TxSubmissionRejectionReason::ValidationFailed,
                    is_synced,
                    Some("Transaction was not stored by the mempool."),
                ),
                response @ (TxStorageResponse::NotStoredAlreadySpent
                | TxStorageResponse::ReorgPool
                | TxStorageResponse::NotStoredAlreadyMined) => {
                    already_spent_or_mined_response(query_service.as_ref(), transaction_signature, is_synced, response)
                        .await
                },
            }
        },
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to submit transaction: {e}"));
        },
    };
    Ok(res)
}

fn submission_response(
    accepted: bool,
    rejection_reason: TxSubmissionRejectionReason,
    is_synced: bool,
    rejection_reason_details: Option<&str>,
) -> TxSubmissionResponse {
    TxSubmissionResponse {
        accepted,
        rejection_reason,
        rejection_reason_details: rejection_reason_details.map(ToString::to_string),
        is_synced,
    }
}

async fn already_spent_or_mined_response<T: BlockchainBackend + 'static>(
    query_service: &query_service::Service<T>,
    signature: Option<Signature>,
    is_synced: bool,
    tx_storage_response: TxStorageResponse,
) -> TxSubmissionResponse {
    let Some(signature) = signature else {
        return default_already_spent_or_mined_response(tx_storage_response, is_synced);
    };

    let response = match query_service.transaction_query(signature).await {
        Ok(response) => response,
        Err(e) => {
            error!(target: LOG_TARGET, "Failed to query submitted transaction kernel: {e}");
            return default_already_spent_or_mined_response(tx_storage_response, is_synced);
        },
    };

    match response.location {
        TxLocation::Mined => submission_response(
            false,
            TxSubmissionRejectionReason::AlreadyMined,
            is_synced,
            Some("Transaction kernel was found in the blockchain; this exact transaction has already been mined."),
        ),
        TxLocation::InMempool => submission_response(
            false,
            TxSubmissionRejectionReason::DoubleSpend,
            is_synced,
            Some("Transaction conflicts with an existing transaction already in the mempool."),
        ),
        TxLocation::None | TxLocation::NotStored => submission_response(
            false,
            TxSubmissionRejectionReason::DoubleSpend,
            is_synced,
            Some("Transaction spends an output that is already spent or conflicts with another transaction."),
        ),
    }
}

fn default_already_spent_or_mined_response(
    tx_storage_response: TxStorageResponse,
    is_synced: bool,
) -> TxSubmissionResponse {
    match tx_storage_response {
        TxStorageResponse::NotStoredAlreadySpent => submission_response(
            false,
            TxSubmissionRejectionReason::DoubleSpend,
            is_synced,
            Some("Transaction spends an output that is already spent or conflicts with another transaction."),
        ),
        TxStorageResponse::ReorgPool | TxStorageResponse::NotStoredAlreadyMined => submission_response(
            false,
            TxSubmissionRejectionReason::AlreadyMined,
            is_synced,
            Some("Transaction was rejected because it was already mined or is currently in the reorg pool."),
        ),
        _ => submission_response(
            false,
            TxSubmissionRejectionReason::AlreadyMined,
            is_synced,
            Some("Transaction was rejected because its outputs were already spent or it was already mined."),
        ),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn submission_response_omits_details_for_accepted_transaction() {
        let response = submission_response(true, TxSubmissionRejectionReason::None, true, None);

        assert!(response.accepted);
        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::None);
        assert!(response.rejection_reason_details.is_none());
    }

    #[test]
    fn submission_response_includes_rejection_details() {
        let response = submission_response(
            false,
            TxSubmissionRejectionReason::ValidationFailed,
            true,
            Some("Transaction failed consensus validation rules."),
        );

        assert!(!response.accepted);
        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::ValidationFailed);
        assert_eq!(
            response.rejection_reason_details.as_deref(),
            Some("Transaction failed consensus validation rules.")
        );
    }

    #[test]
    fn default_already_spent_response_returns_double_spend() {
        let response = default_already_spent_or_mined_response(TxStorageResponse::NotStoredAlreadySpent, true);

        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::DoubleSpend);
    }

    #[test]
    fn default_already_mined_response_returns_already_mined() {
        let response = default_already_spent_or_mined_response(TxStorageResponse::NotStoredAlreadyMined, true);

        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::AlreadyMined);
    }
}
