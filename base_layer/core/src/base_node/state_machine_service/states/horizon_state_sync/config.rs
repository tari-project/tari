//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

/// Configuration for the Horizon State Synchronization.
#[derive(Clone, Copy)]
pub struct HorizonSyncConfig {
    /// The selected horizon block height might be similar to other pruned nodes resulting in spent UTXOs being
    /// discarded before the horizon sync has completed. A height offset is used to help with this problem by
    /// selecting a future height after the current horizon block height.
    pub horizon_sync_height_offset: u64,
    /// The maximum number of retry attempts a node can perform a request from remote nodes.
    pub max_sync_request_retry_attempts: usize,
    /// The maximum number of kernel MMR nodes and kernels that can be requested in a single query.
    pub max_kernel_mmr_node_request_size: usize,
    /// The maximum number of UTXO MMR nodes, range proof MMR nodes and UTXOs that can be requested in a single query.
    pub max_utxo_mmr_node_request_size: usize,
}

impl Default for HorizonSyncConfig {
    fn default() -> Self {
        Self {
            horizon_sync_height_offset: 50,
            max_sync_request_retry_attempts: 5,
            max_kernel_mmr_node_request_size: 1000,
            max_utxo_mmr_node_request_size: 1000,
        }
    }
}
