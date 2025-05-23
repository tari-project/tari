// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::{debug, error};
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{query_service, query_service::Error, BaseNodeWalletQueryService},
    blocks::BlockHeader,
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_header_by_height";

#[derive(Deserialize)]
pub struct QueryParams {
    pub height: u64,
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<QueryParams>,
) -> Result<Json<BlockHeader>, StatusCode> {
    debug!(target: LOG_TARGET, "Received get_header_by_height request: {}", params.height);

    let response = query_service
        .get_header_by_height(params.height)
        .await
        .map_err(|error| {
            error!(target: LOG_TARGET, "Error getting header by height: {:?}", error);
            match error {
                Error::HeaderNotFound { .. } => StatusCode::NOT_FOUND,
                Error::FailedToGetChainMetadata(_) => StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?;
    Ok(Json(response))
}
