//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::collections::{HashMap, HashSet};

use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
use rand::seq::IteratorRandom;

use crate::Peer;

#[derive(Debug, Clone, Default)]
pub struct RelayState {
    selected_relay: Option<RelayPeer>,
    possible_relays: HashMap<PeerId, HashSet<Multiaddr>>,
}

impl RelayState {
    pub fn new<I: IntoIterator<Item = Peer>>(known_relays: I) -> Self {
        Self {
            selected_relay: None,
            possible_relays: {
                let mut hm = HashMap::<_, HashSet<_>>::new();
                for peer in known_relays {
                    let peer_id = peer.public_key.to_peer_id();
                    for addr in peer.addresses {
                        // Ensure that the /p2p/xxx protocol is present in the address
                        let addr = if addr.iter().any(|p| matches!(p, Protocol::P2p(_))) {
                            addr
                        } else {
                            addr.with(Protocol::P2p(peer_id))
                        };
                        hm.entry(peer_id).or_default().insert(addr);
                    }
                }
                hm
            },
        }
    }

    pub fn selected_relay(&self) -> Option<&RelayPeer> {
        self.selected_relay.as_ref()
    }

    pub fn selected_relay_mut(&mut self) -> Option<&mut RelayPeer> {
        self.selected_relay.as_mut()
    }

    pub fn possible_relays(&self) -> impl Iterator<Item = (&PeerId, &HashSet<Multiaddr>)> {
        self.possible_relays.iter()
    }

    pub fn num_possible_relays(&self) -> usize {
        self.possible_relays.len()
    }

    pub fn has_active_relay(&self) -> bool {
        self.selected_relay
            .as_ref()
            .map(|r| r.is_circuit_established)
            .unwrap_or(false)
    }

    pub fn add_possible_relay(&mut self, peer: PeerId, address: Multiaddr) {
        self.possible_relays.entry(peer).or_default().insert(address);
    }

    pub fn clear_selected_relay(&mut self) {
        self.selected_relay = None;
    }

    pub fn select_random_relay(&mut self) {
        let Some((peer, addrs)) = self.possible_relays.iter().choose(&mut rand::thread_rng()) else {
            return;
        };
        self.selected_relay = Some(RelayPeer {
            peer_id: *peer,
            addresses: addrs.iter().cloned().collect(),
            is_circuit_established: false,
            dialled_address: None,
        });
    }
}

#[derive(Debug, Clone)]
pub struct RelayPeer {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,
    pub is_circuit_established: bool,
    pub dialled_address: Option<Multiaddr>,
}
