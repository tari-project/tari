// Copyright 2019, The Tari Project
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

pub mod oneshot_trigger;

use std::{
    future::Future,
    pin::Pin,
    sync::{atomic, atomic::AtomicBool, Arc},
    task::{Context, Poll},
};

use futures::{future, future::FusedFuture};

/// Trigger for shutdowns.
///
/// Use `to_signal` to create a future which will resolve when `Shutdown` is triggered.
/// Use `trigger` to signal. All signals will resolve.
///
/// _Note_: This will trigger when dropped, so the `Shutdown` instance should be held as
/// long as required by the application.
#[derive(Clone, Debug)]
pub struct Shutdown {
    trigger: oneshot_trigger::OneshotTrigger<()>,
    is_triggered: Arc<AtomicBool>,
}
impl Shutdown {
    pub fn new() -> Self {
        Self {
            trigger: oneshot_trigger::OneshotTrigger::new(),
            is_triggered: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn trigger(&mut self) {
        self.trigger.broadcast(());
        self.is_triggered.store(true, atomic::Ordering::SeqCst);
    }

    pub fn is_triggered(&self) -> bool {
        self.trigger.is_used()
    }

    pub fn to_signal(&self) -> ShutdownSignal {
        ShutdownSignal {
            inner: self.trigger.to_signal(),
            is_triggered: self.is_triggered.clone(),
        }
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

/// Receiver end of a shutdown signal. Once received the consumer should shut down.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    inner: oneshot_trigger::OneshotSignal<()>,
    is_triggered: Arc<AtomicBool>,
}

impl ShutdownSignal {
    pub fn is_triggered(&self) -> bool {
        // Shared future in OneshotTrigger requires a poll before is_terminated returns true.
        // For our use case here, we expect is_triggered to return true _immediately_ as the trigger is fired without
        // first polling the signal. To this end, we use an AtomicBool to track the triggered state.
        self.is_triggered.load(atomic::Ordering::SeqCst)
    }

    /// Wait for the shutdown signal to trigger.
    pub fn wait(&mut self) -> &mut Self {
        self
    }

    pub fn select<T: Future + Unpin>(self, other: T) -> future::Select<Self, T> {
        future::select(self, other)
    }
}

impl Future for ShutdownSignal {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.inner).poll(cx) {
            // Whether `trigger()` was called Some(()), or the Shutdown dropped (None) we want to resolve this future
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl FusedFuture for ShutdownSignal {
    fn is_terminated(&self) -> bool {
        self.is_triggered()
    }
}

#[derive(Debug, Clone, Default)]
pub struct OptionalShutdownSignal(Option<ShutdownSignal>);

impl OptionalShutdownSignal {
    pub fn none() -> Self {
        Self(None)
    }

    /// Set the shutdown signal. Once set this OptionalShutdownSignal will resolve
    /// in the same way as the given `ShutdownSignal`.
    pub fn set(&mut self, signal: ShutdownSignal) -> &mut Self {
        self.0 = Some(signal);
        self
    }

    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    pub fn into_signal(self) -> Option<ShutdownSignal> {
        self.0
    }

    pub fn take(&mut self) -> Option<ShutdownSignal> {
        self.0.take()
    }
}

impl Future for OptionalShutdownSignal {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.as_mut() {
            Some(inner) => Pin::new(inner).poll(cx),
            None => Poll::Pending,
        }
    }
}

impl From<Option<ShutdownSignal>> for OptionalShutdownSignal {
    fn from(inner: Option<ShutdownSignal>) -> Self {
        Self(inner)
    }
}

impl From<ShutdownSignal> for OptionalShutdownSignal {
    fn from(inner: ShutdownSignal) -> Self {
        Self(Some(inner))
    }
}

impl FusedFuture for OptionalShutdownSignal {
    fn is_terminated(&self) -> bool {
        self.0.as_ref().map(FusedFuture::is_terminated).unwrap_or(false)
    }
}

#[cfg(test)]
mod test {

    use tokio::task;

    use super::*;

    #[tokio::test]
    async fn trigger() {
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        assert!(!shutdown.is_triggered());
        let fut = task::spawn(async move {
            signal.await;
        });
        shutdown.trigger();
        assert!(shutdown.is_triggered());
        // Shutdown::trigger is idempotent
        shutdown.trigger();
        assert!(shutdown.is_triggered());
        fut.await.unwrap();
    }

    #[tokio::test]
    async fn signal_clone() {
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        let mut signal_clone = signal.clone();
        let fut = task::spawn(async move {
            signal_clone.wait().await;
            assert!(signal_clone.is_triggered());
        });
        assert!(!signal.is_triggered());
        shutdown.trigger();
        assert!(signal.is_triggered());
        assert!(shutdown.is_triggered());
        fut.await.unwrap();
    }

    #[tokio::test]
    async fn drop_trigger() {
        let shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        let signal_clone = signal.clone();
        let fut = task::spawn(async move {
            signal_clone.await;
            signal.await;
        });
        drop(shutdown);
        fut.await.unwrap();
    }
}
