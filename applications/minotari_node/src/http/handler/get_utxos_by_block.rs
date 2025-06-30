// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::debug;
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{
        models::{GetUtxosByBlockRequest, GetUtxosByBlockResponse},
        query_service,
        BaseNodeWalletQueryService,
    },
    chain_storage::BlockchainBackend,
};

use crate::http::handler::{error_handler_with_message, util::from_hex, ErrorResponse};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_utxos_by_block";

#[derive(Deserialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetUtxosByBlockQueryParams {
    #[serde(deserialize_with = "from_hex")]
    #[param(value_type = String, example = "1a8da4213566e3cda06958c7ee46b87870a587fabb1c7f050f553b6da36cccb3"
    )]
    pub header_hash: Vec<u8>,
}

impl From<GetUtxosByBlockQueryParams> for GetUtxosByBlockRequest {
    fn from(params: GetUtxosByBlockQueryParams) -> Self {
        Self {
            header_hash: params.header_hash,
        }
    }
}

#[utoipa::path(
    get,
    operation_id = "get_utxos_by_block",
    params(GetUtxosByBlockQueryParams),
    path = "/get_utxos_by_block",
    responses(
        (status = 200, description = "UTXOs returned successfully for the header", body = GetUtxosByBlockResponse),
        (status = NOT_FOUND, description = "Header not found", body = ErrorResponse, example = json!({"error": "Header not found at height: 10"})),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetUtxosByBlockQueryParams>,
) -> Result<Json<GetUtxosByBlockResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_utxos_by_block request: {params:?}");
    let request = params.into();

    let response = query_service
        .get_utxos_by_block(request)
        .await
        .map_err(error_handler_with_message)?;

    Ok(Json(response))
}
