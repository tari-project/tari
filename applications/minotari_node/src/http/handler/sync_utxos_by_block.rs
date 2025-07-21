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
    base_node::rpc::{
        models::{SyncUtxosByBlockRequest, SyncUtxosByBlockResponse},
        query_service,
        BaseNodeWalletQueryService,
    },
    chain_storage::BlockchainBackend,
};
use tonic::service::AxumBody;

use crate::http::handler::{error_handler_with_message, util::from_hex, ErrorResponse};

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
}

impl From<SyncUtxosByBlockQueryParams> for SyncUtxosByBlockRequest {
    fn from(params: SyncUtxosByBlockQueryParams) -> Self {
        Self {
            start_header_hash: params.start_header_hash,
            limit: params.limit,
            page: params.page,
        }
    }
}

#[utoipa::path(
    get,
    operation_id = "sync_utxos_by_block",
    params(SyncUtxosByBlockQueryParams),
    path = "/sync_utxos_by_block",
    responses(
        (status = 200, description = "UTXOs returned successfully in the given headers' hash range", body = SyncUtxosByBlockResponse),
        (status = NOT_FOUND, description = "Header not found", body = ErrorResponse, example = json!({"error": "Header not found at height: 10"})),
        (status = INTERNAL_SERVER_ERROR, description = "Start/end header hash not found or header height mismatch", body = ErrorResponse),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<SyncUtxosByBlockQueryParams>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received sync_utxos_by_block request: {params:?}");
    let request = params.into();

    let response = query_service
        .sync_utxos_by_block(request)
        .await
        .map_err(error_handler_with_message)?;

    let body = Json(response);
    let mut response = body.into_response();
    response.headers_mut().insert(
        "Cache-Control",
        "public, max-age=3600, s-maxage=1800, stale-while-revalidate=60"
            .parse()
            .expect("should be a valid header value"),
    );
    Ok(response)
}
