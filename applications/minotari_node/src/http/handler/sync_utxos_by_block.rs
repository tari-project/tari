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
    apply_cache_control(
        response.headers_mut(),
        &cache_cfg,
        RouteKey::SyncUtxosByBlock,
        tip_height,
        cache_height(exclude_spent, height, tip_height),
    );
    Ok(response)
}

/// Height the response is cached as.
fn cache_height(exclude_spent: bool, last_block_height: Option<u64>, tip_height: u64) -> u64 {
    let height = last_block_height.unwrap_or(0);
    if exclude_spent {
        // Spent state changes over time, so treat the response as at most 2500 blocks deep, which caches it for at
        // most a day. Wallets scan the last 1000 blocks without excluding spends, which leaves a margin.
        cmp::max(height, tip_height.saturating_sub(2500))
    } else {
        height
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn s_maxage(tip_height: u64, height: u64) -> String {
        HttpCacheConfig::default()
            .cache_control_for(RouteKey::SyncUtxosByBlock, tip_height, height)
            .split(", ")
            .find(|part| part.starts_with("s-maxage="))
            .unwrap()
            .to_string()
    }

    #[test]
    fn exclude_spent_deep_block_is_capped_at_one_day() {
        let height = cache_height(true, Some(1_000), 100_000);
        assert_eq!(height, 97_500);
        assert_eq!(s_maxage(100_000, height), "s-maxage=86400");
    }

    #[test]
    fn exclude_spent_near_tip_block_is_unchanged() {
        assert_eq!(cache_height(true, Some(99_990), 100_000), 99_990);
    }

    #[test]
    fn without_exclude_spent_height_is_unchanged() {
        assert_eq!(cache_height(false, Some(1_000), 100_000), 1_000);
    }

    #[test]
    fn missing_height_defaults_to_zero_before_the_cap() {
        assert_eq!(cache_height(false, None, 100_000), 0);
        assert_eq!(cache_height(true, None, 100_000), 97_500);
    }
}
