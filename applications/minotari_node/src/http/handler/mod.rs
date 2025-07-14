// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tari_core::base_node::rpc::{query_service, query_service::Error};
use utoipa::ToSchema;
pub mod get_header_by_height;
pub mod get_utxos_by_block;
pub use get_header_by_height::__path_handle as __path_get_header_by_height;
pub mod get_tip_info;
pub use get_tip_info::__path_handle as __path_get_tip_info;
pub mod get_height_at_time;
pub use get_height_at_time::__path_handle as __path_get_height_at_time;
pub mod sync_utxos_by_block;
pub use sync_utxos_by_block::__path_handle as __path_sync_utxos_by_block;
pub mod get_utxos_deleted_info;
pub mod get_utxos_mined_info;
pub mod json_rpc;
pub mod transaction_query;
pub mod util;

pub fn query_service_error_to_status_code(error: query_service::Error) -> StatusCode {
    match error {
        Error::HeaderNotFound { .. } => StatusCode::NOT_FOUND,
        Error::FailedToGetChainMetadata(_) => StatusCode::INTERNAL_SERVER_ERROR,
        Error::SignatureConversion(_) => StatusCode::INTERNAL_SERVER_ERROR,
        Error::MempoolService(_) => StatusCode::INTERNAL_SERVER_ERROR,
        Error::SerdeValidation(_) => StatusCode::BAD_REQUEST,
        Error::HashConversion(_) => StatusCode::BAD_REQUEST,
        Error::StartHeaderHashNotFound => StatusCode::NOT_FOUND,
        Error::EndHeaderHashNotFound => StatusCode::NOT_FOUND,
        Error::HeaderHashNotFound => StatusCode::NOT_FOUND,
        Error::HeaderHeightMismatch { .. } => StatusCode::BAD_REQUEST,
    }
}

pub fn error_handler_with_message(error: Error) -> (StatusCode, Json<ErrorResponse>) {
    let error_str = error.to_string();
    (
        query_service_error_to_status_code(error),
        Json(ErrorResponse::new(error_str)),
    )
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    error: String,
}

impl ErrorResponse {
    pub fn new(error: String) -> Self {
        Self { error }
    }
}
