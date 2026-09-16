// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use log::debug;
use tari_core::{
    base_node::rpc::{BaseNodeWalletQueryService, query_service},
    chain_storage::BlockchainBackend,
};
use tari_transaction_components::rpc::models::TipInfoResponse;
use tonic::service::AxumBody;

use crate::{
    HttpCacheConfig,
    http::{
        cache_config::{RouteKey, apply_cache_control},
        handler::{ErrorResponse, error_handler_with_message},
    },
};

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
    Extension(cache_cfg): Extension<Arc<HttpCacheConfig>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_tip_info request...");

    let tip_info = query_service.get_tip_info().await.map_err(error_handler_with_message)?;
    let body = Json(tip_info);
    let mut response = body.into_response();
    apply_cache_control(response.headers_mut(), &cache_cfg, RouteKey::GetTipInfo, 0, 0);
    Ok(response)
}
