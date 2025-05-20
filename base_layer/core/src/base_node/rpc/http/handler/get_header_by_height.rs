use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::{debug, error};
use serde::Deserialize;

use crate::{
    base_node::rpc::{http, http::query_service::Error, BaseNodeWalletQueryService},
    blocks::BlockHeader,
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_header_by_height";

#[derive(Deserialize)]
pub struct QueryParams {
    pub height: u64,
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<http::query_service::Service<B>>>,
    Query(params): Query<QueryParams>,
) -> Result<Json<BlockHeader>, StatusCode> {
    debug!(target: LOG_TARGET, "Received get_header_by_height request: {}", params.height);

    let response = query_service
        .get_header_by_height(params.height)
        .await
        .map_err(|error| {
            error!(target: LOG_TARGET, "Error getting header by height: {:?}", error);
            if matches!(error, Error::HeaderNotFound { .. }) {
                return StatusCode::NOT_FOUND;
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(response))
}
