//  Copyright 2019 The Tari Project
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

use std::{collections::HashMap, sync::Arc, time::Instant};

use tari_comms::peer_manager::NodeId;
use tokio::sync::{RwLock, oneshot::Sender as OneshotSender};

use crate::common::RequestKey;
/// Generate a new random request key to uniquely identify a request and its corresponding responses.
pub fn generate_request_key<R>(rng: &mut R) -> RequestKey
where R: rand::Rng {
    rng.next_u64()
}

type WaitingEntry<T> = (OneshotSender<T>, Instant, Option<NodeId>);

/// WaitingRequests is used to keep track of a set of WaitingRequests.
#[allow(clippy::type_complexity)]
pub struct WaitingRequests<T> {
    requests: Arc<RwLock<HashMap<RequestKey, Option<WaitingEntry<T>>>>>,
}

impl<T> WaitingRequests<T> {
    /// Create a new set of waiting requests.
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a new waiting request without peer binding.
    pub async fn insert(&self, key: RequestKey, reply_tx: OneshotSender<T>) {
        self.insert_for_peer(key, None, reply_tx).await;
    }

    /// Insert a new waiting request optionally bound to a specific expected peer `NodeId`.
    pub async fn insert_for_peer(&self, key: RequestKey, expected_peer: Option<NodeId>, reply_tx: OneshotSender<T>) {
        self.requests
            .write()
            .await
            .insert(key, Some((reply_tx, Instant::now(), expected_peer)));
    }

    /// Remove the waiting request corresponding to the provided key regardless of peer.
    pub async fn remove(&self, key: RequestKey) -> Option<(OneshotSender<T>, Instant)> {
        self.requests
            .write()
            .await
            .remove(&key)
            .flatten()
            .map(|(tx, started, _)| (tx, started))
    }

    /// Remove the waiting request only if the responder matches the expected peer (when one was specified).
    pub async fn remove_for_peer(&self, key: RequestKey, responder: &NodeId) -> Option<(OneshotSender<T>, Instant)> {
        let mut guard = self.requests.write().await;
        let matches_peer = match guard.get(&key) {
            Some(Some((_, _, Some(expected_peer)))) => expected_peer == responder,
            Some(Some((_, _, None))) => true,
            _ => false,
        };
        if !matches_peer {
            return None;
        }
        guard.remove(&key).flatten().map(|(tx, started, _)| (tx, started))
    }
}

impl<T> Clone for WaitingRequests<T> {
    fn clone(&self) -> Self {
        Self {
            requests: self.requests.clone(),
        }
    }
}

impl<T> Default for WaitingRequests<T> {
    fn default() -> Self {
        WaitingRequests::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn remove_for_peer_rejects_mismatched_peer_and_preserves_entry() {
        let waiting = WaitingRequests::<u64>::new();
        let (tx, _rx) = oneshot::channel();
        let target_peer = NodeId::default();
        let other_peer = NodeId::from_key(&[1u8; 32]).unwrap();

        waiting.insert_for_peer(42, Some(target_peer.clone()), tx).await;
        assert!(waiting.remove_for_peer(42, &other_peer).await.is_none());
        assert!(waiting.remove_for_peer(42, &target_peer).await.is_some());
    }
}
