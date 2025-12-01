// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::sync::Arc;

use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension,
    Json,
};
use log::debug;
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{query_service, BaseNodeWalletQueryService},
    chain_storage::BlockchainBackend,
};
use tari_transaction_components::rpc::models::{GetUtxoRequest, GetUtxoResponse};
use tonic::service::AxumBody;

use crate::{
    http::{
        cache_config::{apply_cache_control, RouteKey},
        handler::{error_handler_with_message, util::from_hex, ErrorResponse},
    },
    HttpCacheConfig,
};
const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_utxo";

#[derive(Deserialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetUtxoQueryParams {
    #[serde(deserialize_with = "from_hex")]
    pub utxo: Vec<u8>,
}

impl From<GetUtxoQueryParams> for GetUtxoRequest {
    fn from(params: GetUtxoQueryParams) -> Self {
        Self {
            output_hash: params.utxo,
        }
    }
}

#[utoipa::path(
    get,
    operation_id = "get_utxo",
    params(GetUtxoQueryParams),
    path = "/get_utxo",
    responses(
        (status = 200, description = "UTXOs returned successfully for the header", body = GetUtxoResponse),
        (status = NOT_FOUND, description = "output not found", body = ErrorResponse, example = json!({"error": "output not found"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetUtxoQueryParams>,
    Extension(cache_cfg): Extension<Arc<HttpCacheConfig>>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_utxo request: {params:?}");

    let request = params.into();

    let response = query_service
        .get_utxo(request)
        .await
        .map_err(error_handler_with_message)?;
    let body = Json(response);
    let mut response = body.into_response();
    apply_cache_control(response.headers_mut(), &cache_cfg, RouteKey::GetUtxosByBlock);
    Ok(response)
}
