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

use futures::{task, Async, Future, Poll};
use std::{
    any::Any,
    collections::HashMap,
    hash::Hash,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
        Mutex,
    },
};

/// Simple collection for named handles
pub struct ServiceHandles<K> {
    handles: Mutex<HashMap<K, Box<dyn Any + Sync + Send>>>,
}

impl<K> ServiceHandles<K>
where K: Eq + Hash
{
    /// Create a new ServiceHandles
    pub fn new() -> Self {
        Self {
            handles: Default::default(),
        }
    }

    /// Add a named ServiceHandle
    pub fn insert(&self, service_name: K, value: impl Any + Send + Sync) {
        acquire_lock!(self.handles).insert(service_name, Box::new(value));
    }

    /// Get a ServiceHandle and downcast it to a type `V`. If the item
    /// does not exist or the downcast fails, `None` is returned.
    pub fn get_handle<V>(&self, service_name: &K) -> Option<V>
    where V: Clone + 'static {
        acquire_lock!(self.handles)
            .get(service_name)
            .and_then(|b| b.downcast_ref::<V>())
            .map(Clone::clone)
    }
}

/// Future which resolves to `ServiceHandles` once it is signaled to
/// do so.
// TODO: Currently not used, perhaps remove this
pub struct HandlesFuture<K> {
    handles: Arc<ServiceHandles<K>>,
    is_ready: Arc<AtomicBool>,
}

impl<K> Clone for HandlesFuture<K> {
    fn clone(&self) -> Self {
        Self {
            handles: Arc::clone(&self.handles),
            is_ready: Arc::clone(&self.is_ready),
        }
    }
}

impl<K> HandlesFuture<K>
where K: Eq + Hash
{
    pub fn new() -> Self {
        Self {
            handles: Arc::new(ServiceHandles::new()),
            is_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn insert(&self, service_name: K, value: impl Any + Send + Sync) {
        self.handles.insert(service_name, value);
    }

    pub fn notify_ready(&self) {
        self.is_ready.store(true, Ordering::Relaxed);
    }
}

impl<K> Future for HandlesFuture<K> {
    type Error = ();
    type Item = Arc<ServiceHandles<K>>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.is_ready.load(Ordering::Relaxed) {
            Ok(Async::Ready(Arc::clone(&self.handles)))
        } else {
            task::current().notify();
            Ok(Async::NotReady)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn service_handles_insert_get() {
        #[derive(Clone)]
        struct TestHandle;
        let handles = ServiceHandles::new();
        handles.insert(1, TestHandle);
        handles.get_handle::<TestHandle>(&1).unwrap();
        assert!(handles.get_handle::<()>(&1).is_none());
        assert!(handles.get_handle::<()>(&2).is_none());
    }
}
