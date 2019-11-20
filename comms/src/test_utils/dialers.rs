// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    connection::PeerConnection,
    connection_manager::{ConnectionManagerError, Connectivity, Dialer},
    peer_manager::NodeId,
};
use futures::{future, Future};
use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// Count the number of dial attempts. Resolving into a test peer connection.
pub struct CountDialer<T> {
    count: Arc<AtomicUsize>,
    _t: PhantomData<T>,
}

impl<T> Clone for CountDialer<T> {
    fn clone(&self) -> Self {
        Self {
            count: Arc::clone(&self.count),
            _t: PhantomData,
        }
    }
}

impl<T> CountDialer<T> {
    pub fn new() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
            _t: PhantomData,
        }
    }

    pub fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl<T> Dialer<T> for CountDialer<T> {
    type Error = ConnectionManagerError;
    type Output = Arc<PeerConnection>;

    type Future = impl Future<Output = Result<Self::Output, Self::Error>> + Send + Unpin;

    fn dial(&self, _: &T) -> Self::Future {
        let (conn, _) = PeerConnection::new_with_connecting_state_for_test("127.0.0.1:0".parse().unwrap());
        self.count.fetch_add(1, Ordering::AcqRel);
        future::ready(Ok(Arc::new(conn)))
    }
}

impl<T> Connectivity for CountDialer<T> {
    fn get_connection(&self, _: &NodeId) -> Option<Arc<PeerConnection>> {
        None
    }

    fn get_active_connection_count(&self) -> usize {
        self.count()
    }

    fn disconnect_peer(&self, _: &NodeId) -> Result<Option<Arc<PeerConnection>>, ConnectionManagerError> {
        Ok(None)
    }

    fn set_last_connection_failed(&self, _: &NodeId) -> Result<(), ConnectionManagerError> {
        unimplemented!()
    }

    fn set_last_connection_succeeded(&self, _: &NodeId) -> Result<(), ConnectionManagerError> {
        unimplemented!()
    }
}
