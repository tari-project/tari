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
use std::{ops::Deref, time::Duration};

use futures::FutureExt;
use log::*;
use tari_comms_dht::event::DhtEvent;
use tokio::{sync::broadcast, time::sleep};

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
    #[allow(clippy::too_many_lines)]
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent {
        info!(target: LOG_TARGET, "Starting node.");

        // Check for DHT bootstrap completion first
        info!(target: LOG_TARGET, "[BN STARTING] Checking DHT bootstrap status before proceeding");
        let mut dht_events = shared.dht_event_stream.resubscribe();
        let bootstrap_timeout = sleep(Duration::from_secs(120));
        tokio::pin!(bootstrap_timeout);
        let mut timeout = bootstrap_timeout.fuse();
        // Check for any recent DHT bootstrap events
        let mut bootstrap_events_found = 0;
        loop {
            tokio::select! {
                result = dht_events.recv() => {
                    match result {
                        Ok(event_arc) => {
                            bootstrap_events_found += 1;
                            let event = event_arc.deref();
                            match event {
                                DhtEvent::BootstrapMethodDetermined(method) => {
                                    let method_str = format!("{}", method);
                                    info!(target: LOG_TARGET, "[BN STARTING] Found DHT BootstrapMethodDetermined event: {}", method_str);
                                    if method_str == "ExistingPeers" {
                                        info!(target: LOG_TARGET, "[BN STARTING] DHT bootstrap completed via ExistingPeers - marking primary bootstrap complete");
                                        shared.set_primary_bootstrap_complete(true);
                                        break;
                                    }
                                },
                                DhtEvent::PrimaryBootstrapComplete => {
                                    info!(target: LOG_TARGET, "[BN STARTING] Found DHT PrimaryBootstrapComplete event - marking primary bootstrap complete");
                                    shared.set_primary_bootstrap_complete(true);
                                    break;
                                },
                                _ => {},
                            }
                        },
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: LOG_TARGET, "[BN STARTING] DHT event stream lagged by {} events during startup", n);
                            // Continue processing in case there are still events available
                        },
                        Err(broadcast::error::RecvError::Closed) => {
                            warn!(target: LOG_TARGET, "[BN STARTING] DHT event stream closed during startup check");
                            break;
                        },
                    }
                }
                () = &mut timeout => {
                    warn!(target: LOG_TARGET, "[BN STARTING] Timeout while waiting for bootstrap events");
                    break;
                }
            }
        }

        info!(target: LOG_TARGET, "[BN STARTING] Processed {} DHT bootstrap events. Primary bootstrap complete: {}", bootstrap_events_found, shared.is_primary_bootstrap_complete);

        let mut network_silence_count = 0;
        loop {
            tokio::select! {
                metadata_result = shared.metadata_event_stream.recv() => {
                    match metadata_result.as_ref().map(|v| v.deref()) {
                        Ok(ChainMetadataEvent::NetworkSilence) => {
                            network_silence_count += 1;
                            debug!(target: LOG_TARGET, "NetworkSilence event received ({})", network_silence_count);
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
                },
                dht_result = dht_events.recv() => {
                    match dht_result {
                        Ok(event_arc) => {
                            let event = event_arc.deref();
                            match event {
                                DhtEvent::BootstrapMethodDetermined(method) => {
                                    let method_str = format!("{}", method);
                                    info!(target: LOG_TARGET, "[BN STARTING] Received live DHT BootstrapMethodDetermined event: {}", method_str);
                                    if method_str == "ExistingPeers" {
                                        info!(target: LOG_TARGET, "[BN STARTING] DHT bootstrap completed via ExistingPeers - marking primary bootstrap complete");
                                        shared.set_primary_bootstrap_complete(true);
                                    }
                                },
                                DhtEvent::PrimaryBootstrapComplete => {
                                    info!(target: LOG_TARGET, "[BN STARTING] Received live DHT PrimaryBootstrapComplete event - marking primary bootstrap complete");
                                    shared.set_primary_bootstrap_complete(true);
                                },
                                _ => {}
                            }
                        },
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: LOG_TARGET, "[BN STARTING] DHT event subscriber lagged by {} item(s)", n);
                        },
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!(target: LOG_TARGET, "DHT event subscriber closed");
                            // Continue with metadata events only
                        },
                    }
                }
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
