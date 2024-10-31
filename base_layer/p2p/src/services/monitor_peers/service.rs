//  Copyright 2022, The Tari Project
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

use std::collections::{HashMap, VecDeque};

use futures::pin_mut;
use log::*;
use tari_comms::{
    connection_manager::ConnectionDirection,
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    Minimized,
    PeerConnection,
};
use tari_shutdown::ShutdownSignal;
use tokio::{
    sync::broadcast::error::RecvError,
    time::{self, Duration},
};

use crate::services::{
    liveness::{LivenessEvent, LivenessHandle},
    monitor_peers::LOG_TARGET,
};

struct PeerLiveness<T, const MAX_SIZE: usize> {
    vec: VecDeque<T>,
}

impl<T, const MAX_SIZE: usize> PeerLiveness<T, MAX_SIZE> {
    pub fn new() -> Self {
        Self {
            vec: VecDeque::with_capacity(MAX_SIZE),
        }
    }

    pub fn push_pop(&mut self, item: T) {
        if self.vec.len() == MAX_SIZE {
            self.vec.pop_front();
        }
        self.vec.push_back(item);
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<T> {
        self.vec.iter()
    }
}

struct Stats {
    connected: bool,
    responsive: bool,
    loop_count: u64,
}

struct PeerPingPong {
    expected_nonce: u64,
    received_nonce: Option<u64>,
    node_id: NodeId,
}

pub struct MonitorPeersService {
    comms: ConnectivityRequester,
    liveness_handle: LivenessHandle,
    shutdown_signal: ShutdownSignal,
    auto_ping_interval: Duration,
}

impl MonitorPeersService {
    pub fn new(
        comms: ConnectivityRequester,
        liveness_handle: LivenessHandle,
        shutdown_signal: ShutdownSignal,
        auto_ping_interval: Duration,
    ) -> Self {
        Self {
            comms,
            liveness_handle,
            shutdown_signal,
            auto_ping_interval,
        }
    }

    /// Monitor the liveness of outbound peer connections and disconnect those that do not respond to pings
    /// consecutively. The intent of the interval timer is to be significantly longer than the rate at which
    /// metadata is requested from peers.
    #[allow(clippy::too_many_lines)]
    pub async fn run(mut self) {
        let mut interval_timer = time::interval(self.auto_ping_interval * 10);
        let liveness_events = self.liveness_handle.get_event_stream();
        pin_mut!(liveness_events);

        let mut peer_liveness_stats: HashMap<NodeId, PeerLiveness<Stats, 7>> = HashMap::new();

        let mut loop_count = 0u64;
        loop {
            loop_count += 1;
            tokio::select! {
                biased;
                _ = self.shutdown_signal.wait() => {
                    break;
                }

                _ = interval_timer.tick() => {
                    trace!(target: LOG_TARGET, "Starting monitor peers round (iter {})", loop_count);
                    let active_connections = match self.comms.get_active_connections().await {
                        Ok(val) => val,
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to get active connections ({})", e);
                            continue;
                        },
                    };
                    let mut active_peer_connections = active_connections
                        .iter()
                        .filter(|p|p.peer_features().is_node() && p.direction() == ConnectionDirection::Outbound)
                        .cloned()
                        .collect::<Vec<_>>();
                    if active_peer_connections.is_empty() {
                        trace!(target: LOG_TARGET, "No active connections found");
                        continue;
                    }
                    let active_peer_node_ids = active_peer_connections
                        .iter()
                        .map(|p|p.peer_node_id().clone())
                        .collect::<Vec<_>>();

                    let known_peer_connections = peer_liveness_stats.keys().cloned().collect::<Vec<_>>();
                    for peer_id in &known_peer_connections {
                        if !active_peer_node_ids.contains(peer_id) {
                            // Prior connections not connected now are considered inactive and unresponsive
                            peer_liveness_stats
                                .entry(peer_id.clone())
                                .and_modify(|item| item.push_pop(
                                    Stats {connected: false, responsive: false, loop_count}
                                ));
                        }
                    }
                    for peer_id in &active_peer_node_ids {
                        if !known_peer_connections.contains(peer_id) {
                            // New connections are considered active and responsive
                            peer_liveness_stats.insert( peer_id.clone(), PeerLiveness::new());
                        }
                    }

                    let mut peer_ping_pongs = match self.liveness_handle
                        .send_pings(active_peer_node_ids.clone())
                        .await
                    {
                        Ok(nonces) => active_peer_node_ids
                            .iter()
                            .zip(nonces.iter())
                            .map(|(node_id, &nonce)| PeerPingPong {
                                expected_nonce: nonce,
                                received_nonce: None,
                                node_id: node_id.clone(),
                            })
                            .collect::<Vec<_>>(),
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to send pings to peers ({})", e);
                            continue;
                        },
                    };

                    // Only listen for the expected pongs from the peers (ignore any other pongs)
                    let timeout_timer = time::sleep(self.auto_ping_interval);
                    tokio::pin!(timeout_timer);
                    loop {
                        tokio::select! {
                            biased;
                            _ = self.shutdown_signal.wait() => {
                                break;
                            }

                            event = liveness_events.recv() => {
                                let event_str = format!("{:?}", event);
                                match event {
                                    Ok(arc_event) => {
                                        if let LivenessEvent::ReceivedPong(pong) = &*arc_event {
                                            if let Some(ping_pong) = peer_ping_pongs.iter_mut().find(|p| p.expected_nonce == pong.nonce) {
                                                ping_pong.received_nonce = Some(pong.nonce);
                                            }
                                            if peer_ping_pongs.iter().all(|p| p.received_nonce.is_some()) {
                                                break;
                                            }
                                        }
                                    },
                                    Err(RecvError::Closed) => {
                                        return;
                                    },
                                    Err(ref e) => {
                                        debug!(
                                            target: LOG_TARGET,
                                            "Liveness event error: {:?} ({})",
                                            event_str, e.to_string()
                                        );
                                    },
                                }
                            },

                            _ = &mut timeout_timer => {
                                trace!(
                                    target: LOG_TARGET,
                                    "Timed out waiting for pongs, received {} of {} (iter  {})",
                                    peer_ping_pongs.iter().filter(|p| p.received_nonce.is_some()).count(),
                                    peer_ping_pongs.len(),
                                    loop_count
                                );
                                break;
                            },
                        }
                    }

                    // Compare nonces and close connections for peers that did not respond multiple times
                    update_stats_and_cull_unresponsive_connections(
                        &peer_ping_pongs,
                        &mut active_peer_connections,
                        &mut peer_liveness_stats,
                        loop_count
                    ).await;
                },
            }
        }
    }
}

async fn update_stats_and_cull_unresponsive_connections(
    peer_ping_pongs: &[PeerPingPong],
    active_peer_connections: &mut [PeerConnection],
    peer_liveness_stats: &mut HashMap<NodeId, PeerLiveness<Stats, 7>>,
    loop_count: u64,
) {
    let received_nonces_count = peer_ping_pongs.iter().filter(|p| p.received_nonce.is_some()).count();
    if received_nonces_count != peer_ping_pongs.len() {
        trace!(
            target: LOG_TARGET,
            "Found {} of {} outbound base node peer connections that did not respond to pings",
            peer_ping_pongs.len().saturating_sub(received_nonces_count), active_peer_connections.len()
        );
    }

    let mut disconnect_peers = Vec::new();
    for &mut ref peer in active_peer_connections.iter_mut() {
        if let Some(ping_pong) = peer_ping_pongs.iter().find(|p| &p.node_id == peer.peer_node_id()) {
            if ping_pong.received_nonce.is_some() {
                peer_liveness_stats
                    .entry(peer.peer_node_id().clone())
                    .and_modify(|item| {
                        item.push_pop(Stats {
                            connected: true,
                            responsive: true,
                            loop_count,
                        })
                    });
            } else {
                peer_liveness_stats
                    .entry(peer.peer_node_id().clone())
                    .and_modify(|item| {
                        item.push_pop(Stats {
                            connected: true,
                            responsive: false,
                            loop_count,
                        })
                    });
                if let Some(stats) = peer_liveness_stats.get(peer.peer_node_id()) {
                    // Evaluate the last 3 entries in the stats
                    if stats
                        .iter()
                        .rev()
                        .take(3)
                        .filter(|s| s.connected && !s.responsive)
                        .count() >=
                        3
                    {
                        disconnect_peers.push(peer.clone());
                    } else {
                        trace!(
                            target: LOG_TARGET,
                            "Peer {} stats - (iter, conn, resp) {:?}",
                            peer.peer_node_id(),
                            stats.iter().map(|s|(s.loop_count, s.connected, s.responsive)).collect::<Vec<_>>(),
                        );
                    }
                }
            }
        }
    }

    for peer in disconnect_peers {
        if let Some(stats) = peer_liveness_stats.get(peer.peer_node_id()) {
            debug!(
                target: LOG_TARGET,
                "Disconnecting {} as the peer is no longer responsive - (iter, conn, resp) {:?}",
                peer.peer_node_id(),
                stats.iter().map(|s|(s.loop_count, s.connected, s.responsive)).collect::<Vec<_>>(),
            );
            if let Err(e) = peer.clone().disconnect(Minimized::No).await {
                warn!(
                    target: LOG_TARGET,
                    "Error while attempting to disconnect peer {}: {}", peer.peer_node_id(), e
                );
            }
            peer_liveness_stats.remove(peer.peer_node_id());
            trace!(target: LOG_TARGET, "Disconnected {} (iter, {})", peer.peer_node_id(), loop_count);
        }
    }
}
