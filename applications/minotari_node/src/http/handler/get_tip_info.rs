// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension,
    Json,
};
use log::debug;
use tari_core::{
    base_node::rpc::{models::TipInfoResponse, query_service, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};
use tonic::service::AxumBody;

use crate::http::handler::{error_handler_with_message, ErrorResponse};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_tip_info";

#[utoipa::path(
    get,
    operation_id = "get_tip_info",
    path = "/get_tip_info",
    responses(
        (status = 200, description = "Tip info returned successfully", body = TipInfoResponse),
        (status = INTERNAL_SERVER_ERROR, description = "Failed to get chain metadata", body = ErrorResponse, example = json!({"error": "Failed to get chain metadata: chain storage error"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_tip_info request...");

    let tip_info = query_service.get_tip_info().await.map_err(error_handler_with_message)?;
    let body = Json(tip_info);
    let mut response = body.into_response();
    response.headers_mut().insert(
        "Cache-Control",
        "public, max-age=15, s-maxage=15, stale-while-revalidate=15"
            .parse()
            .expect("should be a valid header value"),
    );
    Ok(response)
}
