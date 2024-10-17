//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub max_concurrent_streams: usize,
    /// The timeout for the client/server sync protocol to complete
    pub sync_timeout: Duration,
    pub max_want_list_len: usize,
    pub max_failure_retries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_concurrent_streams: 3,
            sync_timeout: Duration::from_secs(10),
            max_want_list_len: 1000,
            max_failure_retries: 3,
        }
    }
}
