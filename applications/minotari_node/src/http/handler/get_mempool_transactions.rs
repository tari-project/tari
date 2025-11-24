// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension,
    Json,
};
use chrono::format::Fixed;
use log::debug;
use serde::{Deserialize, Serialize};
use tari_core::{base_node::rpc::query_service, chain_storage::BlockchainBackend, mempool::service::MempoolHandle};
use tari_transaction_components::transaction_components::{Transaction, TransactionOutput};
use tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray};
use tonic::service::AxumBody;
use utoipa::ToSchema;

use crate::{
    http::{
        cache_config::{apply_cache_control, RouteKey},
        handler::ErrorResponse,
    },
    HttpCacheConfig,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_mempool_transactions";

#[derive(Serialize, Deserialize)]
pub struct MempoolTransactionInfo {
    pub outputs: Vec<TransactionOutput>,
    pub input_hashes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct MempoolTransactionsResponse {
    pub transactions: Vec<MempoolTransactionInfo>,
}

#[utoipa::path(
    get,
    operation_id = "get_mempool_transactions",
    path = "/get_mempool_transactions",
    responses(
        (status = 200, description = "Mempool transactions returned successfully"),
        (status = INTERNAL_SERVER_ERROR, description = "Failed to get mempool transactions", body = ErrorResponse, example = json!({"error": "Failed to get mempool state"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Extension(mut mempool_service): Extension<MempoolHandle>,
    Extension(cache_cfg): Extension<Arc<HttpCacheConfig>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_mempool_transactions request...");

    let state = mempool_service.get_state().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("Failed to get mempool state: {}", e))),
        )
    })?;

    let transactions: Vec<Transaction> = state.unconfirmed_pool.into_iter().map(|tx| (*tx).clone()).collect();

    let response_txs = transactions
        .into_iter()
        .map(|tx| {
            let outputs = tx.body().outputs().clone();

            let input_hashes = tx
                .body()
                .inputs()
                .iter()
                .map(|input| input.output_hash().to_hex())
                .collect();

            MempoolTransactionInfo { outputs, input_hashes }
        })
        .collect();
    let response_data = MempoolTransactionsResponse {
        transactions: response_txs,
    };

    let body = Json(response_data);
    let mut response = body.into_response();
    apply_cache_control(response.headers_mut(), &cache_cfg, RouteKey::GetTipInfo, 0, 0);
    Ok(response)
}
