// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use log::debug;
use serde::Deserialize;
use tari_core::{
    base_node::rpc::{
        models::{GetUtxosMinedInfoRequest, GetUtxosMinedInfoResponse},
        query_service,
        BaseNodeWalletQueryService,
    },
    chain_storage::BlockchainBackend,
};

use crate::http::handler::{error_handler_with_message, util::from_hex_comma_separated, ErrorResponse};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::get_utxos_mined_info";

#[derive(Deserialize, Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetUtxosMinedInfoParams {
    #[serde(deserialize_with = "from_hex_comma_separated")]
    pub hashes: Vec<Vec<u8>>,
}

impl From<GetUtxosMinedInfoParams> for GetUtxosMinedInfoRequest {
    fn from(params: GetUtxosMinedInfoParams) -> Self {
        Self { hashes: params.hashes }
    }
}

#[utoipa::path(
    get,
    operation_id = "get_utxos_mined_info",
    params(GetUtxosMinedInfoParams),
    path = "/get_utxos_mined_info",
    responses(
        (status = 200, description = "UTXOs Mined Info", body = GetUtxosMinedInfoResponse),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GetUtxosMinedInfoParams>,
) -> Result<Json<GetUtxosMinedInfoResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received get_utxos_mined_info request: {params:?}");
    let request = params.into();

    let response = query_service
        .get_utxos_mined_info(request)
        .await
        .map_err(error_handler_with_message)?;

    Ok(Json(response))
}
