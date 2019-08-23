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

use super::{LazyService, ServiceHandles};
use futures::{task::AtomicTask, Async, Future, Poll};
use std::{
    any::Any,
    hash::Hash,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Future which resolves to `ServiceHandles` once it is signaled to
/// do so.
pub struct ServiceHandlesFuture<N> {
    handles: Arc<ServiceHandles<N>>,
    is_ready: Arc<AtomicBool>,
    task: Arc<AtomicTask>,
}

impl<N> Clone for ServiceHandlesFuture<N> {
    fn clone(&self) -> Self {
        Self {
            handles: Arc::clone(&self.handles),
            is_ready: Arc::clone(&self.is_ready),
            task: Arc::clone(&self.task),
        }
    }
}

impl<N> ServiceHandlesFuture<N>
where N: Eq + Hash
{
    /// Create a new ServiceHandlesFuture with empty handles
    pub fn new() -> Self {
        Self {
            handles: Arc::new(ServiceHandles::new()),
            is_ready: Arc::new(AtomicBool::new(false)),
            task: Arc::new(AtomicTask::new()),
        }
    }

    /// Insert a service handle with the given name
    pub fn insert(&self, service_name: N, value: impl Any + Send + Sync) {
        self.handles.insert(service_name, value);
    }

    /// Retrieve a handle and downcast it to return type and return a copy, otherwise None is returned
    pub fn get_handle<V>(&self, service_name: N) -> Option<V>
    where V: Clone + 'static {
        self.handles.get_handle(service_name)
    }

    /// Call the given function with the final handles once this future is ready (`notify_ready` is called).
    pub fn lazy_service<F, S>(&self, service_fn: F) -> LazyService<F, Self, S>
    where F: FnOnce(Arc<ServiceHandles<N>>) -> S {
        LazyService::new(self.clone(), service_fn)
    }

    /// Notify that all handles are collected and the task should resolve
    pub fn notify_ready(&self) {
        self.is_ready.store(true, Ordering::SeqCst);
        self.task.notify();
    }
}

impl<N> Future for ServiceHandlesFuture<N> {
    type Error = ();
    type Item = Arc<ServiceHandles<N>>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.is_ready.load(Ordering::SeqCst) {
            Ok(Async::Ready(Arc::clone(&self.handles)))
        } else {
            self.task.register();
            Ok(Async::NotReady)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tokio_mock_task::MockTask;

    #[test]
    fn insert_get() {
        #[derive(Clone)]
        struct TestHandle;
        let handles = ServiceHandlesFuture::new();
        handles.insert(1, TestHandle);
        handles.get_handle::<TestHandle>(1).unwrap();
        assert!(handles.get_handle::<()>(1).is_none());
        assert!(handles.get_handle::<()>(2).is_none());
    }

    #[test]
    fn notify_ready() {
        let mut task = MockTask::new();
        task.enter(|| {
            let mut handles = ServiceHandlesFuture::<()>::new();
            let mut clone = handles.clone();

            assert!(handles.poll().unwrap().is_not_ready());
            assert!(clone.poll().unwrap().is_not_ready());
            handles.notify_ready();
            assert!(handles.poll().unwrap().is_ready());
            assert!(clone.poll().unwrap().is_ready());
        })
    }
}
