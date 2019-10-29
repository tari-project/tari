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

use crate::{
    base_node::{
        states::{StateEvent, StateEvent::FatalError},
        BaseNodeStateMachine,
    },
    chain_storage::BlockchainBackend,
};
use log::*;

const LOG_TARGET: &str = "base_node::fetching_horizon_state";

/// Local state used when synchronizing the node to the pruning horizon.
pub struct HorizonInfo {
    /// The block that we've synchronised to when exiting this state
    horizon_block: u64,
}

impl HorizonInfo {
    pub fn new(horizon_block: u64) -> Self {
        HorizonInfo { horizon_block }
    }

    pub async fn next_event<B: BlockchainBackend>(&mut self, _shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        debug!(
            target: LOG_TARGET,
            "Starting horizon synchronisation at block {}", self.horizon_block
        );

        info!(
            target: LOG_TARGET,
            "Synchronising kernel merkle mountain range to pruning horizon."
        );
        if let Err(e) = self.synchronize_kernel_mmr().await {
            return StateEvent::FatalError(format!("Synchronizing kernel MMR failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising range proof MMR to pruning horizon.");
        if let Err(e) = self.synchronize_range_proof_mmr().await {
            return StateEvent::FatalError(format!("Synchronizing range proof MMR failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising TXO MMR to pruning horizon.");
        if let Err(e) = self.synchronize_output_mmr().await {
            return StateEvent::FatalError(format!("Synchronizing output MMR failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising headers to pruning horizon.");
        if let Err(e) = self.synchronize_headers().await {
            return StateEvent::FatalError(format!("Synchronizing block headers failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising kernels to pruning horizon.");
        if let Err(e) = self.synchronize_kernels().await {
            return StateEvent::FatalError(format!("Synchronizing kernels failed. {}", e));
        }

        info!(target: LOG_TARGET, "Synchronising UTXO set at pruning horizon.");
        if let Err(e) = self.synchronize_utxo_set().await {
            return StateEvent::FatalError(format!("Synchronizing UTXO set failed. {}", e));
        }

        debug!(target: LOG_TARGET, "Pruning horizon state has synchronised");
        StateEvent::HorizonStateFetched
    }

    async fn synchronize_headers(&mut self) -> Result<(), String> {
        Err("unimplemented".into())
    }

    async fn synchronize_kernels(&mut self) -> Result<(), String> {
        Err("unimplemented".into())
    }

    async fn synchronize_utxo_set(&mut self) -> Result<(), String> {
        Err("unimplemented".into())
    }

    async fn synchronize_kernel_mmr(&mut self) -> Result<(), String> {
        Err("unimplemented".into())
    }

    async fn synchronize_range_proof_mmr(&mut self) -> Result<(), String> {
        Err("unimplemented".into())
    }

    async fn synchronize_output_mmr(&mut self) -> Result<(), String> {
        Err("unimplemented".into())
    }
}
