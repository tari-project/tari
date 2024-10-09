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
    time::{Duration, Instant},
};

use tari_comms::peer_manager::Peer;
use tari_utilities::hex::Hex;

use crate::connectivity_service::WalletConnectivityError;

/// The selected peer is a current base node and an optional list of backup peers.
#[derive(Clone)]
pub struct BaseNodePeerManager {
    // The current base node that the wallet is connected to
    current_peer_index: usize,
    // The other base nodes that the wallet can connect to if the selected peer is not available
    peer_list: Vec<Peer>,
    last_connection_attempt: Option<LastConnectionAttempt>,
}

#[derive(Clone, Debug)]
pub struct LastConnectionAttempt {
    pub peer_index: usize,
    pub attempt_time: Instant,
}

impl BaseNodePeerManager {
    /// Create a new BaseNodePeerManager, with the preferred peer index and a list of peers.
    pub fn new(preferred_peer_index: usize, peer_list: Vec<Peer>) -> Result<Self, WalletConnectivityError> {
        if preferred_peer_index >= peer_list.len() {
            return Err(WalletConnectivityError::PeerIndexOutOfBounds(format!(
                "Preferred index: {}, Max index: {}",
                preferred_peer_index,
                peer_list.len() - 1
            )));
        }
        Ok(Self {
            current_peer_index: preferred_peer_index,
            peer_list,
            last_connection_attempt: None,
        })
    }

    /// Get the current peer
    pub fn get_current_peer(&self) -> Peer {
        self.peer_list
            .get(self.current_peer_index)
            .cloned()
            .unwrap_or(self.peer_list[0].clone())
    }

    /// Get the next peer in the list
    pub fn get_next_peer(&mut self) -> Peer {
        self.current_peer_index = (self.current_peer_index + 1) % self.peer_list.len();
        self.peer_list[self.current_peer_index].clone()
    }

    /// Get the base node peer manager state
    pub fn get_state(&self) -> (usize, Vec<Peer>) {
        (self.current_peer_index, self.peer_list.clone())
    }

    /// Set the last connection attempt stats
    pub fn set_last_connection_attempt(&mut self) {
        self.last_connection_attempt = Some(LastConnectionAttempt {
            peer_index: self.current_peer_index,
            attempt_time: Instant::now(),
        })
    }

    /// Get the last connection attempt stats
    pub fn get_last_connection_attempt(&self) -> Option<Duration> {
        if let Some(stats) = self.last_connection_attempt.clone() {
            if stats.peer_index == self.current_peer_index {
                Some(stats.attempt_time.elapsed())
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Display for BaseNodePeerManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let last_connection_attempt = match self.get_last_connection_attempt() {
            Some(stats) => format!("{:?}", stats.as_secs()),
            None => "Never".to_string(),
        };
        write!(
            f,
            "BaseNodePeerManager {{ current index: {}, last attempt (s): {}, peer list: {:?} }}",
            self.current_peer_index,
            last_connection_attempt,
            self.peer_list
                .iter()
                .map(|p| (p.node_id.to_hex(), p.public_key.to_hex()))
                .collect::<Vec<_>>()
        )
    }
}
