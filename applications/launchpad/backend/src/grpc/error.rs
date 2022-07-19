// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GrpcError {
    #[error("Fatal error: {0}")]
    FatalError(String),
    #[error("Connection error: {0}")]
    GrpcConnection(#[from] tonic::transport::Error),
    #[error("GRPC error: {0}")]
    GrpcStatus(#[from] tonic::Status),
}
impl GrpcError {
    pub fn chained_message(&self) -> String {
        let mut messages = vec![self.to_string()];
        let mut this = self as &dyn std::error::Error;
        while let Some(next) = this.source() {
            messages.push(next.to_string());
            this = next;
        }
        messages.join(" caused by:\n")
    }
}
