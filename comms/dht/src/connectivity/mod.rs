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

#[cfg(test)]
mod test;

mod metrics;
pub use metrics::{MetricsCollector, MetricsCollectorHandle};

use crate::{connectivity::metrics::MetricsError, event::DhtEvent, DhtActorError, DhtConfig, DhtRequester};
use chrono::MIN_DATETIME;
use futures::{stream::Fuse, StreamExt};
use log::*;
use std::{sync::Arc, time::Instant};
use tari_comms::{
    connectivity::{ConnectivityError, ConnectivityEvent, ConnectivityEventRx, ConnectivityRequester},
    peer_manager::{NodeDistance, NodeId, Peer, PeerManagerError, PeerQuery, PeerQuerySortBy, XorDistance},
    NodeIdentity,
    PeerConnection,
    PeerManager,
};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::{sync::broadcast, task, task::JoinHandle, time};

const LOG_TARGET: &str = "comms::dht::connectivity";

#[derive(Debug, Error)]
pub enum DhtConnectivityError {
    #[error("ConnectivityError: {0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Failed to send network Join message: {0}")]
    SendJoinFailed(#[from] DhtActorError),
    #[error("Metrics error: {0}")]
    MetricError(#[from] MetricsError),
}

/// # DHT Connectivity Actor
///
/// Responsible for ensuring DHT network connectivity to a neighbouring and random peer set. This includes joining the
/// network when the node has established some peer connections (e.g to seed peers). It maintains neighbouring and
/// random peer pools and instructs the comms `ConnectivityManager` to establish those connections. Once a configured
/// percentage of these peers is online, the node is established on the DHT network.
///
/// The DHT connectivity actor monitors the connectivity state (using `ConnectivityEvent`s) and attempts
/// to maintain connectivity to the network as peers come and go.
pub struct DhtConnectivity {
    config: DhtConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    connectivity: ConnectivityRequester,
    dht_requester: DhtRequester,
    peer_buckets: Vec<Vec<NodeId>>,
    peer_buckets_last_refresh: Option<Instant>,
    stats: Stats,
    dht_events: Fuse<broadcast::Receiver<Arc<DhtEvent>>>,
    metrics_collector: MetricsCollectorHandle,
    shutdown_signal: Option<ShutdownSignal>,
}

impl DhtConnectivity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        connectivity: ConnectivityRequester,
        dht_requester: DhtRequester,
        dht_events: broadcast::Receiver<Arc<DhtEvent>>,
        metrics_collector: MetricsCollectorHandle,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let mut peer_buckets = Vec::with_capacity(config.num_network_buckets as usize);
        for _i in 0..(config.num_network_buckets + 1) as usize {
            peer_buckets.push(vec![]);
        }
        Self {
            peer_buckets,
            config,
            peer_manager,
            node_identity,
            connectivity,
            dht_requester,
            metrics_collector,
            peer_buckets_last_refresh: None,
            stats: Stats::new(),
            dht_events: dht_events.fuse(),
            shutdown_signal: Some(shutdown_signal),
        }
    }

    /// Spawn a DhtConnectivity actor. This will immediately subscribe to the connection manager event stream to
    /// prevent unexpected missed events.
    pub fn spawn(mut self) -> JoinHandle<Result<(), DhtConnectivityError>> {
        // Listen to events as early as possible
        let connectivity_events = self.connectivity.get_event_subscription();
        task::spawn(async move {
            debug!(target: LOG_TARGET, "Waiting for connectivity manager to start");
            let _ = self.connectivity.wait_started().await;
            match self.run(connectivity_events).await {
                Ok(_) => Ok(()),
                Err(err) => {
                    error!(target: LOG_TARGET, "DhtConnectivity exited with error: {:?}", err);
                    Err(err)
                },
            }
        })
    }

    pub async fn run(mut self, connectivity_events: ConnectivityEventRx) -> Result<(), DhtConnectivityError> {
        let mut connectivity_events = connectivity_events.fuse();
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("DhtConnectivity initialized without a shutdown_signal");

        debug!(target: LOG_TARGET, "DHT connectivity starting");
        self.refresh_peer_pools().await?;

        let mut ticker = time::interval(self.config.connectivity_update_interval).fuse();

        loop {
            futures::select! {
                event = connectivity_events.select_next_some() => {
                    if let Ok(event) = event {
                        if let Err(err) = self.handle_connectivity_event(&event).await {
                            debug!(target: LOG_TARGET, "Error handling connectivity event: {:?}", err);
                        }
                    }
               },

               event = self.dht_events.select_next_some() => {
                   if let Ok(event) = event {
                        if let Err(err) = self.handle_dht_event(&event).await {
                            debug!(target: LOG_TARGET, "Error handling DHT event: {:?}", err);
                        }
                   }
               },

               _ = ticker.next() => {
                    if let Err(err) = self.refresh_peer_pool_if_required().await {
                        debug!(target: LOG_TARGET, "Error refreshing random peer pool: {:?}", err);
                    }
                    if let Err(err) = self.check_and_ban_flooding_peers().await {
                        debug!(target: LOG_TARGET, "Error checking for peer flooding: {:?}", err);
                    }
               },

               _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "DhtConnectivity shutting down because the shutdown signal was received");
                    break;
               }
            }
        }

        Ok(())
    }

    async fn handle_dht_event(&mut self, event: &DhtEvent) -> Result<(), DhtConnectivityError> {
        #[allow(clippy::single_match)]
        match event {
            DhtEvent::NetworkDiscoveryPeersAdded(info) => {
                if info.has_new_neighbours() {
                    debug!(
                        target: LOG_TARGET,
                        "Network discovery discovered {} more neighbouring peers. Reinitializing pools",
                        info.num_new_peers
                    );
                    self.refresh_peer_pools().await?;
                }
            },
            _ => {},
        }

        Ok(())
    }

    async fn check_and_ban_flooding_peers(&mut self) -> Result<(), DhtConnectivityError> {
        let nodes = self
            .metrics_collector
            .get_message_rates_exceeding(self.config.flood_ban_max_msg_count, self.config.flood_ban_timespan)
            .await?;

        for (peer, mps) in nodes {
            warn!(
                target: LOG_TARGET,
                "Banning peer `{}` because of flooding. Message rate: {:.2}m/s", peer, mps
            );
            self.connectivity
                .ban_peer_until(
                    peer,
                    self.config.ban_duration,
                    "Exceeded maximum message rate".to_string(),
                )
                .await?;
        }
        Ok(())
    }

    async fn refresh_peer_pools(&mut self) -> Result<(), DhtConnectivityError> {
        let buckets = XorDistance::get_buckets(self.config.num_network_buckets);
        self.refresh_peer_bucket(0, buckets[0].0, buckets[0].1, self.config.num_nodes_in_home_bucket)
            .await?;
        for (i, b) in buckets.iter().enumerate().skip(1) {
            self.refresh_peer_bucket(i, b.0, b.1, self.config.num_nodes_in_other_buckets)
                .await?;
        }

        self.peer_buckets_last_refresh = Some(Instant::now());

        Ok(())
    }

    async fn refresh_peer_bucket(
        &mut self,
        bucket_number: usize,
        min_distance: NodeDistance,
        max_distance: NodeDistance,
        num_nodes: usize,
    ) -> Result<(), DhtConnectivityError>
    {
        let mut new_neighbours = self
            .fetch_peers_in_bucket(num_nodes, min_distance, max_distance, &[])
            .await?;

        let (intersection, difference) = self.peer_buckets[bucket_number]
            .iter()
            .cloned()
            .partition::<Vec<_>, _>(|n| new_neighbours.contains(n));
        // Only retain the peers that aren't already added
        new_neighbours.retain(|n| !intersection.contains(&n));
        self.peer_buckets[bucket_number].retain(|n| intersection.contains(&n));

        info!(
            target: LOG_TARGET,
            "Adding {} peer(s) to bucket {}, removing {} peers",
            new_neighbours.len(),
            bucket_number,
            difference.len()
        );

        for peer in difference {
            self.connectivity.remove_peer(peer).await?;
        }
        for peer in new_neighbours {
            self.insert_peer_into_bucket(peer, bucket_number, num_nodes).await?;
        }
        Ok(())
    }

    async fn refresh_peer_pool_if_required(&mut self) -> Result<(), DhtConnectivityError> {
        let should_refresh = self
            .peer_buckets_last_refresh
            .map(|instant| instant.elapsed() >= self.config.connectivity_peer_buckets_refresh)
            .unwrap_or(true);
        if should_refresh {
            self.refresh_peer_pools().await?;
        }

        Ok(())
    }

    async fn handle_new_peer_connected(&mut self, conn: &PeerConnection) -> Result<(), DhtConnectivityError> {
        if conn.peer_features().is_client() {
            debug!(
                target: LOG_TARGET,
                "Client node '{}' connected",
                conn.peer_node_id().short_str()
            );
            return Ok(());
        }

        let current_dist = conn.peer_node_id().distance(self.node_identity.node_id());
        let bucket = current_dist.get_bucket(self.config.num_network_buckets);
        debug!(
            target: LOG_TARGET,
            "Peer '{}' connected. Adding to peer bucket {}.",
            conn.peer_node_id().short_str(),
            bucket.2
        );

        let bucket_size = if bucket.2 == 0 {
            self.config.num_nodes_in_home_bucket
        } else {
            self.config.num_nodes_in_other_buckets
        };

        self.insert_peer_into_bucket(conn.peer_node_id().clone(), bucket.2 as usize, bucket_size)
            .await?;

        Ok(())
    }

    async fn handle_connectivity_event(&mut self, event: &ConnectivityEvent) -> Result<(), DhtConnectivityError> {
        use ConnectivityEvent::*;
        debug!(target: LOG_TARGET, "Handling connectivity event:{}", event);
        match event {
            PeerConnected(conn) => {
                self.handle_new_peer_connected(conn).await?;
            },
            ManagedPeerDisconnected(node_id) |
            ManagedPeerConnectFailed(node_id) |
            PeerOffline(node_id) |
            PeerBanned(node_id) => {
                if self.metrics_collector.clear_metrics(node_id.clone()).await.is_err() {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to clear metrics for peer `{}`. Metric collector is shut down.", node_id
                    );
                };
                self.replace_managed_peer(node_id).await?;
            },
            ConnectivityStateOnline(n) => {
                if self.should_send_join() {
                    debug!(
                        target: LOG_TARGET,
                        "Node is online ({} peer(s) connected). Sending announce.", n
                    );
                    self.dht_requester
                        .send_join()
                        .await
                        .map_err(DhtConnectivityError::SendJoinFailed)?;

                    self.stats.mark_join_sent();
                }
            },
            ConnectivityStateOffline => {
                self.refresh_peer_pools().await?;
            },
            _ => {},
        }

        Ok(())
    }

    async fn replace_managed_peer(&mut self, current_peer: &NodeId) -> Result<(), DhtConnectivityError> {
        debug!(target: LOG_TARGET, "Replacing managed peer: {}", current_peer);
        let bucket = current_peer
            .distance(self.node_identity.node_id())
            .get_bucket(self.config.num_network_buckets);

        let bucket_num = bucket.2 as usize;

        if self.peer_buckets[bucket_num].contains(current_peer) {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' in bucket {} is offline. Adding a new peer if possible", current_peer, bucket_num
            );
            let exclude = &self.peer_buckets[bucket_num];
            match self.fetch_peers_in_bucket(1, bucket.0, bucket.1, &exclude).await?.pop() {
                Some(node_id) => {
                    if let Some(pos) = self.peer_buckets[bucket_num].iter().position(|n| n == current_peer) {
                        self.peer_buckets[bucket_num].remove(pos);
                    }
                    self.peer_buckets[bucket_num].push(node_id.clone());
                    self.connectivity.remove_peer(current_peer.clone()).await?;
                },
                None => {
                    warn!(
                        target: LOG_TARGET,
                        "Unable to fetch new peer to replace disconnected peer '{}' in bucket {} because not enough \
                         peers are known. Peer bucket size is {}.",
                        current_peer,
                        bucket_num,
                        self.peer_buckets[bucket_num].len()
                    );
                },
            }
        }

        Ok(())
    }

    async fn insert_peer_into_bucket(
        &mut self,
        node_id: NodeId,
        bucket_number: usize,
        bucket_size: usize,
    ) -> Result<(), ConnectivityError>
    {
        if self.peer_buckets[bucket_number].len() + 1 > bucket_size {
            // Sort by last seed to remove the least connected peer
            let mut peer_last_connected: Vec<(usize, Peer)> = vec![];
            for (index, p) in self.peer_buckets[bucket_number].iter().enumerate() {
                peer_last_connected.push((index, self.peer_manager.find_by_node_id(p).await?));
            }

            peer_last_connected.sort_by(|peer_a, peer_b| {
                peer_a
                    .1
                    .last_seen()
                    .unwrap_or(MIN_DATETIME)
                    .cmp(&peer_b.1.last_seen().unwrap_or(MIN_DATETIME))
            });

            if let Some((removed_index, removed_peer)) = peer_last_connected.pop() {
                self.peer_buckets[bucket_number].remove(removed_index);
                info!(
                    target: LOG_TARGET,
                    "Removing peer {} from bucket {} because it is full ({} slots used)",
                    removed_peer.node_id,
                    bucket_number,
                    bucket_size
                );
                self.connectivity.remove_peer(removed_peer.node_id).await?;
            }
        };
        info!(
            target: LOG_TARGET,
            "Adding peer {} to bucket {} ({}/{} slots used)",
            node_id,
            bucket_number,
            self.peer_buckets[bucket_number].len() + 1,
            bucket_size
        );
        self.peer_buckets[bucket_number].push(node_id.clone());
        self.connectivity.add_managed_peers(vec![node_id]).await?;
        Ok(())
    }

    async fn fetch_peers_in_bucket(
        &self,
        n: usize,
        min_distance: NodeDistance,
        max_distance: NodeDistance,
        excluded: &[NodeId],
    ) -> Result<Vec<NodeId>, DhtConnectivityError>
    {
        let peer_manager = &self.peer_manager;
        let node_id = self.node_identity.node_id();
        // Fetch to all n nearest neighbour Communication Nodes
        // which are eligible for connection.
        // Currently that means:
        // - The peer isn't banned,
        // - it has the required features
        // - it didn't recently fail to connect, and
        // - it is not in the exclusion list in closest_request
        let mut connect_ineligable_count = 0;
        let mut banned_count = 0;
        let mut excluded_count = 0;
        let mut filtered_out_node_count = 0;
        let mut not_in_bucket = 0;
        let query = PeerQuery::new()
            .select_where(|peer| {
                let distance = peer.node_id.distance(node_id);
                let is_in_bucket = distance < max_distance && distance >= min_distance;
                if !is_in_bucket {
                    not_in_bucket += 1;
                    return false;
                }

                if peer.is_banned() {
                    banned_count += 1;
                    return false;
                }

                if peer.features.is_client() {
                    filtered_out_node_count += 1;
                    return false;
                }

                if peer
                    .offline_since()
                    .map(|since| since <= self.config.offline_peer_cooldown)
                    .unwrap_or(false)
                {
                    connect_ineligable_count += 1;
                    return false;
                }

                let is_excluded = excluded.contains(&peer.node_id);
                if is_excluded {
                    excluded_count += 1;
                    return false;
                }

                true
            })
            .sort_by(PeerQuerySortBy::LastConnected)
            .limit(n);

        let peers = peer_manager.perform_query(query).await?;
        let total_excluded = banned_count + connect_ineligable_count + excluded_count + filtered_out_node_count;
        debug!(
            target: LOG_TARGET,
            "\n====================================\n Closest Peer Selection for bucket {min_distance} - \
             {max_distance} \n\n {num_peers} peer(s) selected\n {total} peer(s) were not selected \n\n {banned} \
             banned\n {filtered_out} not communication node\n {not_connectable} are not connectable\n {excluded} \
             explicitly excluded \n {not_in_bucket} not in range\n====================================\n",
            min_distance = min_distance,
            max_distance = max_distance,
            num_peers = peers.len(),
            total = total_excluded,
            banned = banned_count,
            filtered_out = filtered_out_node_count,
            not_connectable = connect_ineligable_count,
            excluded = excluded_count,
            not_in_bucket = not_in_bucket
        );

        Ok(peers.into_iter().map(|p| p.node_id).collect())
    }

    fn should_send_join(&self) -> bool {
        if !self.config.auto_join {
            return false;
        }
        let cooldown = self.config.join_cooldown_interval;
        self.stats
            .join_last_sent_at()
            .map(|at| at.elapsed() > cooldown)
            .unwrap_or(true)
    }
}

/// Basic connectivity stats. Right now, it is only used to track the last time a join message was sent to prevent the
/// node spamming the network if local connectivity changes.
#[derive(Debug, Default)]
struct Stats {
    join_last_sent_at: Option<Instant>,
}

impl Stats {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn join_last_sent_at(&self) -> Option<Instant> {
        self.join_last_sent_at
    }

    pub fn mark_join_sent(&mut self) {
        self.join_last_sent_at = Some(Instant::now());
    }
}
