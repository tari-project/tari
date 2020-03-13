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

use derive_error::Error;
use futures::channel::oneshot::Sender as OneshotSender;
use rand::RngCore;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub type RequestKey = u64;

/// Generate a new random request key to uniquely identify a request and its corresponding responses.
pub fn generate_request_key<R>(rng: &mut R) -> RequestKey
where R: RngCore {
    rng.next_u64()
}

/// The WaitingRequest is used to link incoming responses to the original sent request. When enough responses have been
/// received or the request timeout has been received then the received responses are returned on the reply_tx.
#[derive(Debug)]
pub struct WaitingRequest<T, R> {
    pub(crate) reply_tx: Option<OneshotSender<T>>,
    pub(crate) received_responses: Vec<R>,
    pub(crate) desired_resp_count: usize,
}

#[derive(Debug, Error)]
pub enum WaitingRequestError {
    /// A problem has been encountered with the storage backend.
    #[error(non_std, no_from)]
    BackendError(String),
}

/// WaitingRequests is used to keep track of a set of WaitingRequests.
pub struct WaitingRequests<T, R> {
    requests: Arc<RwLock<HashMap<RequestKey, WaitingRequest<T, R>>>>,
}

impl<T, R> WaitingRequests<T, R> {
    /// Create a new set of waiting requests.
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a new waiting request.
    pub fn insert(&self, key: RequestKey, waiting_request: WaitingRequest<T, R>) -> Result<(), WaitingRequestError> {
        self.requests
            .write()
            .map_err(|e| WaitingRequestError::BackendError(e.to_string()))?
            .insert(key, waiting_request);
        Ok(())
    }

    /// Remove the waiting request corresponding to the provided key.
    pub fn remove(&self, key: RequestKey) -> Result<Option<WaitingRequest<T, R>>, WaitingRequestError> {
        Ok(self
            .requests
            .write()
            .map_err(|e| WaitingRequestError::BackendError(e.to_string()))?
            .remove(&key))
    }

    /// Check if a sufficient number of responses have been received to complete the waiting request. Also, add the
    /// newly received response to the specified waiting request.
    pub fn check_complete(
        &self,
        key: RequestKey,
        response: R,
    ) -> Result<Option<WaitingRequest<T, R>>, WaitingRequestError>
    {
        let mut lock = self
            .requests
            .write()
            .map_err(|e| WaitingRequestError::BackendError(e.to_string()))?;
        let mut finalize_request = false;
        if let Some(waiting_request) = lock.get_mut(&key) {
            waiting_request.received_responses.push(response);
            finalize_request = waiting_request.received_responses.len() >= waiting_request.desired_resp_count;
        }
        if finalize_request {
            return Ok(lock.remove(&key));
        }
        Ok(None)
    }
}

impl<T, R> Clone for WaitingRequests<T, R> {
    fn clone(&self) -> Self {
        Self {
            requests: self.requests.clone(),
        }
    }
}

impl<T, R> Default for WaitingRequests<T, R> {
    fn default() -> Self {
        WaitingRequests::new()
    }
}
