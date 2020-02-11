// Copyright 2019. The Tari Project
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

use crate::{base_node::states::SyncStatus, chain_storage::ChainMetadata};

use log::*;

/// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
pub fn determine_sync_mode(local: ChainMetadata, network: ChainMetadata, log_target: &str) -> SyncStatus {
    use crate::base_node::states::SyncStatus::*;
    match network.height_of_longest_chain {
        None => {
            info!(
                target: log_target,
                "The rest of the network doesn't appear to have any up-to-date chain data, so we're going to assume \
                 we're at the tip"
            );
            UpToDate
        },
        Some(network_tip) => {
            let local_tip = local.height_of_longest_chain.unwrap_or(0);
            if local_tip < network_tip {
                info!(
                    target: log_target,
                    "Our local blockchain history is a little behind that of the network. We're at block #{}, and the \
                     chain tip is at #{}",
                    local_tip,
                    network_tip
                );
                Lagging(network_tip)
            } else {
                UpToDate
            }
        },
    }
}
