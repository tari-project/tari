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
use tari_core::{
    base_node::rpc::query_service,
    chain_storage::BlockchainBackend,
    mempool::service::MempoolHandle,
    proto,
};
use tari_transaction_components::{rpc::models::TxSubmissionResponseV1, transaction_components::Transaction};

use crate::http::handler::ErrorResponse;

pub mod submit_transaction;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc";

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Extension(mempool_service): Extension<MempoolHandle>,
    Json(mut request): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received JSON-RPC request: {request:?}");

    match request.method.as_str() {
        "submit_transaction" => {
            // Take the parameter out of the request rather than cloning it; nothing reads it afterwards
            let tx = request
                .params
                .get_mut("transaction")
                .map(serde_json::Value::take)
                .ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::new("Missing transaction parameter".to_string())),
                    )
                })?;
            let transaction = decode_transaction(tx)?;
            // Version defaults to 1 for backward compatibility with older wallets.
            // V2 adds the optional `details` field in the response.
            let version: u64 = request.params.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
            match submit_transaction::handle(query_service.clone(), &mut (mempool_service.clone()), transaction).await {
                Ok(response) => {
                    let result = if version >= 2 {
                        serde_json::to_value(response).unwrap_or_else(|e| {
                            warn!(target: LOG_TARGET, "Failed to serialize response: {e}");
                            serde_json::Value::Null
                        })
                    } else {
                        serde_json::to_value(TxSubmissionResponseV1::from(response)).unwrap_or_else(|e| {
                            warn!(target: LOG_TARGET, "Failed to serialize V1 response: {e}");
                            serde_json::Value::Null
                        })
                    };
                    Ok(Json(JsonRpcResponse {
                        result,
                        error: None,
                        id: request.id,
                    }))
                },
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

/// Decodes the `transaction` parameter of a `submit_transaction` request.
///
/// The JSON value is decoded and then round-tripped through the protobuf representation used by the gRPC and P2P
/// entry points. That conversion is where the decode-time invariants of every transaction component are enforced,
/// so this entry point cannot hand the mempool a transaction that a peer could not have sent.
fn decode_transaction(value: serde_json::Value) -> Result<Transaction, (StatusCode, Json<ErrorResponse>)> {
    let bad_request = |e: String| (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(e)));
    let transaction = serde_json::from_value::<Transaction>(value).map_err(|e| bad_request(e.to_string()))?;
    let proto = proto::types::Transaction::try_from(transaction).map_err(bad_request)?;
    Transaction::try_from(proto).map_err(bad_request)
}

#[derive(Deserialize, Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Serialize, Debug)]

pub struct JsonRpcResponse {
    pub result: serde_json::Value,
    pub error: Option<String>,
    pub id: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use tari_common::configuration::Network;
    use tari_common_types::types::PrivateKey;
    use tari_core::{
        base_node::{StateMachineHandle, state_machine_service::states::StatusInfo},
        chain_storage::async_db::AsyncBlockchainDb,
        mempool::{TxStorageResponse, test_utils::mock::create_mempool_service_mock},
        test_helpers::blockchain::{TempDatabase, create_new_blockchain_with_network},
    };
    use tari_shutdown::Shutdown;
    use tari_transaction_components::transaction_components::{
        TransactionOutput,
        encrypted_data::STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
    };
    use tari_utilities::hex::to_hex;
    use tokio::sync::{broadcast, watch};

    use super::*;

    fn make_query_service(shutdown: &Shutdown, mempool: MempoolHandle) -> Arc<query_service::Service<TempDatabase>> {
        let db = AsyncBlockchainDb::from(create_new_blockchain_with_network(Network::LocalNet));
        let (state_tx, _state_rx) = broadcast::channel(10);
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        let state_machine = StateMachineHandle::new(state_tx, status_rx, shutdown.to_signal());
        Arc::new(query_service::Service::new(db, state_machine, mempool))
    }

    fn transaction_json(encrypted_data_hex: Option<String>) -> serde_json::Value {
        let tx = Transaction::new(
            vec![],
            vec![TransactionOutput::default()],
            vec![],
            PrivateKey::default(),
            PrivateKey::default(),
        );
        let mut json = serde_json::to_value(&tx).unwrap();
        if let Some(hex) = encrypted_data_hex {
            json["body"]["outputs"][0]["encrypted_data"]["data"] = serde_json::Value::String(hex);
        }
        json
    }

    fn request(transaction: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: "1".to_string(),
            method: "submit_transaction".to_string(),
            params: serde_json::json!({ "transaction": transaction }),
        }
    }

    #[tokio::test]
    async fn submit_transaction_rejects_short_encrypted_data_without_touching_the_mempool() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        let query_service = make_query_service(&shutdown, mempool.clone());

        let short = to_hex(&[1u8; STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1]);
        let tx = transaction_json(Some(short));
        // The JSON really does contain the short value
        assert_eq!(
            tx["body"]["outputs"][0]["encrypted_data"]["data"]
                .as_str()
                .unwrap()
                .len(),
            2 * (STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1)
        );

        let (status, Json(err)) = handle(Extension(query_service), Extension(mempool), Json(request(tx)))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let err = serde_json::to_string(&err).unwrap();
        assert!(err.contains("at least"), "{}", err);
        assert_eq!(mempool_state.get_call_count(), 0, "the mempool must not be called");
    }

    #[tokio::test]
    async fn submit_transaction_forwards_a_well_formed_transaction_to_the_mempool() {
        let shutdown = Shutdown::new();
        let (mempool, mempool_state) = create_mempool_service_mock();
        mempool_state
            .set_submit_transaction_response(TxStorageResponse::UnconfirmedPool)
            .await;
        let query_service = make_query_service(&shutdown, mempool.clone());

        let response = match handle(
            Extension(query_service),
            Extension(mempool),
            Json(request(transaction_json(None))),
        )
        .await
        {
            Ok(Json(response)) => response,
            Err((status, Json(err))) => panic!("{status}: {}", serde_json::to_string(&err).unwrap()),
        };
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(mempool_state.get_call_count(), 1);
    }
}
