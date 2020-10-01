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

const LOG_TARGET: &str = "wallet::output_manager_service::config";

#[derive(Clone)]
pub struct OutputManagerServiceConfig {
    pub base_node_query_timeout: Duration,
    pub max_utxo_query_size: usize,
    pub prevent_fee_gt_amount: bool,
}

impl Default for OutputManagerServiceConfig {
    fn default() -> Self {
        Self {
            base_node_query_timeout: Duration::from_secs(30),
            max_utxo_query_size: 5000,
            prevent_fee_gt_amount: true,
        }
    }
}
impl OutputManagerServiceConfig {
    pub fn new(base_node_query_timeout: Duration, prevent_fee_gt_amount: bool) -> Self {
        trace!(
            target: LOG_TARGET,
            "Timeouts - Base node query: {} s",
            base_node_query_timeout.as_secs()
        );
        Self {
            base_node_query_timeout,
            prevent_fee_gt_amount,
            ..Default::default()
        }
    }
}
