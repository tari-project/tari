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

use std::ops::Deref;

use log::*;
use tokio::sync::broadcast;

use crate::{
    base_node::{
        chain_metadata_service::ChainMetadataEvent,
        state_machine_service::{
            states::{listening::Listening, StateEvent},
            BaseNodeStateMachine,
        },
    },
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::starting_state";

// The data structure handling Base Node Startup
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Starting;

impl Starting {
    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Starting node.");

        let mut network_silence_count = 0;
        loop {
            let metadata_event = shared.metadata_event_stream.recv().await;
            match metadata_event.as_ref().map(|v| v.deref()) {
                Ok(ChainMetadataEvent::NetworkSilence) => {
                    network_silence_count += 1;
                    debug!("NetworkSilence event received ({})", network_silence_count);
                    if network_silence_count >= 3 {
                        return StateEvent::Initialized(true);
                    }
                },
                Ok(ChainMetadataEvent::PeerChainMetadataReceived(_)) => {
                    return StateEvent::Initialized(false);
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(target: LOG_TARGET, "Metadata event subscriber lagged by {} item(s)", n);
                },
                Err(broadcast::error::RecvError::Closed) => {
                    debug!(target: LOG_TARGET, "Metadata event subscriber closed");
                    break;
                },
            }
        }

        debug!(
            target: LOG_TARGET,
            "Event listener is complete because liveness metadata and timeout streams were closed"
        );
        StateEvent::UserQuit
    }
}

/// State management for Starting -> Listening. This state change occurs every time a node is restarted.
impl From<Starting> for Listening {
    fn from(_: Starting) -> Self {
        Default::default()
    }
}
