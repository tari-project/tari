// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use log::*;
use std::time::Duration;

const LOG_TARGET: &str = "wallet::base_node_service::config";

#[derive(Clone)]
pub struct BaseNodeServiceConfig {
    pub refresh_interval: Duration,
    pub request_keys_max_age: Duration,
}

impl Default for BaseNodeServiceConfig {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_secs(30),
            request_keys_max_age: Duration::from_secs(120),
        }
    }
}

impl BaseNodeServiceConfig {
    pub fn new(refresh_interval: Duration, request_keys_max_age: Duration) -> Self {
        info!(
            target: LOG_TARGET,
            "Setting new wallet base node service config, refresh_interval: {:?}, request_keys_max_age: {:?}",
            refresh_interval,
            request_keys_max_age
        );
        Self {
            refresh_interval,
            request_keys_max_age,
        }
    }
}
