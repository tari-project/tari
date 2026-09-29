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
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures::{
    FutureExt,
    channel::{oneshot, oneshot::Receiver},
    future::{FusedFuture, Shared},
};

/// Create a new [`OneshotTrigger`].
pub fn channel<T: Clone>() -> OneshotTrigger<T> {
    OneshotTrigger::new()
}

/// A single-use broadcast: every [`OneshotSignal`] resolves to `Some(value)` once [`broadcast`](Self::broadcast) is
/// called, or to `None` once every clone of the trigger has been dropped without broadcasting.
#[derive(Clone, Debug)]
pub struct OneshotTrigger<T> {
    sender: Arc<Mutex<Option<oneshot::Sender<T>>>>,
    signal: OneshotSignal<T>,
}

impl<T: Clone> OneshotTrigger<T> {
    pub fn new() -> Self {
        let (tx, rx) = oneshot::channel();
        Self {
            sender: Arc::new(Mutex::new(Some(tx))),
            signal: rx.shared().into(),
        }
    }

    pub fn to_signal(&self) -> OneshotSignal<T> {
        self.signal.clone()
    }

    pub fn broadcast(&mut self, item: T) {
        let mut x = self.sender.lock().unwrap();
        if let Some(tx) = (*x).take() {
            let _result = tx.send(item);
        }
    }

    pub fn is_used(&self) -> bool {
        self.sender.lock().unwrap().is_none()
    }
}

impl<T: Clone> Default for OneshotTrigger<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Receiving end of a [`OneshotTrigger`].
///
/// Resolves to `Some(value)` once [`OneshotTrigger::broadcast`] is called, or to `None` if every clone of the
/// trigger was dropped without broadcasting. The output is cached per handle: polling a handle again after it has
/// resolved returns the same output rather than panicking or changing its answer.
#[derive(Debug, Clone)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct OneshotSignal<T> {
    inner: Shared<oneshot::Receiver<T>>,
    /// `Some(output)` once this handle has resolved. The inner `Shared` must never be polled after that point
    /// (futures-util panics if a completed `Shared` is polled again), so this doubles as that guard.
    output: Option<Option<T>>,
}

// The cached output is never pinned: it is only ever moved out by clone, never projected. `Shared<Receiver<T>>` is
// `Unpin` for every `T`.
impl<T> Unpin for OneshotSignal<T> {}

impl<T: Clone> From<Shared<oneshot::Receiver<T>>> for OneshotSignal<T> {
    fn from(inner: Shared<Receiver<T>>) -> Self {
        Self { inner, output: None }
    }
}

impl<T: Clone> Future for OneshotSignal<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(output) = &this.output {
            return Poll::Ready(output.clone());
        }

        // Defensive: a `Shared` that has already completed must not be polled again. This is unreachable while the
        // output cache is set on every `Ready`, but keeps the handle from panicking if that ever changes.
        if this.inner.is_terminated() {
            this.output = Some(None);
            return Poll::Ready(None);
        }

        let output = match Pin::new(&mut this.inner).poll(cx) {
            Poll::Ready(Ok(v)) => Some(v),
            // Channel canceled: every trigger was dropped without broadcasting
            Poll::Ready(Err(_)) => None,
            Poll::Pending => return Poll::Pending,
        };
        this.output = Some(output.clone());
        Poll::Ready(output)
    }
}

impl<T: Clone> FusedFuture for OneshotSignal<T> {
    /// True once this handle has returned `Poll::Ready`.
    fn is_terminated(&self) -> bool {
        self.output.is_some()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn repoll_returns_cached_value() {
        let mut trigger = OneshotTrigger::<u32>::new();
        let mut signal = trigger.to_signal();
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(signal.poll_unpin(&mut cx).is_pending());
        assert!(!signal.is_terminated());
        trigger.broadcast(7);
        assert_eq!(signal.poll_unpin(&mut cx), Poll::Ready(Some(7)));
        assert!(signal.is_terminated());
        // Re-polling a completed handle must neither panic nor change its answer
        assert_eq!(signal.poll_unpin(&mut cx), Poll::Ready(Some(7)));
        assert_eq!(signal.poll_unpin(&mut cx), Poll::Ready(Some(7)));
        // A clone of a completed handle carries the cached output
        let mut clone = signal.clone();
        assert!(clone.is_terminated());
        assert_eq!(clone.poll_unpin(&mut cx), Poll::Ready(Some(7)));
        // A fresh handle taken after the broadcast still sees the value
        let mut fresh = trigger.to_signal();
        assert_eq!(fresh.poll_unpin(&mut cx), Poll::Ready(Some(7)));
    }

    #[test]
    fn drop_path_returns_none_consistently() {
        let trigger = OneshotTrigger::<u32>::new();
        let mut signal = trigger.to_signal();
        let mut other = signal.clone();
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(signal.poll_unpin(&mut cx).is_pending());
        drop(trigger);
        assert_eq!(signal.poll_unpin(&mut cx), Poll::Ready(None));
        assert!(signal.is_terminated());
        assert_eq!(signal.poll_unpin(&mut cx), Poll::Ready(None));
        assert!(!other.is_terminated());
        assert_eq!(other.poll_unpin(&mut cx), Poll::Ready(None));
        assert_eq!(other.poll_unpin(&mut cx), Poll::Ready(None));
    }

    #[tokio::test]
    async fn await_then_repoll() {
        let mut trigger = OneshotTrigger::<&'static str>::new();
        let mut signal = trigger.to_signal();
        let task = tokio::spawn(async move {
            let first = (&mut signal).await;
            let second = (&mut signal).await;
            (first, second)
        });
        trigger.broadcast("done");
        assert_eq!(task.await.unwrap(), (Some("done"), Some("done")));
    }
}
