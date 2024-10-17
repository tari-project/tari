//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The epoch time relative to the Unix epoch used for peer record signatures. This allows converting to and from a Unix
/// epoch and the timestamps used in peer records. This allows timestamp to be represented in less bytes (varint
/// encoding). The BASE_EPOCH_TIME is December 12, 2023 12:00:00 AM UTC
pub const BASE_EPOCH_TIME: Duration = Duration::from_secs(1_702_339_200);

pub fn epoch_time_now() -> Duration {
    // If the system time is before the UNIX_EPOCH, then we emit a time at the BASE_EPOCH_TIME since no update time in
    // this crate can be before that.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .saturating_sub(BASE_EPOCH_TIME)
}
