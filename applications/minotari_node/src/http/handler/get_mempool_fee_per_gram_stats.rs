// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use log::debug;
use serde::{Deserialize, Serialize};
use tari_core::{
    base_node::rpc::{BaseNodeWalletQueryService, query_service},
    chain_storage::BlockchainBackend,
};
use tonic::service::AxumBody;
use utoipa::ToSchema;

use crate::http::handler::{ErrorResponse, error_handler_with_message};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_mempool_fee_per_gram_stats";

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetMempoolFeePerGramStatsQueryParams {
    pub count: u64,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct FeePerGramStatResponse {
    pub order: u64,
    pub min_fee_per_gram: u64,
    pub avg_fee_per_gram: u64,
    pub max_fee_per_gram: u64,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetMempoolFeePerGramStatsResponse {
    pub stats: Vec<FeePerGramStatResponse>,
}

#[utoipa::path(
    get,
    operation_id = "get_mempool_fee_per_gram_stats",
    params(GetMempoolFeePerGramStatsQueryParams),
    path = "/get_mempool_fee_per_gram_stats",
    responses(
        (status = 200, description = "Mempool fee per gram stats returned successfully", body = GetMempoolFeePerGramStatsResponse),
        (status = BAD_REQUEST, description = "Invalid count parameter", body = ErrorResponse, example = json!({"error": "count must be less than or equal to 20"})),
        (status = INTERNAL_SERVER_ERROR, description = "Failed to get mempool stats", body = ErrorResponse, example = json!({"error": "Internal server error"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetMempoolFeePerGramStatsQueryParams>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_mempool_fee_per_gram_stats request: count={}", params.count);

    let count = usize::try_from(params.count).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("count must be less than or equal to 20".to_string())),
        )
    })?;

    let stats = query_service
        .get_mempool_fee_per_gram_stats(count)
        .await
        .map_err(error_handler_with_message)?;

    let response = GetMempoolFeePerGramStatsResponse {
        stats: stats
            .into_iter()
            .map(|s| FeePerGramStatResponse {
                order: s.order,
                min_fee_per_gram: s.min_fee_per_gram.as_u64(),
                avg_fee_per_gram: s.avg_fee_per_gram.as_u64(),
                max_fee_per_gram: s.max_fee_per_gram.as_u64(),
            })
            .collect(),
    };

    let body = Json(response);
    Ok(body.into_response())
}
