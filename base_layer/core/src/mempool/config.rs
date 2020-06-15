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
use serde::{Deserialize, Serialize};
use tari_common::{configuration::seconds, Network, NetworkConfigPath};

/// Configuration for the Mempool.
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct MempoolConfig {
    pub unconfirmed_pool: UnconfirmedPoolConfig,
    pub orphan_pool: OrphanPoolConfig,
    pub pending_pool: PendingPoolConfig,
    pub reorg_pool: ReorgPoolConfig,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            unconfirmed_pool: UnconfirmedPoolConfig::default(),
            orphan_pool: OrphanPoolConfig::default(),
            pending_pool: PendingPoolConfig::default(),
            reorg_pool: ReorgPoolConfig::default(),
        }
    }
}

impl NetworkConfigPath for MempoolConfig {
    fn main_key_prefix() -> &'static str {
        "mempool"
    }
}

/// Configuration for the MempoolService.
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct MempoolServiceConfig {
    /// The allocated waiting time for a request waiting for service responses from the Mempools of remote Base nodes.
    #[serde(with = "seconds")]
    pub request_timeout: Duration,
}

impl Default for MempoolServiceConfig {
    fn default() -> Self {
        Self {
            request_timeout: consts::MEMPOOL_SERVICE_REQUEST_TIMEOUT,
        }
    }
}

impl NetworkConfigPath for MempoolServiceConfig {
    fn main_key_prefix() -> &'static str {
        "mempool_service"
    }
}
