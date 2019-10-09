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

use super::ServiceHandles;
use crate::handles::LazyService;
use futures::{
    task::{AtomicWaker, Context},
    Future,
    Poll,
};
use std::{
    any::Any,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
        Arc,
    },
};

pub fn handle_notifier() -> (Notifier, ServiceHandlesFuture) {
    let (tx, rx) = mpsc::channel();
    let ready_flag = Arc::new(AtomicBool::new(false));
    (
        Notifier::new(Arc::clone(&ready_flag), rx),
        ServiceHandlesFuture::new(Arc::clone(&ready_flag), tx),
    )
}

pub struct Notifier {
    ready_flag: Arc<AtomicBool>,
    waker_receiver: mpsc::Receiver<Arc<AtomicWaker>>,
}

impl Notifier {
    pub fn new(ready_flag: Arc<AtomicBool>, waker_receiver: mpsc::Receiver<Arc<AtomicWaker>>) -> Self {
        Self {
            ready_flag,
            waker_receiver,
        }
    }

    /// Notify that all handles are collected and the task should resolve
    pub fn notify(&self) {
        self.ready_flag.store(true, Ordering::SeqCst);
        while let Ok(waker) = self.waker_receiver.try_recv() {
            waker.wake();
        }
    }
}

/// Future which resolves to `ServiceHandles` once it is signaled to
/// do so.
pub struct ServiceHandlesFuture {
    handles: Arc<ServiceHandles>,
    ready_flag: Arc<AtomicBool>,
    wake_sender: mpsc::Sender<Arc<AtomicWaker>>,
    waker: Arc<AtomicWaker>,
}

impl Clone for ServiceHandlesFuture {
    fn clone(&self) -> Self {
        let waker = Arc::new(AtomicWaker::new());
        self.wake_sender.send(Arc::clone(&waker)).expect("unable to send waker");
        Self {
            handles: Arc::clone(&self.handles),
            ready_flag: Arc::clone(&self.ready_flag),
            wake_sender: self.wake_sender.clone(),
            waker,
        }
    }
}

impl ServiceHandlesFuture {
    /// Create a new ServiceHandlesFuture with empty handles
    pub fn new(ready_flag: Arc<AtomicBool>, wake_sender: mpsc::Sender<Arc<AtomicWaker>>) -> Self {
        Self {
            handles: Arc::new(ServiceHandles::new()),
            ready_flag,
            wake_sender,
            waker: Arc::new(AtomicWaker::new()),
        }
    }

    /// Insert a service handle with the given name
    pub fn register<H>(&self, handle: H)
    where H: Any + Send + Sync {
        self.handles.register(handle);
    }

    /// Retrieve a handle and downcast it to return type and return a copy, otherwise None is returned
    pub fn get_handle<H>(&self) -> Option<H>
    where H: Clone + 'static {
        self.handles.get_handle()
    }

    // /// Call the given function with the final handles once this future is ready (`notify_ready` is called).
    pub fn lazy_service<F, S>(&self, service_fn: F) -> LazyService<F, Self, S>
    where F: FnOnce(Arc<ServiceHandles>) -> S {
        LazyService::new(self.clone(), service_fn)
    }

    pub fn into_inner(self) -> Arc<ServiceHandles> {
        self.handles
    }
}

impl Future for ServiceHandlesFuture {
    type Output = Arc<ServiceHandles>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready_flag.load(Ordering::SeqCst) {
            Poll::Ready(Arc::clone(&self.handles))
        } else {
            self.waker.register(cx.waker());
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::FutureExt;
    use std::iter::repeat_with;
    use tari_test_utils::counter_context;

    #[test]
    fn insert_get() {
        #[derive(Clone)]
        struct TestHandle;
        let (_notifier, handles) = handle_notifier();
        handles.register(TestHandle);
        handles.get_handle::<TestHandle>().unwrap();
        assert!(handles.get_handle::<()>().is_none());
    }

    #[test]
    fn notify_ready() {
        let (notifier, mut handles) = handle_notifier();
        let mut clone = handles.clone();

        counter_context!(cx, wake_count);

        assert!(handles.poll_unpin(&mut cx).is_pending());
        assert!(clone.poll_unpin(&mut cx).is_pending());
        assert_eq!(wake_count.get(), 0);
        notifier.notify();
        assert_eq!(wake_count.get(), 1);
        assert!(handles.poll_unpin(&mut cx).is_ready());
        assert!(clone.poll_unpin(&mut cx).is_ready());
    }

    #[test]
    fn notify_many() {
        let (notifier, mut handles) = handle_notifier();
        let mut clones = repeat_with(|| handles.clone()).take(10).collect::<Vec<_>>();

        counter_context!(cx, wake_count);
        assert!(handles.poll_unpin(&mut cx).is_pending());

        for clone in clones.iter_mut() {
            assert!(clone.poll_unpin(&mut cx).is_pending());
        }

        notifier.notify();

        for clone in clones.iter_mut() {
            assert!(clone.poll_unpin(&mut cx).is_ready());
        }

        assert_eq!(wake_count.get(), 10);
    }
}
