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

use futures::{channel::oneshot, future::join_all};
use std::time::Duration;
use tokio::future::FutureExt;

/// Receiver end of a shutdown signal. Once the oneshot sender has been received
/// the receiver should send on the received oneshot to indicate that it has shut down.
pub type ShutdownSignal = oneshot::Receiver<ShutdownSignalGuard>;

pub struct ShutdownSignalGuard(Option<oneshot::Sender<()>>);

impl ShutdownSignalGuard {
    pub fn signal(&mut self) -> Result<(), ()> {
        match self.0.take() {
            Some(signal) => signal.send(()),
            None => Ok(()),
        }
    }
}

impl From<oneshot::Sender<()>> for ShutdownSignalGuard {
    fn from(signal: oneshot::Sender<()>) -> Self {
        Self(Some(signal))
    }
}

impl Drop for ShutdownSignalGuard {
    fn drop(&mut self) {
        let _ = self.signal();
    }
}

pub struct Shutdown {
    signals: Vec<oneshot::Sender<ShutdownSignalGuard>>,
    is_triggered: bool,
    timeout: Duration,
    on_triggered: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl Default for Shutdown {
    fn default() -> Self {
        Self {
            signals: Vec::default(),
            is_triggered: false,
            timeout: Duration::from_secs(5),
            on_triggered: None,
        }
    }
}

impl Shutdown {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn on_triggered<F>(&mut self, on_trigger: F) -> &mut Self
    where F: FnOnce() + Send + Sync + 'static {
        self.on_triggered = Some(Box::new(on_trigger));
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn new_signal(&mut self) -> ShutdownSignal {
        let (tx, rx) = oneshot::channel();
        self.signals.push(tx);
        rx
    }

    pub async fn trigger(&mut self) -> Result<(), ()> {
        if !self.is_triggered {
            self.is_triggered = true;
            let mut receivers = Vec::with_capacity(self.signals.len());
            for signal in self.signals.drain(..) {
                let (sender, receiver) = oneshot::channel();
                signal.send(sender.into()).map_err(|_| ())?;
                receivers.push(receiver);
            }

            for result in join_all(receivers).timeout(self.timeout).await.map_err(|_| ())? {
                result.map_err(|_| ())?
            }

            if let Some(on_triggered) = self.on_triggered.take() {
                on_triggered();
            }
        }

        Ok(())
    }

    pub fn is_triggered(&self) -> bool {
        self.is_triggered
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Instant,
    };
    use tokio::{runtime::Runtime, timer};

    #[test]
    fn trigger() {
        let rt = Runtime::new().unwrap();
        let mut shutdown = Shutdown::new();
        let signal = shutdown.new_signal();
        assert_eq!(shutdown.is_triggered(), false);
        rt.spawn(async move {
            signal.await.unwrap().signal().unwrap();
        });
        rt.block_on(shutdown.trigger()).unwrap();
        assert_eq!(shutdown.is_triggered(), true);
    }

    #[test]
    fn trigger_timeout() {
        let rt = Runtime::new().unwrap();
        let mut shutdown = Shutdown::new().with_timeout(Duration::from_secs(0));
        let signal = shutdown.new_signal();
        assert_eq!(shutdown.is_triggered(), false);
        rt.spawn(async move {
            let guard = signal.await.unwrap();
            timer::delay(Instant::now() + Duration::from_secs(1)).await;
            drop(guard);
        });
        assert!(rt.block_on(shutdown.trigger()).is_err());
        assert_eq!(shutdown.is_triggered(), true);
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
        let signal = shutdown.new_signal();
        rt.spawn(async move {
            let _ = signal.await.unwrap();
        });
        rt.block_on(shutdown.trigger()).unwrap();
        assert_eq!(spy.load(Ordering::SeqCst), true);
    }
}
