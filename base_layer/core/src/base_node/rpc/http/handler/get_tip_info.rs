use std::sync::Arc;

use axum::{http::StatusCode, Extension, Json};
use log::error;

use crate::{
    base_node::rpc::{http, models::TipInfoResponse, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_tip_info";

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<http::query_service::Service<B>>>,
) -> Result<Json<TipInfoResponse>, StatusCode> {
    let tip_info = query_service.get_tip_info().await.map_err(|error| {
        error!(target: LOG_TARGET, "Error getting tip info: {:?}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(tip_info.into()))
}
