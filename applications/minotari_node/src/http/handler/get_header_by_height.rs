// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::debug;
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{models::BlockHeader, query_service, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};

use crate::http::handler::{error_handler_with_message, ErrorResponse};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_header_by_height";

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetHeaderByHeightQueryParams {
    pub height: u64,
}

#[utoipa::path(
    get,
    operation_id = "get_header_by_height",
    params(GetHeaderByHeightQueryParams),
    path = "/get_header_by_height",
    responses(
        (status = 200, description = "Block header returned successfully", body = BlockHeader),
        (status = NOT_FOUND, description = "Header not found", body = ErrorResponse, example = json!({"error": "Header not found at specified time"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetHeaderByHeightQueryParams>,
) -> Result<Json<BlockHeader>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_header_by_height request: {}", params.height);

    let response = query_service
        .get_header_by_height(params.height)
        .await
        .map_err(error_handler_with_message)?;
    Ok(Json(response))
}
