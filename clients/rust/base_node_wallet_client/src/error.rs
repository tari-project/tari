// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Failed to parse http address: {0}")]
    HttpAddressParse(#[from] url::ParseError),
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),
}
