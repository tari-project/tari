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
#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
use futures::{
    channel::{oneshot, oneshot::Canceled},
    future::{Fuse, FusedFuture, Shared},
    task::{Context, Poll},
    Future,
    FutureExt,
};
use std::pin::Pin;

/// Trigger for shutdowns.
///
/// Use `to_signal` to create a future which will resolve when `Shutdown` is triggered.
/// Use `trigger` to signal. All signals will resolve.
///
/// _Note_: This will trigger when dropped, so the `Shutdown` instance should be held as
/// long as required by the application.
pub struct Shutdown {
    trigger: Option<oneshot::Sender<()>>,
    signal: ShutdownSignal,
    on_triggered: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl Shutdown {
    /// Create a new Shutdown
    pub fn new() -> Self {
        let (tx, rx) = oneshot::channel();
        Self {
            trigger: Some(tx),
            signal: rx.fuse().shared(),
            on_triggered: None,
        }
    }

    /// Set the on_triggered callback
    pub fn on_triggered<F>(&mut self, on_trigger: F) -> &mut Self
    where F: FnOnce() + Send + Sync + 'static {
        self.on_triggered = Some(Box::new(on_trigger));
        self
    }

    /// Convert this into a ShutdownSignal without consuming the
    /// struct.
    pub fn to_signal(&self) -> ShutdownSignal {
        self.signal.clone()
    }

    /// Trigger any listening signals
    pub fn trigger(&mut self) -> Result<(), ()> {
        match self.trigger.take() {
            Some(trigger) => {
                trigger.send(())?;

                if let Some(on_triggered) = self.on_triggered.take() {
                    on_triggered();
                }

                Ok(())
            },
            None => Ok(()),
        }
    }

    pub fn is_triggered(&self) -> bool {
        self.trigger.is_none()
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        let _ = self.trigger();
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

/// Receiver end of a shutdown signal. Once received the consumer should shut down.
pub type ShutdownSignal = Shared<Fuse<oneshot::Receiver<()>>>;

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
    type Output = Result<(), Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.as_mut() {
            Some(inner) => inner.poll_unpin(cx),
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
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokio::runtime::Runtime;

    #[test]
    fn trigger() {
        let rt = Runtime::new().unwrap();
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        assert_eq!(shutdown.is_triggered(), false);
        rt.spawn(async move {
            signal.await.unwrap();
        });
        shutdown.trigger().unwrap();
        // Shutdown::trigger is idempotent
        shutdown.trigger().unwrap();
        assert_eq!(shutdown.is_triggered(), true);
    }

    #[test]
    fn signal_clone() {
        let rt = Runtime::new().unwrap();
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        let signal_clone = signal.clone();
        rt.spawn(async move {
            signal_clone.await.unwrap();
            signal.await.unwrap();
        });
        shutdown.trigger().unwrap();
    }

    #[test]
    fn drop_trigger() {
        let rt = Runtime::new().unwrap();
        let shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        let signal_clone = signal.clone();
        rt.spawn(async move {
            signal_clone.await.unwrap();
            signal.await.unwrap();
        });
        drop(shutdown);
    }

    #[test]
    fn on_trigger() {
        let rt = Runtime::new().unwrap();
        let spy = Arc::new(AtomicBool::new(false));
        let spy_clone = Arc::clone(&spy);
        let mut shutdown = Shutdown::new();
        shutdown.on_triggered(move || {
            spy_clone.store(true, Ordering::SeqCst);
        });
        let signal = shutdown.to_signal();
        rt.spawn(async move {
            signal.await.unwrap();
        });
        shutdown.trigger().unwrap();
        assert_eq!(spy.load(Ordering::SeqCst), true);
    }
}
