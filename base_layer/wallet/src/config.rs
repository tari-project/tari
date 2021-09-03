// Copyright 2021. The Tari Project
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

use std::time::Duration;

use tari_core::{consensus::NetworkConsensus, transactions::CryptoFactories};
use tari_p2p::{auto_update::AutoUpdateConfig, initialization::CommsConfig};

use crate::{
    base_node_service::config::BaseNodeServiceConfig,
    output_manager_service::config::OutputManagerServiceConfig,
    transaction_service::config::TransactionServiceConfig,
};

pub const KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY: &str = "comms";

#[derive(Clone)]
pub struct WalletConfig {
    pub comms_config: CommsConfig,
    pub factories: CryptoFactories,
    pub transaction_service_config: Option<TransactionServiceConfig>,
    pub output_manager_service_config: Option<OutputManagerServiceConfig>,
    pub buffer_size: usize,
    pub rate_limit: usize,
    pub network: NetworkConsensus,
    pub base_node_service_config: BaseNodeServiceConfig,
    pub scan_for_utxo_interval: Duration,
    pub updater_config: Option<AutoUpdateConfig>,
    pub autoupdate_check_interval: Option<Duration>,
}

impl WalletConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        comms_config: CommsConfig,
        factories: CryptoFactories,
        transaction_service_config: Option<TransactionServiceConfig>,
        output_manager_service_config: Option<OutputManagerServiceConfig>,
        network: NetworkConsensus,
        base_node_service_config: Option<BaseNodeServiceConfig>,
        buffer_size: Option<usize>,
        rate_limit: Option<usize>,
        scan_for_utxo_interval: Option<Duration>,
        updater_config: Option<AutoUpdateConfig>,
        autoupdate_check_interval: Option<Duration>,
    ) -> Self {
        Self {
            comms_config,
            factories,
            transaction_service_config,
            output_manager_service_config,
            buffer_size: buffer_size.unwrap_or(1500),
            rate_limit: rate_limit.unwrap_or(50),
            network,
            base_node_service_config: base_node_service_config.unwrap_or_default(),
            scan_for_utxo_interval: scan_for_utxo_interval.unwrap_or_else(|| Duration::from_secs(43200)),
            updater_config,
            autoupdate_check_interval,
        }
    }
}
