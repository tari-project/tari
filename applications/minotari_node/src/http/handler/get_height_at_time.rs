// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::debug;
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{query_service, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};

use crate::http::handler::{error_handler_with_message, ErrorResponse};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_height_at_time";

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetHeightAtTimeQueryParams {
    pub time: u64,
}

#[utoipa::path(
    get,
    operation_id = "get_height_at_time",
    params(GetHeightAtTimeQueryParams),
    path = "/get_height_at_time",
    responses(
        (status = 200, description = "Height at specific time returned successfully", body = u64),
        (status = NOT_FOUND, description = "Header not found", body = ErrorResponse, example = json!({"error": "Header not found at time: 10"})),
        (status = INTERNAL_SERVER_ERROR, description = "Failed to get chain metadata", body = ErrorResponse, example = json!({"error": "Failed to get chain metadata: chain storage error"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetHeightAtTimeQueryParams>,
) -> Result<Json<u64>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_height_at_time request: {}", params.time);

    let response = query_service
        .get_height_at_time(params.time)
        .await
        .map_err(error_handler_with_message)?;
    Ok(Json(response))
}
