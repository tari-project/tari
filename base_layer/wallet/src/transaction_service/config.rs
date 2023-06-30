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

use std::{fmt, time::Duration};

use log::*;
use serde::{Deserialize, Serialize};
use tari_common::configuration::serializers;

const LOG_TARGET: &str = "wallet::transaction_service::config";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionServiceConfig {
    /// This is the timeout period that will be used for base node broadcast monitoring tasks
    #[serde(with = "serializers::seconds")]
    pub broadcast_monitoring_timeout: Duration,
    /// This is the timeout period that will be used for chain monitoring tasks
    #[serde(with = "serializers::seconds")]
    pub chain_monitoring_timeout: Duration,
    /// This is the timeout period that will be used for sending transactions directly
    #[serde(with = "serializers::seconds")]
    pub direct_send_timeout: Duration,
    /// This is the timeout period that will be used for sending transactions via broadcast mode
    #[serde(with = "serializers::seconds")]
    pub broadcast_send_timeout: Duration,
    /// This is the timeout period that will be used for low power moded polling tasks
    #[serde(with = "serializers::seconds")]
    pub low_power_polling_timeout: Duration,
    /// This is the timeout period that will be used to resend transactions that did not make any progress
    #[serde(with = "serializers::seconds")]
    pub transaction_resend_period: Duration,
    /// This is the timeout period that will be used to ignore repeated transactions
    #[serde(with = "serializers::seconds")]
    pub resend_response_cooldown: Duration,
    /// This is the timeout period that will be used to expire pending transactions
    #[serde(with = "serializers::seconds")]
    pub pending_transaction_cancellation_timeout: Duration,
    /// This is the number of block confirmations required for a transaction to be considered completely mined and
    /// confirmed
    pub num_confirmations_required: u64,
    /// The number of batches the unconfirmed transactions will be divided into before being queried from the base node
    pub max_tx_query_batch_size: usize,
    /// This option specifies the transaction routing mechanism as being directly between wallets, making use of store
    /// and forward or using any combination of these.
    pub transaction_routing_mechanism: TransactionRoutingMechanism,
    /// This is the size of the event channel used to communicate transaction status events to the wallet's UI. A busy
    /// console wallet doing thousands of bulk payments or used for stress testing needs a fairly big size.
    pub transaction_event_channel_size: usize,
    /// This is the timeout period that will be used to re-submit transactions not found in the mempool
    #[serde(with = "serializers::seconds")]
    pub transaction_mempool_resubmission_window: Duration,
}

impl Default for TransactionServiceConfig {
    fn default() -> Self {
        Self {
            broadcast_monitoring_timeout: Duration::from_secs(30),
            chain_monitoring_timeout: Duration::from_secs(60),
            direct_send_timeout: Duration::from_secs(20),
            broadcast_send_timeout: Duration::from_secs(60),
            low_power_polling_timeout: Duration::from_secs(300),
            transaction_resend_period: Duration::from_secs(600),
            resend_response_cooldown: Duration::from_secs(300),
            pending_transaction_cancellation_timeout: Duration::from_secs(259_200), // 3 Days
            num_confirmations_required: 3,
            max_tx_query_batch_size: 20,
            transaction_routing_mechanism: TransactionRoutingMechanism::default(),
            transaction_event_channel_size: 1000,
            transaction_mempool_resubmission_window: Duration::from_secs(600),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum TransactionRoutingMechanism {
    DirectOnly,
    StoreAndForwardOnly,
    DirectAndStoreAndForward,
}

impl fmt::Display for TransactionRoutingMechanism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectOnly => f.write_str("'DirectOnly'"),
            Self::StoreAndForwardOnly => f.write_str("'StoreAndForwardOnly'"),
            Self::DirectAndStoreAndForward => f.write_str("'DirectAndStoreAndForward'"),
        }
    }
}

impl From<String> for TransactionRoutingMechanism {
    fn from(value: String) -> Self {
        match value.as_str() {
            "DirectOnly" => Self::DirectOnly,
            "StoreAndForwardOnly" => Self::StoreAndForwardOnly,
            "DirectAndStoreAndForward" => Self::DirectAndStoreAndForward,
            _ => {
                warn!(
                    target: LOG_TARGET,
                    "Transaction sending mechanism config setting not recognized, using default value {}",
                    Self::DirectAndStoreAndForward
                );
                Self::DirectAndStoreAndForward
            },
        }
    }
}

impl Default for TransactionRoutingMechanism {
    fn default() -> Self {
        Self::DirectAndStoreAndForward
    }
}
