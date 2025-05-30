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

use std::time::Duration;

use log::*;
use tari_comms::{connectivity::ConnectivityError, peer_manager::PeerFeatures};

use crate::network_discovery::state_machine::{NetworkDiscoveryContext, StateEvent};

const LOG_TARGET: &str = "comms::dht::network_discovery";

#[derive(Debug)]
pub(super) struct Initializing<'a> {
    context: &'a mut NetworkDiscoveryContext,
}

impl<'a> Initializing<'a> {
    pub fn new(context: &'a mut NetworkDiscoveryContext) -> Self {
        Self { context }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        let connectivity = &mut self.context.connectivity;
        debug!(target: LOG_TARGET, "Waiting for this node to come online...");
        while let Err(err) = connectivity.wait_for_connectivity(Duration::from_secs(10)).await {
            match err {
                ConnectivityError::OnlineWaitTimeout(_) => {
                    debug!(target: LOG_TARGET, "Still waiting for this node to come online...");
                },
                _ => {
                    error!(target: LOG_TARGET, "Connectivity error during initialization: {}. Discovery cannot proceed.", err);
                    return err.into();
                },
            }
        }

        // Initial discovery and refresh sync peers delay period, when a configured connection needs preference,
        // usually needed for the wallet to connect to its own base node first.
        if let Some(delay) = self.context.config.network_discovery.initial_peer_sync_delay {
            tokio::time::sleep(delay).await;
            debug!(target: LOG_TARGET, "Discovery starting after delayed for {:.0?}", delay);
        }

        // Get detailed peer count breakdown - query all peers
        let all_peers = match self.context.peer_manager.all(None).await {
            Ok(peers) => peers,
            Err(e) => {
                error!(target: LOG_TARGET, "Failed to query peer database: {}. Proceeding to bootstrap anyway.", e);
                vec![] // If DB query fails, assume no peers and proceed to bootstrap
            },
        };

        let mut total_peers = 0;
        let mut seed_peers = 0;
        let mut banned_peers = 0;
        let mut deleted_peers = 0;
        let mut offline_peers = 0;
        let mut failed_address_peers = 0;
        let mut non_communication_node_peers = 0;
        let mut suitable_peers = 0;

        for peer in &all_peers {
            total_peers += 1;

            if peer.is_seed() {
                seed_peers += 1;
            } else if peer.is_banned() {
                banned_peers += 1;
            } else if peer.deleted_at.is_some() {
                deleted_peers += 1;
            } else if peer.is_offline() {
                offline_peers += 1;
            } else if peer.all_addresses_failed() {
                failed_address_peers += 1;
            } else if peer.features != PeerFeatures::COMMUNICATION_NODE {
                non_communication_node_peers += 1;
            } else {
                suitable_peers += 1;
            }
        }

        let min_peers_for_bootstrap_skip = self.context.config.network_discovery.min_desired_peers;

        info!(
            target: LOG_TARGET,
            "BOOTSTRAP DECISION: Peer DB analysis - Total: {}, Seeds: {}, Banned: {}, Deleted: {}, Offline: {}, Failed addresses: {}, Non-comm nodes: {}, Suitable: {} (min required: {})",
            total_peers, seed_peers, banned_peers, deleted_peers, offline_peers, failed_address_peers, non_communication_node_peers, suitable_peers, min_peers_for_bootstrap_skip
        );

        if suitable_peers >= min_peers_for_bootstrap_skip {
            info!(
                target: LOG_TARGET,
                "BOOTSTRAP DECISION: Skipping SeedStrap - found {} suitable peers (>= {} required)",
                suitable_peers,
                min_peers_for_bootstrap_skip
            );
            StateEvent::InitialPeersSufficient
        } else {
            info!(
                target: LOG_TARGET,
                "BOOTSTRAP DECISION: Starting SeedStrap - found {} suitable peers (< {} required)",
                suitable_peers,
                min_peers_for_bootstrap_skip
            );
            debug!(target: LOG_TARGET, "Node is online. Starting network discovery (will proceed to SeedStrap)");
            StateEvent::Initialized
        }
    }
}
