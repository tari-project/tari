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

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};
use tari_common_types::chain_metadata::ChainMetadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InProgressHorizonSyncState {
    pub metadata: ChainMetadata,
    pub initial_kernel_checkpoint_count: u64,
    pub initial_utxo_checkpoint_count: u64,
    pub initial_rangeproof_checkpoint_count: u64,
}

impl InProgressHorizonSyncState {
    pub fn new_with_metadata(metadata: ChainMetadata) -> Self {
        Self {
            metadata,
            initial_kernel_checkpoint_count: 0,
            initial_utxo_checkpoint_count: 0,
            initial_rangeproof_checkpoint_count: 0,
        }
    }
}

impl Display for InProgressHorizonSyncState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "metadata = {}, #kernel checkpoints = ({}), #UTXO checkpoints = ({}), #range proof checkpoints = ({})",
            self.metadata,
            self.initial_kernel_checkpoint_count,
            self.initial_utxo_checkpoint_count,
            self.initial_rangeproof_checkpoint_count,
        )
    }
}
