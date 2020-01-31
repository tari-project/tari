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

use crate::mempool::{
    consts,
    orphan_pool::OrphanPoolConfig,
    pending_pool::PendingPoolConfig,
    reorg_pool::ReorgPoolConfig,
    unconfirmed_pool::UnconfirmedPoolConfig,
};
use bitflags::_core::time::Duration;
use config::Config;
use tari_common::{ConfigExtractor, ConfigurationError, Network};

/// Configuration for the Mempool.
#[derive(Clone, Copy)]
pub struct MempoolConfig {
    pub unconfirmed_pool_config: UnconfirmedPoolConfig,
    pub orphan_pool_config: OrphanPoolConfig,
    pub pending_pool_config: PendingPoolConfig,
    pub reorg_pool_config: ReorgPoolConfig,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            unconfirmed_pool_config: UnconfirmedPoolConfig::default(),
            orphan_pool_config: OrphanPoolConfig::default(),
            pending_pool_config: PendingPoolConfig::default(),
            reorg_pool_config: ReorgPoolConfig::default(),
        }
    }
}

impl ConfigExtractor for MempoolConfig {
    fn set_default(cfg: &mut Config) {
        let default = MempoolConfig::default();
        for network in &["testnet", "mainnet"] {
            cfg.set_default(
                &format!("mempool.{}.unconfirmed_pool_storage_capacity", network),
                default.unconfirmed_pool_config.storage_capacity as i64,
            )
            .unwrap();
            cfg.set_default(
                &format!("mempool.{}.weight_tx_skip_count", network),
                default.unconfirmed_pool_config.weight_tx_skip_count as i64,
            )
            .unwrap();
            cfg.set_default(
                &format!("mempool.{}.orphan_pool_storage_capacity", network),
                default.orphan_pool_config.storage_capacity as i64,
            )
            .unwrap();
            cfg.set_default(
                &format!("mempool.{}.orphan_tx_ttl", network),
                default.orphan_pool_config.tx_ttl.as_secs() as i64,
            )
            .unwrap();
            cfg.set_default(
                &format!("mempool.{}.pending_pool_storage_capacity", network),
                default.pending_pool_config.storage_capacity as i64,
            )
            .unwrap();
            cfg.set_default(
                &format!("mempool.{}.reorg_pool_storage_capacity", network),
                default.reorg_pool_config.storage_capacity as i64,
            )
            .unwrap();
            cfg.set_default(
                &format!("mempool.{}.reorg_tx_ttl", network),
                default.reorg_pool_config.tx_ttl.as_secs() as i64,
            )
            .unwrap();
        }
    }

    fn extract_configuration(cfg: &Config, network: Network) -> Result<Self, ConfigurationError>
    where Self: Sized {
        let mut config = MempoolConfig::default();
        let key = format!("mempool.{}.unconfirmed_pool_storage_capacity", network);
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
        config.unconfirmed_pool_config.storage_capacity = val;
        let key = format!("mempool.{}.weight_tx_skip_count", network);
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
        config.unconfirmed_pool_config.weight_tx_skip_count = val;
        let key = format!("mempool.{}.orphan_pool_storage_capacity", network);
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
        config.orphan_pool_config.storage_capacity = val;
        let key = format!("mempool.{}.orphan_tx_ttl", network);
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64;
        config.orphan_pool_config.tx_ttl = Duration::from_secs(val);
        let key = format!("mempool.{}.pending_pool_storage_capacity", network);
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
        config.pending_pool_config.storage_capacity = val;
        let key = format!("mempool.{}.reorg_pool_storage_capacity", network);
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
        config.reorg_pool_config.storage_capacity = val;
        let key = format!("mempool.{}.reorg_tx_ttl", network);
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64;
        config.reorg_pool_config.tx_ttl = Duration::from_secs(val);
        Ok(config)
    }
}

/// Configuration for the MempoolService.
#[derive(Clone, Copy)]
pub struct MempoolServiceConfig {
    /// The allocated waiting time for a request waiting for service responses from the Mempools of remote Base nodes.
    pub request_timeout: Duration,
}

impl Default for MempoolServiceConfig {
    fn default() -> Self {
        Self {
            request_timeout: consts::MEMPOOL_SERVICE_REQUEST_TIMEOUT,
        }
    }
}

impl ConfigExtractor for MempoolServiceConfig {
    fn set_default(cfg: &mut Config) {
        let service_default = MempoolServiceConfig::default();
        for network in &["testnet", "mainnet"] {
            let key = format!("mempool.{}.request_timeout", network);
            cfg.set_default(&key, service_default.request_timeout.as_secs() as i64)
                .unwrap();
        }
    }

    fn extract_configuration(cfg: &Config, network: Network) -> Result<Self, ConfigurationError>
    where Self: Sized {
        let mut config = MempoolServiceConfig::default();
        let key = config_string(network, "request_timeout");
        let val = cfg
            .get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
        config.request_timeout = Duration::from_secs(val as u64);
        Ok(config)
    }
}

fn config_string(network: Network, key: &str) -> String {
    format!("mempool.{}.{}", network, key)
}
