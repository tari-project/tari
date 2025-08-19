// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use thiserror::Error;
/// The error happens when a duration is negative.
#[derive(Debug, Error)]
#[error("Diration is negative: {ms} ms")]
pub struct NegativeDurationError {
    ms: i64,
}
