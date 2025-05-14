use crate::base_node::rpc::http::models::{ChainMetadata, TipInfoResponse};
use crate::base_node::rpc::{http, BaseNodeWalletQueryService};
use crate::chain_storage::BlockchainBackend;
use crate::proto;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use log::error;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_tip_info";

impl From<proto::base_node::ChainMetadata> for ChainMetadata {
    fn from(proto_metadata: proto::base_node::ChainMetadata) -> Self {
        ChainMetadata {
            best_block_height: proto_metadata.best_block_height,
            best_block_hash: proto_metadata.best_block_hash,
            accumulated_difficulty: proto_metadata.accumulated_difficulty,
            pruned_height: proto_metadata.pruned_height,
            timestamp: proto_metadata.timestamp,
        }
    }
}

impl From<proto::base_node::TipInfoResponse> for TipInfoResponse {
    fn from(proto_resp: proto::base_node::TipInfoResponse) -> Self {
        TipInfoResponse {
            metadata: proto_resp.metadata.map(|metadata| metadata.into()),
            is_synced: proto_resp.is_synced,
        }
    }
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<http::query_service::Service<B>>>,
) -> Result<Json<TipInfoResponse>, StatusCode> {
    let tip_info = query_service.get_tip_info().await
        .map_err(|error| {
            error!(target: LOG_TARGET, "Error getting tip info: {:?}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(tip_info.into()))
}