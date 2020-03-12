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
        chain_metadata_service::ChainMetadataEvent,
        states::{helpers::determine_sync_mode, StateEvent, StateEvent::FatalError, SyncStatus},
        BaseNodeStateMachine,
    },
    chain_storage::BlockchainBackend,
};
use futures::{
    channel::mpsc::{channel, Sender},
    stream::StreamExt,
    SinkExt,
};
use log::*;
use std::time::{Duration, Instant};
use tokio::runtime;

const LOG_TARGET: &str = "c::bn::states::listening";

// The max duration that the listening state will wait between metadata events before it will start asking for metadata
// from remote nodes.
const LISTENING_SILENCE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Configuration for the Listening state.
#[derive(Clone, Copy, Debug)]
pub struct ListeningConfig {
    pub listening_silence_timeout: Duration,
}

impl Default for ListeningConfig {
    fn default() -> Self {
        Self {
            listening_silence_timeout: LISTENING_SILENCE_TIMEOUT,
        }
    }
}

/// This state listens for chain metadata events received from the liveness and chain metadata service. Based on the
/// received metadata, if it detects that the current node is lagging behind the network it will switch to block sync
/// state. If no metadata is received for a prolonged period of time it will transition to the initial sync state and
/// request chain metadata from remote nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct ListeningInfo;

impl ListeningInfo {
    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Listening for chain metadata updates");

        let mut metadata_event_stream = shared.metadata_event_stream.clone().fuse();
        let (timeout_event_sender, timeout_event_receiver) = channel(1);
        let mut timeout_event_receiver = timeout_event_receiver.fuse();

        // Create the initial timeout event
        spawn_timeout_event(
            &shared.executor,
            timeout_event_sender.clone(),
            shared.config.listening_config.listening_silence_timeout,
        );

        let mut last_event_time = Instant::now();
        loop {
            futures::select! {
                metadata_event = metadata_event_stream.select_next_some() => {
                    if let ChainMetadataEvent::PeerChainMetadataReceived(chain_metadata_list) = &*metadata_event {
                        if let Some(network) = chain_metadata_list.first() {
                            info!(target: LOG_TARGET, "Loading local blockchain metadata.");
                            let local = match shared.db.get_metadata() {
                                Ok(m) => m,
                                Err(e) => {
                                    let msg = format!("Could not get local blockchain metadata. {}", e.to_string());
                                    return FatalError(msg);
                                },
                            };

                            if let SyncStatus::Lagging = determine_sync_mode(&local, &network, LOG_TARGET) {
                                return StateEvent::FallenBehind(SyncStatus::Lagging);
                            }
                        }
                        last_event_time = Instant::now();
                    }
                },

                _ = timeout_event_receiver.select_next_some() => {
                    let timeout_time = Instant::now();
                    let time_difference = timeout_time.duration_since(last_event_time);
                    trace!(target: LOG_TARGET, "Timeout event: {}s since last chain metadata event.", time_difference.as_secs());
                    if time_difference >= shared.config.listening_config.listening_silence_timeout {
                        return StateEvent::NetworkSilence;
                    }

                    // Timeout was early, spawn an updated timeout with correct delay
                    let timeout_delay = shared.config.listening_config.listening_silence_timeout - time_difference;
                    spawn_timeout_event(&shared.executor, timeout_event_sender.clone(), timeout_delay);
                },

                complete => {
                    debug!(target: LOG_TARGET, "Event listener is complete because liveness metadata and timeout streams were closed");
                    return StateEvent::UserQuit;
                }
            }
        }
    }
}

// Spawn a timeout event that will respond on the timeout event sender once the specified time delay has been reached.
fn spawn_timeout_event(executor: &runtime::Handle, mut timeout_event_sender: Sender<()>, timeout: Duration) {
    executor.spawn(async move {
        tokio::time::delay_for(timeout).await;
        let _ = timeout_event_sender.send(()).await;
    });
}
