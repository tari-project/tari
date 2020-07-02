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

// The selected horizon block height might be similar to other pruned nodes resulting in spent UTXOs being discarded
// before the horizon sync has completed. A height offset is used to help with this problem by selecting a future height
// after the current horizon block height.
const HORIZON_SYNC_HEIGHT_OFFSET: u64 = 50;
// The maximum number of retry attempts a node can perform a request from remote nodes.
const MAX_SYNC_REQUEST_RETRY_ATTEMPTS: usize = 3;
const MAX_HEADER_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_MMR_NODE_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_KERNEL_REQUEST_RETRY_ATTEMPTS: usize = 5;
const MAX_TXO_REQUEST_RETRY_ATTEMPTS: usize = 5;
// The number of headers that can be requested in a single query.
const HEADER_REQUEST_SIZE: usize = 100;
// The number of MMR nodes or UTXOs that can be requested in a single query.
const MMR_NODE_OR_UTXO_REQUEST_SIZE: usize = 1000;

/// Configuration for the Horizon State Synchronization.
#[derive(Clone, Copy)]
pub struct HorizonSyncConfig {
    pub horizon_sync_height_offset: u64,
    pub max_sync_request_retry_attempts: usize,
    pub max_header_request_retry_attempts: usize,
    pub max_mmr_node_request_retry_attempts: usize,
    pub max_kernel_request_retry_attempts: usize,
    pub max_txo_request_retry_attempts: usize,
    pub header_request_size: usize,
    pub mmr_node_or_utxo_request_size: usize,
}

impl Default for HorizonSyncConfig {
    fn default() -> Self {
        Self {
            horizon_sync_height_offset: HORIZON_SYNC_HEIGHT_OFFSET,
            max_sync_request_retry_attempts: MAX_SYNC_REQUEST_RETRY_ATTEMPTS,
            max_header_request_retry_attempts: MAX_HEADER_REQUEST_RETRY_ATTEMPTS,
            max_mmr_node_request_retry_attempts: MAX_MMR_NODE_REQUEST_RETRY_ATTEMPTS,
            max_kernel_request_retry_attempts: MAX_KERNEL_REQUEST_RETRY_ATTEMPTS,
            max_txo_request_retry_attempts: MAX_TXO_REQUEST_RETRY_ATTEMPTS,
            header_request_size: HEADER_REQUEST_SIZE,
            mmr_node_or_utxo_request_size: MMR_NODE_OR_UTXO_REQUEST_SIZE,
        }
    }
}
