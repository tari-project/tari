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

use axum::{Extension, Json, http::StatusCode};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use tari_core::{base_node::rpc::query_service, chain_storage::BlockchainBackend, mempool::service::MempoolHandle};
use tari_transaction_components::{
    rpc::models::{TxSubmissionResponse, TxSubmissionResponseV1},
    transaction_components::Transaction,
};

use crate::http::handler::ErrorResponse;

pub mod submit_transaction;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc";

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Extension(mempool_service): Extension<MempoolHandle>,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received JSON-RPC request: {request:?}");

    match request.method.as_str() {
        "submit_transaction" => {
            let submit_request = serde_json::from_value::<SubmitTransactionRequest>(request.params.clone())
                .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(e.to_string()))))?;
            let SubmitTransactionRequest { transaction, version } = submit_request;
            match submit_transaction::handle(query_service.clone(), &mut (mempool_service.clone()), transaction).await {
                Ok(response) => Ok(Json(JsonRpcResponse {
                    result: submit_transaction_response_value(response, version).unwrap_or_else(|e| {
                        warn!(target: LOG_TARGET, "Failed to serialize response: {e}");
                        serde_json::Value::Null
                    }),
                    error: None,
                    id: request.id,
                })),
                Err(e) => {
                    debug!(target: LOG_TARGET, "Error submitting transaction: {e}");

                    Ok(Json(JsonRpcResponse {
                        result: serde_json::Value::Null,
                        error: Some(e.to_string()),
                        id: request.id,
                    }))
                },
            }
        },
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Method not found".to_string())),
        )),
    }
}

fn submit_transaction_response_value(
    response: TxSubmissionResponseV1,
    version: u64,
) -> Result<serde_json::Value, serde_json::Error> {
    match version {
        0 => serde_json::to_value(TxSubmissionResponse::from(response)),
        _ => serde_json::to_value(response),
    }
}

#[derive(Deserialize, Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct SubmitTransactionRequest {
    transaction: Transaction,
    #[serde(default)]
    version: u64,
}

#[derive(Serialize, Debug)]

pub struct JsonRpcResponse {
    pub result: serde_json::Value,
    pub error: Option<String>,
    pub id: String,
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_transaction_components::rpc::models::TxSubmissionRejectionReason;

    fn response_with_details() -> TxSubmissionResponseV1 {
        TxSubmissionResponseV1 {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::ValidationFailed,
            rejection_reason_details: Some("specific validation error".to_string()),
            is_synced: true,
        }
    }

    #[test]
    fn submit_transaction_response_version_zero_omits_details() {
        let value = submit_transaction_response_value(response_with_details(), 0).unwrap();

        assert!(value.get("rejection_reason_details").is_none());
    }

    #[test]
    fn submit_transaction_response_version_one_includes_details() {
        let value = submit_transaction_response_value(response_with_details(), 1).unwrap();

        assert_eq!(value["rejection_reason_details"], "specific validation error");
    }
}
