use crate::base_node::rpc::http::query_service::Error;
use crate::base_node::rpc::{http, BaseNodeWalletQueryService};
use crate::blocks::BlockHeader;
use crate::chain_storage::BlockchainBackend;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::{Extension, Json};
use log::error;
use serde::Deserialize;
use std::sync::Arc;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_header_by_height";

#[derive(Deserialize)]
pub struct QueryParams {
    pub height: u64,
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<http::query_service::Service<B>>>,
    Query(params): Query<QueryParams>,
) -> Result<Json<BlockHeader>, StatusCode> {
    let response = query_service.get_header_by_height(params.height).await
        .map_err(|error| {
            error!(target: LOG_TARGET, "Error getting header by height: {:?}", error);
            if matches!(error, Error::HeaderNotFound {..}) {
                return StatusCode::NOT_FOUND;
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(response))
}