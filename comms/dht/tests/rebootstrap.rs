//  Copyright 2026, The Tari Project
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

mod harness;
use std::{sync::Arc, time::Duration};

use harness::*;
use tari_comms::peer_manager::{Peer, PeerFeatures};
use tari_comms_dht::{SeedPeerProvider, event::DhtEvent};
use tokio::time;

/// Stands in for DNS: the seed is only known through the provider, not through the peer database.
struct StaticSeeds(Vec<Peer>);

#[tari_comms::async_trait]
impl SeedPeerProvider for StaticSeeds {
    async fn resolve_seed_peers(&self) -> Vec<Peer> {
        self.0.clone()
    }
}

/// A node whose stored peers are all unreachable recovers to an outbound pool peer through a reachable seed, without
/// a restart: DhtConnectivity notices the starved pool, network discovery rebootstraps from the seed and the pool is
/// refilled with what the seed knows.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_isolated_node_recovers_through_a_reachable_seed() {
    // node_B is reachable, but only the seed knows about it
    let node_b = make_node("node_B", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    let seed = make_node(
        "seed",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_b.to_peer()),
    )
    .await;

    // Nothing listens on any of these addresses
    let stale_peers = (0..5)
        .map(|_| {
            let mut peer = make_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
            let addresses = peer.addresses.address_iter().cloned().collect::<Vec<_>>();
            for addr in &addresses {
                peer.addresses.mark_last_seen_now(addr);
            }
            peer
        })
        .collect::<Vec<_>>();

    let mut config = dht_config();
    config.network_discovery.enabled = true;
    config.connectivity.update_interval = Duration::from_millis(500);
    // Ticks are slowed down by the connectivity actor's 15s pause on going offline, so don't wait for three of them
    config.connectivity.pool_starved_ticks = 1;
    let node_a = make_node_with_seed_peer_provider(
        "node_A",
        make_node_identity(PeerFeatures::COMMUNICATION_NODE),
        config,
        stale_peers,
        Some(Arc::new(StaticSeeds(vec![seed.to_peer()]))),
    )
    .await;

    let node_b_id = node_b.node_identity().node_id().clone();
    let mut dht_events = node_a.dht.subscribe_dht_events();
    // Generous budgets: each phase can take ~45s locally, mostly DhtConnectivity's 15s pauses on going offline, and
    // more on a loaded CI machine.

    // node_A rebootstraps and learns about node_B from the seed
    time::timeout(Duration::from_secs(180), async {
        loop {
            if let DhtEvent::RebootstrapComplete(info) = &*dht_events.recv().await.unwrap() &&
                info.learned_peers.contains(&node_b_id)
            {
                break;
            }
        }
    })
    .await
    .expect("node_A did not learn about node_B through a rebootstrap");

    // ...and ends up with a live outbound connection to it. The DHT pool takes every outbound peer while it is short of
    // outbound peers, and nothing else in node_A hangs up on a pool peer, so a connection that is still up after
    // several pool refreshes is a pool peer.
    let mut connectivity = node_a.comms.connectivity();
    let mut connected_since = None;
    time::timeout(Duration::from_secs(180), async {
        loop {
            let is_connected =
                connectivity.get_active_connections().await.unwrap().iter().any(|conn| {
                    conn.peer_node_id() == &node_b_id && conn.direction().is_outbound() && conn.is_connected()
                });
            connected_since = if is_connected {
                connected_since.or_else(|| Some(time::Instant::now()))
            } else {
                None
            };
            if connected_since.is_some_and(|since| since.elapsed() >= Duration::from_secs(5)) {
                break;
            }
            time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("node_A did not keep an outbound connection to node_B");

    node_a.shutdown().await;
    seed.shutdown().await;
    node_b.shutdown().await;
}
