// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[derive(Debug, thiserror::Error)]
pub enum SidechainProofValidationError {
    #[error("Unexpected command: {details}")]
    UnexpectedCommand { details: String },
    #[error("Invalid proof: {details}")]
    InvalidProof { details: String },
    #[error("Internal error: {details}")]
    InternalError { details: String },
    #[error("Jellyfish proof verification error: {0}")]
    JmtProofVerifyError(#[from] tari_jellyfish::JmtProofVerifyError),
}

impl SidechainProofValidationError {
    pub fn is_internal_error(&self) -> bool {
        matches!(self, Self::InternalError { .. })
    }

    pub fn internal_error<T: ToString>(details: T) -> Self {
        Self::InternalError {
            details: details.to_string(),
        }
    }
}
