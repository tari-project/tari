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

use futures::channel::oneshot::Sender as OneshotSender;
use rand::RngCore;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::RwLock;

pub type RequestKey = u64;

/// Generate a new random request key to uniquely identify a request and its corresponding responses.
pub fn generate_request_key<R>(rng: &mut R) -> RequestKey
where R: RngCore {
    rng.next_u64()
}

/// WaitingRequests is used to keep track of a set of WaitingRequests.
pub struct WaitingRequests<T> {
    requests: Arc<RwLock<HashMap<RequestKey, Option<(OneshotSender<T>, Instant)>>>>,
}

impl<T> WaitingRequests<T> {
    /// Create a new set of waiting requests.
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a new waiting request.
    pub async fn insert(&self, key: RequestKey, reply_tx: OneshotSender<T>) {
        self.requests
            .write()
            .await
            .insert(key, Some((reply_tx, Instant::now())));
    }

    /// Remove the waiting request corresponding to the provided key.
    pub async fn remove(&self, key: RequestKey) -> Option<(OneshotSender<T>, Instant)> {
        self.requests.write().await.remove(&key).unwrap_or(None)
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
