// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use axum::{
    Extension,
    Json,
    extract::Query,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use log::debug;
use serde::Deserialize;
use tari_common_types::{serializers, types::CompressedCommitment};
use tari_core::{
    base_node::rpc::{BaseNodeWalletQueryService, query_service},
    chain_storage::BlockchainBackend,
};
use tari_transaction_components::rpc::models::GenerateBurnOutputProofResponse;
use tari_utilities::ByteArray;
use tonic::service::AxumBody;

use crate::http::handler::{ErrorResponse, error_handler_with_message};

const LOG_TARGET: &str = "c::base_node::rpc::http::handler::generate_burn_output_proof";

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GenerateBurnOutputProofParams {
    /// The hex-encoded commitment of the burn output
    #[serde(deserialize_with = "serializers::hex::deserialize")]
    pub commitment: Vec<u8>,
}

#[utoipa::path(
    get,
    operation_id = "generate_burn_output_proof",
    params(GenerateBurnOutputProofParams),
    path = "/generate_burn_output_proof",
    responses(
        (status = 200, description = "Burn output proof generated successfully", body = GenerateBurnOutputProofResponse),
        (status = NOT_FOUND, description = "Burn not found", body = ErrorResponse, example = json!({"error": "Burn not found"})),
        (status = GONE, description = "The block body is pruned on this node, use an archival node", body = ErrorResponse),
    ),
)]
pub async fn handle<B: BlockchainBackend + 'static>(
    Extension(query_service): Extension<Arc<query_service::Service<B>>>,
    Query(params): Query<GenerateBurnOutputProofParams>,
) -> Result<Response<AxumBody>, (StatusCode, Json<ErrorResponse>)> {
    debug!(target: LOG_TARGET, "Received generate_burn_output_proof request");

    let commitment = CompressedCommitment::from_canonical_bytes(&params.commitment).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!("Invalid commitment: {}", e))),
        )
    })?;

    let response = query_service
        .generate_burn_output_proof(commitment)
        .await
        .map_err(error_handler_with_message)?;

    let body = Json(response);
    let mut response = body.into_response();
    response.headers_mut().insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=120, s-maxage=60, stale-while-revalidate=15"),
    );
    Ok(response)
}
