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
        models::{GetUtxosDeletedInfoRequest, GetUtxosDeletedInfoResponse},
        query_service,
        BaseNodeWalletQueryService,
    },
    chain_storage::BlockchainBackend,
};
use tonic::service::AxumBody;

use crate::http::handler::{
    error_handler_with_message,
    util::{from_hex, from_hex_comma_separated},
    ErrorResponse,
};
const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_utxos_deleted_info";

#[derive(Deserialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetUtxosDeletedInfoParams {
    #[serde(deserialize_with = "from_hex_comma_separated")]
    pub hashes: Vec<Vec<u8>>,
    #[serde(deserialize_with = "from_hex")]
    pub must_include_header: Vec<u8>,
}

impl From<GetUtxosDeletedInfoParams> for GetUtxosDeletedInfoRequest {
    fn from(params: GetUtxosDeletedInfoParams) -> Self {
        Self {
            hashes: params.hashes,
            must_include_header: params.must_include_header,
        }
    }
}

#[utoipa::path(
    get,
    operation_id = "get_utxos_deleted_info",
    params(GetUtxosDeletedInfoParams),
    path = "/get_utxos_deleted_info",
    responses(
        (status = 200, description = "UTXOs Deleted Info", body = GetUtxosDeletedInfoResponse),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetUtxosDeletedInfoParams>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_utxos_deleted_info request: {params:?}");
    let request = params.into();

    let response = query_service
        .get_utxos_deleted_info(request)
        .await
        .map_err(error_handler_with_message)?;

    let body = Json(response);
    let mut response = body.into_response();
    response.headers_mut().insert(
        "Cache-Control",
        "public, max-age=60, s-maxage=30, stale-while-revalidate=15"
            .parse()
            .expect("should be a valid header value"),
    );
    Ok(response)
}
