// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use axum::http::StatusCode;
use tari_core::base_node::rpc::{query_service, query_service::Error};

pub mod get_header_by_height;
pub mod get_tip_info;

pub mod get_height_at_time;
pub mod transaction_query;

pub fn query_service_error_to_status_code(error: query_service::Error) -> StatusCode {
    match error {
        Error::HeaderNotFound { .. } => StatusCode::NOT_FOUND,
        Error::FailedToGetChainMetadata(_) => StatusCode::INTERNAL_SERVER_ERROR,
        Error::SignatureConversion(_) => StatusCode::INTERNAL_SERVER_ERROR,
        Error::MempoolService(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
