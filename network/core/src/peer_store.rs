// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::collections::{HashMap, HashSet};

use libp2p::PeerId;

use crate::{identity, Peer};

#[derive(Debug, Clone)]
struct PeerRecord {
    peer: Peer,
    _is_banned: bool,
}

#[derive(Debug)]
pub struct PeerStore {
    store: HashMap<PeerId, PeerRecord>,
    public_key_to_peer_id: HashMap<identity::PublicKey, PeerId>,
    _ban_list: HashSet<PeerId>,
}

impl PeerStore {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            public_key_to_peer_id: HashMap::new(),
            _ban_list: HashSet::new(),
        }
    }

    pub fn insert(&mut self, peer: Peer) {
        let peer_id = peer.peer_id();
        self.public_key_to_peer_id.insert(peer.public_key().clone(), peer_id);
        self.store.insert(peer_id, PeerRecord {
            peer,
            _is_banned: false,
        });
    }

    pub fn remove(&mut self, peer_id: &PeerId) -> Option<Peer> {
        if let Some(peer) = self.store.remove(peer_id).map(|rec| rec.peer) {
            self.public_key_to_peer_id.remove(peer.public_key());
            Some(peer)
        } else {
            None
        }
    }

    pub fn contains(&self, peer_id: &PeerId) -> bool {
        self.store.contains_key(peer_id)
    }
}
