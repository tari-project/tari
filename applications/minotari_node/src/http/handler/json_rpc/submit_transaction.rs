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
use tari_utilities::{ByteArray, hex::Hex};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc::submit_transaction";
const MAX_TRANSACTION_SUMMARY_ITEMS: usize = 10;

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
    let res = match mempool_service.submit_transaction(transaction.clone()).await {
        Ok(response) => {
            debug!(target: LOG_TARGET, "Transaction submitted successfully: {response:?}");
            let mined_location = match response {
                TxStorageResponse::NotStoredAlreadySpent |
                TxStorageResponse::ReorgPool |
                TxStorageResponse::NotStoredAlreadyMined => transaction_location(query_service.as_ref(), &transaction)
                    .await
                    .map_err(|e| {
                        error!(target: LOG_TARGET, "Failed to query transaction location: {e}");
                        anyhow::anyhow!("Failed to query transaction location: {e}")
                    })?,
                _ => TxLocation::None,
            };
            build_response(response, &transaction, is_synced, mined_location)
        },
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to submit transaction: {e}"));
        },
    };
    Ok(res)
}

async fn transaction_location<T: BlockchainBackend + 'static>(
    query_service: &query_service::Service<T>,
    transaction: &Transaction,
) -> Result<TxLocation, query_service::Error> {
    let Some(signature) = transaction.first_kernel_excess_sig() else {
        return Ok(TxLocation::None);
    };

    let response = query_service
        .transaction_query(Signature {
            public_nonce: signature.get_compressed_public_nonce().as_bytes().to_vec(),
            signature: signature.get_signature().as_bytes().to_vec(),
        })
        .await?;

    Ok(response.location)
}

fn build_response(
    response: TxStorageResponse,
    transaction: &Transaction,
    is_synced: bool,
    mined_location: TxLocation,
) -> TxSubmissionResponse {
    let (accepted, rejection_reason, rejection_details) = match response {
        TxStorageResponse::UnconfirmedPool => (true, TxSubmissionRejectionReason::None, None),
        TxStorageResponse::NotStoredOrphan => (
            false,
            TxSubmissionRejectionReason::Orphan,
            Some(format!(
                "Orphan transaction: one or more inputs are unknown to the base node and are not present in the \
                 mempool. {}",
                transaction_inputs(transaction)
            )),
        ),
        TxStorageResponse::NotStoredFeeTooLow => (
            false,
            TxSubmissionRejectionReason::FeeTooLow,
            Some(format!(
                "Transaction fee {} is below the minimum accepted by this mempool.",
                transaction
                    .body
                    .get_total_fee()
                    .map(|fee| fee.as_u64().to_string())
                    .unwrap_or_else(|e| format!("could not be calculated ({e})"))
            )),
        ),
        TxStorageResponse::NotStoredTimeLocked => (
            false,
            TxSubmissionRejectionReason::TimeLocked,
            Some(format!(
                "Transaction is time locked or spends inputs that are not mature yet. max_kernel_timelock={}, \
                 min_spendable_height={}.",
                transaction.max_kernel_timelock(),
                transaction
                    .min_spendable_height()
                    .map(|height| height.to_string())
                    .unwrap_or_else(|e| format!("unknown ({e})"))
            )),
        ),
        TxStorageResponse::NotStoredWithReason(reason) => (
            false,
            TxSubmissionRejectionReason::ValidationFailed,
            Some(format!("{reason}. {}", transaction_summary(transaction))),
        ),
        TxStorageResponse::NotStoredConsensus | TxStorageResponse::NotStored => (
            false,
            TxSubmissionRejectionReason::ValidationFailed,
            Some(format!(
                "Mempool validation failed for this transaction. {}",
                transaction_summary(transaction)
            )),
        ),
        TxStorageResponse::NotStoredAlreadyMined => (
            false,
            TxSubmissionRejectionReason::AlreadyMined,
            Some(format!(
                "Transaction kernel was already mined. {}",
                transaction_kernel(transaction)
            )),
        ),
        TxStorageResponse::NotStoredAlreadySpent | TxStorageResponse::ReorgPool => match mined_location {
            TxLocation::Mined => (
                false,
                TxSubmissionRejectionReason::AlreadyMined,
                Some(format!(
                    "Transaction kernel was already mined. {}",
                    transaction_kernel(transaction)
                )),
            ),
            _ => (
                false,
                TxSubmissionRejectionReason::DoubleSpend,
                Some(format!(
                    "Transaction double-spends at least one input already spent on-chain or by another mempool \
                     transaction. {}",
                    transaction_inputs(transaction)
                )),
            ),
        },
    };

    TxSubmissionResponse {
        accepted,
        rejection_reason,
        rejection_details,
        is_synced,
    }
}

fn transaction_summary(transaction: &Transaction) -> String {
    format!(
        "{} {} {}",
        transaction_inputs(transaction),
        transaction_outputs(transaction),
        transaction_kernel(transaction)
    )
}

fn transaction_inputs(transaction: &Transaction) -> String {
    let inputs_len = transaction.body.inputs().len();
    let inputs = transaction
        .body
        .inputs()
        .iter()
        .take(MAX_TRANSACTION_SUMMARY_ITEMS)
        .map(|input| {
            let commitment = input
                .commitment()
                .map(|commitment| format!(", commitment={}", commitment.to_hex()))
                .unwrap_or_default();
            format!("output_hash={}{}", input.output_hash().to_hex(), commitment)
        })
        .collect::<Vec<_>>()
        .join("; ");

    if inputs_len > MAX_TRANSACTION_SUMMARY_ITEMS {
        format!("inputs[{}]=[{}; ...]", inputs_len, inputs)
    } else {
        format!("inputs[{}]=[{}]", inputs_len, inputs)
    }
}

fn transaction_outputs(transaction: &Transaction) -> String {
    let outputs_len = transaction.body.outputs().len();
    let outputs = transaction
        .body
        .outputs()
        .iter()
        .take(MAX_TRANSACTION_SUMMARY_ITEMS)
        .map(|output| format!("hash={}, commitment={}", output.hash().to_hex(), output.commitment.to_hex()))
        .collect::<Vec<_>>()
        .join("; ");

    if outputs_len > MAX_TRANSACTION_SUMMARY_ITEMS {
        format!("outputs[{}]=[{}; ...]", outputs_len, outputs)
    } else {
        format!("outputs[{}]=[{}]", outputs_len, outputs)
    }
}

fn transaction_kernel(transaction: &Transaction) -> String {
    transaction.first_kernel_excess_sig().map_or_else(
        || "kernel_signature=<none>".to_string(),
        |signature| format!("kernel_signature={}", signature.get_signature().to_hex()),
    )
}

#[cfg(test)]
mod tests {
    use tari_transaction_components::transaction_components::Transaction;

    use super::*;

    fn empty_transaction() -> Transaction {
        Transaction::new(vec![], vec![], vec![], Default::default(), Default::default())
    }

    #[test]
    fn response_includes_mempool_validation_detail() {
        let response = build_response(
            TxStorageResponse::NotStoredWithReason("Invalid range proof for output commitment abc".to_string()),
            &empty_transaction(),
            true,
            TxLocation::None,
        );

        assert!(!response.accepted);
        assert_eq!(response.rejection_reason, TxSubmissionRejectionReason::ValidationFailed);
        assert!(response.rejection_details.unwrap().contains("Invalid range proof"));
    }

    #[test]
    fn already_spent_response_distinguishes_double_spend_from_already_mined() {
        let transaction = empty_transaction();

        let double_spend = build_response(
            TxStorageResponse::NotStoredAlreadySpent,
            &transaction,
            true,
            TxLocation::NotStored,
        );
        let already_mined =
            build_response(TxStorageResponse::NotStoredAlreadySpent, &transaction, true, TxLocation::Mined);

        assert_eq!(double_spend.rejection_reason, TxSubmissionRejectionReason::DoubleSpend);
        assert_eq!(already_mined.rejection_reason, TxSubmissionRejectionReason::AlreadyMined);
    }
}
