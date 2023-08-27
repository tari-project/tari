// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use taiji_comms::peer_manager::Peer;

#[derive(Debug)]
pub enum PeerType {
    BaseNode,
    ValidatorNode,
    Wallet,
    TokenWallet,
}

#[derive(Debug)]
pub struct PeerWithType {
    pub peer: Peer,
    pub peer_type: PeerType,
}

impl PeerWithType {
    /// Constructs a new peer with peer type
    pub fn new(peer: Peer, peer_type: PeerType) -> PeerWithType {
        PeerWithType { peer, peer_type }
    }
}
