use std::sync::Arc;

use axum::{http::StatusCode, Extension, Json};
use log::debug;
use serde::{Deserialize, Serialize};
use tari_core::{
    base_node::rpc::query_service,
    chain_storage::BlockchainBackend,
    mempool::{service::MempoolHandle, Mempool},
};

use crate::http::handler::ErrorResponse;

pub mod submit_transaction;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::json_rpc";

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(mempool_service): Extension<MempoolHandle>,
    Json(params): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<ErrorResponse>)> {
    let request: JsonRpcRequest = params.into();
    debug!(target: LOG_TARGET, "Received JSON-RPC request: {request:?}");

    // let response = query_service
    //     .get_utxos_deleted_info(request)
    //     .await
    //     .map_err(error_handler_with_message)?;

    match request.method.as_str() {
        "submit_transaction" => {
            let tx = request.params.get("transaction").ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new("Missing transaction parameter".to_string())),
                )
            })?;
            let transaction = serde_json::from_value(tx.clone())
                .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(e.to_string()))))?;
            match submit_transaction::handle(&mut (mempool_service.clone()), transaction).await {
                Ok(response) => Ok(Json(JsonRpcResponse {
                    result: serde_json::to_value(response).unwrap_or_default(),
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
