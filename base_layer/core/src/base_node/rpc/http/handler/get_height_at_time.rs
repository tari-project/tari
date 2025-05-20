use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::{debug, error};
use serde::Deserialize;

use crate::{
    base_node::rpc::{http, http::query_service::Error, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_height_at_time";

#[derive(Deserialize)]
pub struct QueryParams {
    pub time: u64,
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<http::query_service::Service<B>>>,
    Query(params): Query<QueryParams>,
) -> Result<Json<u64>, StatusCode> {
    debug!(target: LOG_TARGET, "Received get_height_at_time request: {}", params.time);

    let response = query_service.get_height_at_time(params.time).await.map_err(|error| {
        error!(target: LOG_TARGET, "Error getting height at specific time: {:?}", error);
        if matches!(error, Error::HeaderNotFound { .. }) {
            return StatusCode::NOT_FOUND;
        }
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(response))
}
