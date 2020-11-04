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

const LOG_TARGET: &str = "wallet::transaction_service::config";

#[derive(Clone)]
pub struct TransactionServiceConfig {
    pub broadcast_monitoring_timeout: Duration,
    pub chain_monitoring_timeout: Duration,
    pub direct_send_timeout: Duration,
    pub broadcast_send_timeout: Duration,
    pub low_power_polling_timeout: Duration,
    pub transaction_resend_period: Duration,
    pub resend_response_cooldown: Duration,
    pub pending_transaction_cancellation_timeout: Duration,
}

impl Default for TransactionServiceConfig {
    fn default() -> Self {
        Self {
            broadcast_monitoring_timeout: Duration::from_secs(30),
            chain_monitoring_timeout: Duration::from_secs(30),
            direct_send_timeout: Duration::from_secs(20),
            broadcast_send_timeout: Duration::from_secs(30),
            low_power_polling_timeout: Duration::from_secs(300),
            transaction_resend_period: Duration::from_secs(3600),
            resend_response_cooldown: Duration::from_secs(300),
            pending_transaction_cancellation_timeout: Duration::from_secs(259200), // 3 Days
        }
    }
}

impl TransactionServiceConfig {
    pub fn new(
        broadcast_monitoring_timeout: Duration,
        chain_monitoring_timeout: Duration,
        direct_send_timeout: Duration,
        broadcast_send_timeout: Duration,
    ) -> Self
    {
        trace!(
            target: LOG_TARGET,
            "Timeouts - Broadcast monitoring: {} s, Chain monitoring: {} s, Direct send: {} s, Broadcast send: {} s",
            broadcast_monitoring_timeout.as_secs(),
            chain_monitoring_timeout.as_secs(),
            direct_send_timeout.as_secs(),
            broadcast_send_timeout.as_secs(),
        );
        Self {
            broadcast_monitoring_timeout,
            chain_monitoring_timeout,
            direct_send_timeout,
            broadcast_send_timeout,
            ..Default::default()
        }
    }
}
