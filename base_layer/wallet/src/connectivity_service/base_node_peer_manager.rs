//  Copyright 2021, The Tari Project
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

use std::{
    fmt::Display,
    sync::{atomic, atomic::AtomicUsize, Arc},
    time::{Duration, Instant},
};

use tari_network::{identity::PeerId, Peer};

use crate::connectivity_service::WalletConnectivityError;

/// The selected peer is a current base node and an optional list of backup peers.
#[derive(Clone)]
pub struct BaseNodePeerManager {
    // The current base node that the wallet is connected to
    current_peer_index: Arc<AtomicUsize>,
    // The other base nodes that the wallet can connect to if the selected peer is not available
    peer_list: Arc<Vec<Peer>>,
    local_last_connection_attempt: Option<Instant>,
}

impl BaseNodePeerManager {
    /// Create a new BaseNodePeerManager, with the preferred peer index and a list of peers.
    pub fn new(preferred_peer_index: usize, peer_list: Vec<Peer>) -> Result<Self, WalletConnectivityError> {
        if preferred_peer_index >= peer_list.len() {
            return Err(WalletConnectivityError::PeerIndexOutOfBounds(format!(
                "Preferred index: {}, Max index: {}",
                preferred_peer_index,
                peer_list
                    .len()
                    .checked_sub(1)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "<empty>".to_string())
            )));
        }
        Ok(Self {
            current_peer_index: Arc::new(AtomicUsize::new(preferred_peer_index)),
            peer_list: Arc::new(peer_list),
            local_last_connection_attempt: None,
        })
    }

    /// Get the current peer's PeerId
    pub fn get_current_peer_id(&self) -> PeerId {
        self.get_current_peer().peer_id()
    }

    pub fn select_next_peer_if_attempted(&mut self) -> &Peer {
        if self.time_since_last_connection_attempt().is_some() {
            self.select_next_peer();
        }
        self.get_current_peer()
    }

    /// Get the current peer.
    pub fn get_current_peer(&self) -> &Peer {
        self.peer_list
            .get(self.current_peer_index())
            // Panic: cannot panic because this instance cannot be constructed with an empty peer_list
            .unwrap_or(&self.peer_list[0])
    }

    /// Changes to the next peer in the list, returning that peer
    pub fn select_next_peer(&mut self) -> &Peer {
        self.set_current_peer_index((self.current_peer_index() + 1) % self.peer_list.len());
        if self.peer_list.len() > 1 {
            // Reset the last attempt since we've moved onto another peer
            self.local_last_connection_attempt = None;
        }
        &self.peer_list[self.current_peer_index()]
    }

    pub fn peer_list(&self) -> &[Peer] {
        &self.peer_list
    }

    /// Get the base node peer manager state
    pub fn get_state(&self) -> (usize, &[Peer]) {
        (self.current_peer_index(), &self.peer_list)
    }

    /// Set the last connection attempt stats
    pub fn set_last_connection_attempt(&mut self) {
        self.local_last_connection_attempt = Some(Instant::now());
    }

    /// Get the last connection attempt for the current peer
    pub fn time_since_last_connection_attempt(&self) -> Option<Duration> {
        self.local_last_connection_attempt.as_ref().map(|t| t.elapsed())
    }

    fn set_current_peer_index(&self, index: usize) {
        self.current_peer_index.store(index, atomic::Ordering::SeqCst);
    }

    fn current_peer_index(&self) -> usize {
        self.current_peer_index.load(atomic::Ordering::SeqCst)
    }
}

impl Display for BaseNodePeerManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let last_connection_attempt = match self.time_since_last_connection_attempt() {
            Some(stats) => format!("{:?}", stats.as_secs()),
            None => "Never".to_string(),
        };
        write!(
            f,
            "BaseNodePeerManager {{ current index: {}, last attempt (s): {}, peer list: {} entries }}",
            self.current_peer_index(),
            last_connection_attempt,
            self.peer_list.len()
        )
    }
}
