// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{http::StatusCode, Extension, Json};
use log::{debug, error};
use tari_core::{
    base_node::rpc::{models::TipInfoResponse, query_service, query_service::Error, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_tip_info";

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
) -> Result<Json<TipInfoResponse>, StatusCode> {
    debug!(target: LOG_TARGET, "Received get_tip_info request...");

    let tip_info = query_service.get_tip_info().await.map_err(|error| {
        error!(target: LOG_TARGET, "Error getting tip info: {:?}", error);
        match error {
            Error::HeaderNotFound { .. } => StatusCode::NOT_FOUND,
            Error::FailedToGetChainMetadata(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    })?;
    Ok(Json(tip_info))
}
