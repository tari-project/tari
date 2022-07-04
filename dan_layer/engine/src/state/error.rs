// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::sync::PoisonError;

use tari_mmr::error::MerkleMountainRangeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateStorageError {
    #[error("Lock error")]
    LockError,
    #[error("Merkle error:{0}")]
    MerkleMountainRangeError(#[from] MerkleMountainRangeError),
    #[error("Could not connect to storage:{reason}")]
    ConnectionError { reason: String },
    #[error("Query error:{reason}")]
    QueryError { reason: String },
    #[error("Migration error: {reason}")]
    MigrationError { reason: String },
    #[error("General storage error: {details}")]
    General { details: String },
}

impl<T> From<PoisonError<T>> for StateStorageError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::LockError
    }
}
