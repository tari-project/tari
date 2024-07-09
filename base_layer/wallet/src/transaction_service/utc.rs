// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{convert::TryFrom, time::Duration};

use chrono::{NaiveDateTime, Utc};
use thiserror::Error;
/// The error happens when a duration is negative.
#[derive(Debug, Error)]
#[error("Diration is negative: {ms} ms")]
pub struct NegativeDurationError {
    ms: i64,
}

/// The function compared two non-leap UTC timestamps.
/// 1. `chrono` uses `SystemTime` and will never produce leap-seconds.
/// 2. `chrono` supports leap seconds that can be read from the string format (as `60` second), because it's required by
///    the standard (ISO 8601).
/// 3. Leap-second handled automatically by NTP and we could ignore it as soon as `chrono` doesn't handle them
///    accurately. No guarantees and only the one second handeled.
pub fn utc_duration_since(since: &NaiveDateTime) -> Result<Duration, NegativeDurationError> {
    let now_ms = Utc::now().naive_utc().timestamp_millis();
    let since_ms = since.timestamp_millis();
    let ms = now_ms - since_ms;
    if ms >= 0 {
        Ok(Duration::from_millis(u64::try_from(ms).unwrap()))
    } else {
        Err(NegativeDurationError { ms })
    }
}
