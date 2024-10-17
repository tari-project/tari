//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub max_concurrent_streams_per_peer: usize,
    pub send_recv_timeout: Duration,
    pub inbound_message_buffer_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_concurrent_streams_per_peer: 3,
            send_recv_timeout: Duration::from_secs(10),
            inbound_message_buffer_size: 10,
        }
    }
}
