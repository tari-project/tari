// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{cmp, fmt::Display, sync::Arc};

use axum::{
    Extension,
    Json,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use log::debug;
use serde::Deserialize;
use tari_common_types::types::HashOutput;
use tari_core::{
    base_node::rpc::{BaseNodeWalletQueryService, query_service},
    chain_storage::BlockchainBackend,
};
use tari_transaction_components::rpc::models::{
    SyncUtxosByBlockRequest,
    SyncUtxosByBlockResponseV0,
    SyncUtxosByBlockResponseV1,
};
use tari_utilities::hex::Hex;
use tonic::service::AxumBody;

use crate::{
    HttpCacheConfig,
    http::{
        cache_config::{RouteKey, apply_cache_control},
        handler::{ErrorResponse, error_handler_with_message, util::from_hex},
    },
};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::sync_utxos_by_block";

#[derive(Deserialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SyncUtxosByBlockQueryParams {
    #[serde(deserialize_with = "from_hex")]
    #[param(value_type = String, example = "1a8da4213566e3cda06958c7ee46b87870a587fabb1c7f050f553b6da36cccb3"
    )]
    pub start_header_hash: Vec<u8>,
    #[param(value_type = u64, example = 5)]
    pub limit: u64,
    #[param(value_type = u64, example = 0)]
    pub page: u64,
    #[serde(default)]
    #[param(value_type = bool, example = false)]
    pub exclude_spent: bool,
    #[serde(default)]
    #[param(value_type = bool, example = false)]
    pub exclude_inputs: bool,
    #[serde(default)]
    pub version: u8,
}

impl From<SyncUtxosByBlockQueryParams> for SyncUtxosByBlockRequest {
    fn from(params: SyncUtxosByBlockQueryParams) -> Self {
        Self {
            start_header_hash: params.start_header_hash,
            limit: params.limit,
            page: params.page,
            exclude_spent: params.exclude_spent,
            exclude_inputs: params.exclude_inputs,
            version: params.version,
        }
    }
}

#[utoipa::path(
    get,
    operation_id = "sync_utxos_by_block",
    params(SyncUtxosByBlockQueryParams),
    path = "/sync_utxos_by_block",
    responses(
        (status = 200, description = "UTXOs returned successfully in the given headers' hash range", body = SyncUtxosByBlockResponseV0),
        (status = 200, description = "UTXOs returned successfully in the given headers' hash range", body = SyncUtxosByBlockResponseV1),
        (status = NOT_FOUND, description = "Header not found", body = ErrorResponse, example = json!({"error": "Header not found at height: 10"})),
        (status = INTERNAL_SERVER_ERROR, description = "Start/end header hash not found or header height mismatch", body = ErrorResponse),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<SyncUtxosByBlockQueryParams>,
    Extension(cache_cfg): Extension<Arc<HttpCacheConfig>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received sync_utxos_by_block request: {params}");
    let request: SyncUtxosByBlockRequest = params.into();
    let tip_info = query_service.get_tip_info().await.map_err(error_handler_with_message)?;
    let tip_height = tip_info.metadata.map(|m| m.best_block_height()).unwrap_or(0);
    let height;
    let exclude_spent = request.exclude_spent;
    let mut response = match request.version {
        0 => {
            let response = query_service
                .sync_utxos_by_block_v0(request)
                .await
                .map_err(error_handler_with_message)?;
            height = response.blocks.last().map(|b| b.height);
            let body = Json(response);
            body.into_response()
        },
        _ => {
            let response = query_service
                .sync_utxos_by_block_v1(request)
                .await
                .map_err(error_handler_with_message)?;
            height = response.blocks.last().map(|b| b.height);
            let body = Json(response);
            body.into_response()
        },
    };
    let last_height = if let true = exclude_spent {
        // if we're excluding spent outputs we cannot cache for longer than a day, so we force the "height" so that the
        // caching will only be a max of 1 day. Wallets will sent full without excluding spends the last 1000 blocks so
        // this leaves a margin for wallets to get data if it changed.
        cmp::min(height.unwrap_or(0), tip_height.saturating_add(2500))
    } else {
        height.unwrap_or(0)
    };

    apply_cache_control(
        response.headers_mut(),
        &cache_cfg,
        RouteKey::SyncUtxosByBlock,
        tip_height,
        last_height,
    );
    Ok(response)
}

impl Display for SyncUtxosByBlockQueryParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SyncUtxosByBlockQueryParams {{ start_header_hash: {}, limit: {}, page: {}, exclude_spent: {}, \
             exclude_inputs: {} }}",
            HashOutput::try_from(self.start_header_hash.as_slice())
                .unwrap_or_default()
                .to_hex(),
            self.limit,
            self.page,
            self.exclude_spent,
            self.exclude_inputs
        )
    }
}
