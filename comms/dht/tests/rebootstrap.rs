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
use tari_comms::{
    connectivity::ConnectivityEvent,
    peer_manager::{Peer, PeerFeatures},
};
use tari_comms_dht::SeedPeerProvider;
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

    // The connection itself may not last: a discovery round that syncs from node_B afterwards hangs up on it. What is
    // being tested is that node_A gets there, so watch for the connection being made rather than polling for it.
    let node_b_id = node_b.node_identity().node_id().clone();
    let mut events = node_a.comms.connectivity().get_event_subscription();
    time::timeout(Duration::from_secs(60), async {
        loop {
            if let ConnectivityEvent::PeerConnected(conn) = events.recv().await.unwrap() &&
                conn.peer_node_id() == &node_b_id &&
                conn.direction().is_outbound()
            {
                break;
            }
        }
    })
    .await
    .expect("node_A did not dial node_B");

    node_a.shutdown().await;
    seed.shutdown().await;
    node_b.shutdown().await;
}
