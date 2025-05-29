// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::debug;
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{
        models::{SyncUtxosByBlockRequest, SyncUtxosByBlockResponse},
        query_service,
        BaseNodeWalletQueryService,
    },
    chain_storage::BlockchainBackend,
};

use crate::http::handler::{error_handler_with_message, util::from_hex, ErrorResponse};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::sync_utxos_by_block";

#[derive(Deserialize, Debug)]
pub struct SyncUtxosByBlockQueryParams {
    #[serde(deserialize_with = "from_hex")]
    pub start_header_hash: Vec<u8>,
    #[serde(deserialize_with = "from_hex")]
    pub end_header_hash: Vec<u8>,
    pub limit: u64,
    pub page: u64,
}

impl From<SyncUtxosByBlockQueryParams> for SyncUtxosByBlockRequest {
    fn from(params: SyncUtxosByBlockQueryParams) -> Self {
        Self {
            start_header_hash: params.start_header_hash,
            end_header_hash: params.end_header_hash,
            limit: params.limit,
            page: params.page,
        }
    }
}

pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<SyncUtxosByBlockQueryParams>,
) -> Result<Json<SyncUtxosByBlockResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received sync_utxos_by_block request: {params:?}");
    let request = params.into();

    let response = query_service
        .sync_utxos_by_block(request)
        .await
        .map_err(error_handler_with_message)?;

    Ok(Json(response))
}
