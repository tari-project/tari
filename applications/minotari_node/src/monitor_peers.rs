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

use std::collections::HashMap;

use log::*;
use tari_comms::{connection_manager::ConnectionDirection, peer_manager::NodeId, CommsNode, Minimized, PeerConnection};
use tari_p2p::services::liveness::{error::LivenessError, LivenessEvent, LivenessHandle};
use tari_shutdown::Shutdown;
use tokio::{
    sync::broadcast::error::RecvError,
    time::{self, Duration},
};

const LOG_TARGET: &str = "minotari::base_node::monitor_peers";

use std::collections::VecDeque;

pub struct PeerLiveness<T, const MAX_SIZE: usize> {
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

/// Monitor the liveness of outbound peer connections and disconnect those that do not respond to pings consecutively.
/// The intent of the interval timer is to be significantly longer than the rate at which metadata is requested from
/// peers.
#[allow(clippy::too_many_lines)]
pub async fn monitor_peers(
    comms: CommsNode,
    mut liveness_handle: LivenessHandle,
    shutdown: Shutdown,
    metadata_auto_ping_interval: Duration,
) -> Result<(), LivenessError> {
    let mut interval_timer = time::interval(metadata_auto_ping_interval * 10);
    let mut shutdown_signal = shutdown.to_signal();

    let mut peer_liveness_stats: HashMap<NodeId, PeerLiveness<Stats, 7>> = HashMap::new();

    let mut loop_count = 1u64;
    loop {
        tokio::select! {
            biased;
            _ = shutdown_signal.wait() => {
                break;
            }

            _ = interval_timer.tick() => {
                let active_connections = comms.connectivity().get_active_connections().await?;
                let mut active_peer_connections = active_connections
                    .iter()
                    .filter(|p|p.peer_features().is_node() && p.direction() == ConnectionDirection::Outbound)
                    .collect::<Vec<_>>();
                if active_peer_connections.is_empty() {
                    continue;
                }
                let active_peer_node_ids = active_peer_connections
                    .iter()
                    .map(|&p|p.peer_node_id().clone())
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

                let mut liveness_events = liveness_handle.get_event_stream();
                let mut expected_nonces = liveness_handle.send_pings(
                    active_peer_node_ids,
                    Duration::from_millis(100)
                ).await?;
                expected_nonces.sort();

                // Only listen for the expected pongs from the peers (ignore any other pongs)
                let mut received_nonces = Vec::new();
                let timeout_timer = time::sleep(metadata_auto_ping_interval);
                tokio::pin!(timeout_timer);
                loop {
                    tokio::select! {
                        biased;
                        _ = shutdown_signal.wait() => {
                            break;
                        }

                        event = liveness_events.recv() => {
                            let event_str = format!("{:?}", event);
                            match event {
                                Ok(arc_event) => {
                                    if let LivenessEvent::ReceivedPong(pong) = &*arc_event {
                                        if expected_nonces.contains(&pong.nonce) {
                                            received_nonces.push(pong.nonce);
                                            received_nonces.sort();
                                        }
                                        if received_nonces == expected_nonces {
                                            break;
                                        }
                                    }
                                },
                                Err(RecvError::Closed) => {
                                    return Ok(());
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
                            break;
                        },
                    }
                }

                // Compare nonces and close connections for peers that did not respond multiple times
                update_stats_and_cull_unresponsive_connections(
                    &expected_nonces,
                    &received_nonces,
                    &mut active_peer_connections,
                    &mut peer_liveness_stats,
                    loop_count
                ).await?;
            },
        }
        loop_count += 1;
    }

    Ok(())
}

async fn update_stats_and_cull_unresponsive_connections(
    expected_nonces: &[u64],
    received_nonces: &[u64],
    active_peer_connections: &mut [&PeerConnection],
    peer_liveness_stats: &mut HashMap<NodeId, PeerLiveness<Stats, 7>>,
    loop_count: u64,
) -> Result<(), LivenessError> {
    if received_nonces != expected_nonces {
        trace!(
            target: LOG_TARGET,
            "Found {} of {} outbound base node peer connections that did not respond to pings",
            expected_nonces.len().saturating_sub(received_nonces.len()), active_peer_connections.len()
        );
    }
    for (i, &mut peer) in active_peer_connections.iter_mut().enumerate() {
        if received_nonces.contains(&expected_nonces[i]) {
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
                    debug!(
                        target: LOG_TARGET,
                        "Disconnecting {} as the peer is no longer responsive - \
                        (iter, conn, resp) {:?}",
                        peer.peer_node_id(),
                        stats.iter().map(|s|(s.loop_count, s.connected, s.responsive)).collect::<Vec<_>>(),
                    );
                    peer.clone().disconnect(Minimized::No).await?;
                    peer_liveness_stats.remove(peer.peer_node_id());
                } else {
                    trace!(
                        target: LOG_TARGET,
                        "Peer {} stats - (iter, conn, resp) {:?}",
                        peer.peer_node_id(),
                        stats.iter().map(|s|(s.loop_count, s.connected, s.responsive)).collect::<Vec<_>>(),
                    );
                }
            } else {
                warn!(target: LOG_TARGET, "Entry {} not in stats (check 3)!", peer.peer_node_id());
            }
        }
    }

    Ok(())
}
