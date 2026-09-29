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

//! A cancellation primitive: one [`Shutdown`] owner side, any number of [`ShutdownSignal`] listeners.
//!
//! A signal resolves when any clone of the `Shutdown` calls [`Shutdown::trigger`], or when the **last** clone of the
//! `Shutdown` is dropped. Either way [`ShutdownSignal::is_triggered`] becomes `true` before any waiting signal is
//! woken, and [`ShutdownSignal::reason`] reports which of the two happened.
//!
//! [`Shutdown::wait_for_listeners`] resolves once every `ShutdownSignal` handed out has been dropped, which lets an
//! owner wait for the tasks it told to stop to actually exit.

pub mod oneshot_trigger;

// Compile and run the README examples as doc tests
#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        Mutex,
        PoisonError,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use futures::{
    FutureExt,
    channel::oneshot,
    future::{self, FusedFuture},
    task::AtomicWaker,
};

/// Why a shutdown signal resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShutdownReason {
    /// [`Shutdown::trigger`] was called on some clone of the `Shutdown`.
    Triggered,
    /// The last clone of the `Shutdown` was dropped without `trigger` being called.
    Dropped,
}

const REASON_NONE: u8 = 0;
const REASON_TRIGGERED: u8 = 1;
const REASON_DROPPED: u8 = 2;

impl ShutdownReason {
    fn as_u8(self) -> u8 {
        match self {
            ShutdownReason::Triggered => REASON_TRIGGERED,
            ShutdownReason::Dropped => REASON_DROPPED,
        }
    }
}

/// State shared by every `Shutdown` clone and every `ShutdownSignal`.
#[derive(Debug)]
struct State {
    /// Set (after `reason`) by whichever of `trigger` or last-clone drop happens first, and never cleared.
    is_triggered: AtomicBool,
    /// First writer wins. Always set before `is_triggered`.
    reason: AtomicU8,
    /// Number of live `ShutdownSignal` handles.
    listeners: AtomicUsize,
    /// Woken when `listeners` drops to zero.
    drain_waker: AtomicWaker,
}

impl State {
    fn new() -> Self {
        Self {
            is_triggered: AtomicBool::new(false),
            reason: AtomicU8::new(REASON_NONE),
            listeners: AtomicUsize::new(0),
            drain_waker: AtomicWaker::new(),
        }
    }

    fn set_triggered(&self, reason: ShutdownReason) {
        // The first reason recorded sticks: a trigger followed by the last clone dropping stays `Triggered`.
        let _ignore = self
            .reason
            .compare_exchange(REASON_NONE, reason.as_u8(), Ordering::SeqCst, Ordering::SeqCst);
        self.is_triggered.store(true, Ordering::SeqCst);
    }

    fn is_triggered(&self) -> bool {
        self.is_triggered.load(Ordering::SeqCst)
    }

    fn reason(&self) -> Option<ShutdownReason> {
        if !self.is_triggered() {
            return None;
        }
        match self.reason.load(Ordering::SeqCst) {
            REASON_TRIGGERED => Some(ShutdownReason::Triggered),
            REASON_DROPPED => Some(ShutdownReason::Dropped),
            _ => None,
        }
    }

    fn poll_drained(&self, cx: &mut Context<'_>) -> Poll<()> {
        if self.listeners.load(Ordering::SeqCst) == 0 {
            return Poll::Ready(());
        }
        self.drain_waker.register(cx.waker());
        // Re-check after registering so a drop between the first check and `register` is not missed
        if self.listeners.load(Ordering::SeqCst) == 0 {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/// The owner-side trigger, shared by every `Shutdown` clone. Dropping the last clone drops this and resolves all
/// signals with [`ShutdownReason::Dropped`].
#[derive(Debug)]
struct Trigger {
    state: Arc<State>,
    sender: Mutex<Option<oneshot::Sender<()>>>,
    signal: oneshot_trigger::OneshotSignal<()>,
}

impl Trigger {
    fn new() -> Self {
        let (tx, rx) = oneshot::channel();
        Self {
            state: Arc::new(State::new()),
            sender: Mutex::new(Some(tx)),
            signal: rx.shared().into(),
        }
    }

    fn fire(&self, reason: ShutdownReason) {
        // Store the flag BEFORE waking anyone, so every woken signal observes `is_triggered() == true`.
        self.state.set_triggered(reason);
        let sender = self.sender.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(tx) = sender {
            match reason {
                ShutdownReason::Triggered => {
                    let _ignore = tx.send(());
                },
                // Dropping the sender cancels the channel, which resolves every signal
                ShutdownReason::Dropped => drop(tx),
            }
        }
    }
}

impl Drop for Trigger {
    fn drop(&mut self) {
        self.fire(ShutdownReason::Dropped);
    }
}

/// Owner side of a shutdown signal.
///
/// Use [`to_signal`](Self::to_signal) to create a future which resolves when the shutdown happens, and
/// [`trigger`](Self::trigger) to make it happen.
///
/// `Shutdown` is `Clone`; all clones control the same shutdown. Signals resolve when **any** clone calls `trigger`,
/// or when the **last** clone is dropped (with [`ShutdownReason::Dropped`]). Hold on to a `Shutdown` for as long as
/// the things listening to it should keep running.
#[derive(Clone, Debug)]
pub struct Shutdown {
    trigger: Arc<Trigger>,
}

impl Shutdown {
    pub fn new() -> Self {
        Self {
            trigger: Arc::new(Trigger::new()),
        }
    }

    /// Trigger the shutdown, resolving every signal. Idempotent.
    pub fn trigger(&self) {
        self.trigger.fire(ShutdownReason::Triggered);
    }

    /// True once `trigger` has been called on any clone.
    pub fn is_triggered(&self) -> bool {
        self.trigger.state.is_triggered()
    }

    /// Why the shutdown happened, or `None` if it has not happened yet.
    pub fn reason(&self) -> Option<ShutdownReason> {
        self.trigger.state.reason()
    }

    /// Create a new signal that resolves when this shutdown happens.
    pub fn to_signal(&self) -> ShutdownSignal {
        ShutdownSignal {
            inner: self.trigger.signal.clone(),
            listener: Listener::new(self.trigger.state.clone()),
        }
    }

    /// Resolves once every [`ShutdownSignal`] created from this `Shutdown` (including clones of those signals) has
    /// been dropped. The `Shutdown` itself does not count.
    ///
    /// This does not trigger the shutdown; call [`trigger`](Self::trigger) first. A listener that is held somewhere
    /// and never dropped keeps this pending forever, so callers should bound it with a timeout.
    ///
    /// Only one drain future is guaranteed to be woken at a time: await it from a single task.
    pub fn wait_for_listeners(&self) -> impl Future<Output = ()> + Send + 'static {
        drain(self.trigger.state.clone())
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

fn drain(state: Arc<State>) -> impl Future<Output = ()> + Send + 'static {
    future::poll_fn(move |cx| state.poll_drained(cx))
}

/// Counts one live `ShutdownSignal` for [`Shutdown::wait_for_listeners`].
#[derive(Debug)]
struct Listener {
    state: Arc<State>,
}

impl Listener {
    fn new(state: Arc<State>) -> Self {
        state.listeners.fetch_add(1, Ordering::SeqCst);
        Self { state }
    }
}

impl Clone for Listener {
    fn clone(&self) -> Self {
        Self::new(self.state.clone())
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        if self.state.listeners.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.state.drain_waker.wake();
        }
    }
}

/// Receiver end of a shutdown signal. Once it resolves, the holder should shut down and drop it.
///
/// Dropping a `ShutdownSignal` does not trigger anything; it only tells [`Shutdown::wait_for_listeners`] that this
/// listener is gone.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    inner: oneshot_trigger::OneshotSignal<()>,
    listener: Listener,
}

impl ShutdownSignal {
    /// True once the shutdown has happened, whether by `trigger` or by the last `Shutdown` clone dropping. This is
    /// true before any waiting signal is woken and does not require this signal to have been polled.
    pub fn is_triggered(&self) -> bool {
        self.listener.state.is_triggered()
    }

    /// Why the shutdown happened, or `None` if it has not happened yet.
    pub fn reason(&self) -> Option<ShutdownReason> {
        self.listener.state.reason()
    }

    /// Wait for the shutdown signal to trigger.
    pub fn wait(&mut self) -> &mut Self {
        self
    }

    pub fn select<T: Future + Unpin>(self, other: T) -> future::Select<Self, T> {
        future::select(self, other)
    }

    /// Consume this signal and return a future that resolves once every other `ShutdownSignal` for the same
    /// shutdown has been dropped. Equivalent to [`Shutdown::wait_for_listeners`] for holders that only have a signal.
    ///
    /// This does not trigger the shutdown, and callers should bound it with a timeout.
    pub fn drained(self) -> impl Future<Output = ()> + Send + 'static {
        let state = self.listener.state.clone();
        drop(self);
        drain(state)
    }
}

impl Future for ShutdownSignal {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Whether `trigger()` was called (Some(())), or the last Shutdown was dropped (None), we resolve this future
        self.inner.poll_unpin(cx).map(|_| ())
    }
}

impl FusedFuture for ShutdownSignal {
    /// True once THIS handle has returned `Poll::Ready`. Use [`is_triggered`](ShutdownSignal::is_triggered) to ask
    /// whether the shutdown has happened.
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use futures::task::noop_waker_ref;
    use tokio::task;

    use super::*;

    fn poll_once<F: Future + Unpin>(fut: &mut F) -> Poll<F::Output> {
        fut.poll_unpin(&mut Context::from_waker(noop_waker_ref()))
    }

    #[tokio::test]
    async fn trigger() {
        let shutdown = Shutdown::new();
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
        let shutdown = Shutdown::new();
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

    #[test]
    fn last_clone_drop_sets_triggered_and_reason() {
        let shutdown = Shutdown::new();
        let other = shutdown.clone();
        let mut signal = shutdown.to_signal();

        drop(shutdown);
        // A surviving clone keeps the shutdown alive
        assert!(!signal.is_triggered());
        assert_eq!(signal.reason(), None);
        assert!(!other.is_triggered());
        assert!(poll_once(&mut signal).is_pending());

        drop(other);
        assert!(signal.is_triggered());
        assert_eq!(signal.reason(), Some(ShutdownReason::Dropped));
        assert!(poll_once(&mut signal).is_ready());
    }

    #[test]
    fn reason_after_trigger() {
        let shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        assert_eq!(shutdown.reason(), None);
        assert_eq!(signal.reason(), None);
        shutdown.trigger();
        assert_eq!(shutdown.reason(), Some(ShutdownReason::Triggered));
        assert_eq!(signal.reason(), Some(ShutdownReason::Triggered));
        // Dropping the owner afterwards does not change the recorded reason
        drop(shutdown);
        assert_eq!(signal.reason(), Some(ShutdownReason::Triggered));
    }

    #[test]
    fn woken_waiter_observes_triggered() {
        // Run many times on real threads to give a broadcast-before-store ordering a chance to show up
        for _ in 0..200 {
            let shutdown = Shutdown::new();
            let signal = shutdown.to_signal();
            let waiter = std::thread::spawn(move || {
                let mut signal = signal;
                futures::executor::block_on(&mut signal);
                signal.is_triggered()
            });
            shutdown.trigger();
            assert!(waiter.join().unwrap());
        }
    }

    #[test]
    fn woken_waiter_observes_triggered_on_drop() {
        for _ in 0..200 {
            let shutdown = Shutdown::new();
            let signal = shutdown.to_signal();
            let waiter = std::thread::spawn(move || {
                let mut signal = signal;
                futures::executor::block_on(&mut signal);
                (signal.is_triggered(), signal.reason())
            });
            drop(shutdown);
            assert_eq!(waiter.join().unwrap(), (true, Some(ShutdownReason::Dropped)));
        }
    }

    #[test]
    fn is_terminated_follows_fused_future_contract_on_trigger() {
        let shutdown = Shutdown::new();
        let mut signal = shutdown.to_signal();
        assert!(!signal.is_terminated());
        shutdown.trigger();
        // Triggered, but this handle has not returned Ready yet
        assert!(signal.is_triggered());
        assert!(!signal.is_terminated());
        assert!(poll_once(&mut signal).is_ready());
        assert!(signal.is_terminated());
        // Re-polling a terminated signal must not panic
        assert!(poll_once(&mut signal).is_ready());
    }

    #[test]
    fn is_terminated_follows_fused_future_contract_on_drop() {
        let shutdown = Shutdown::new();
        let mut signal = shutdown.to_signal();
        let mut other = signal.clone();
        drop(shutdown);
        assert!(signal.is_triggered());
        assert!(!signal.is_terminated());
        assert!(poll_once(&mut signal).is_ready());
        assert!(signal.is_terminated());
        assert!(poll_once(&mut signal).is_ready());
        // Terminated is per handle
        assert!(!other.is_terminated());
        assert!(poll_once(&mut other).is_ready());
        assert!(other.is_terminated());
    }

    #[test]
    fn wait_for_listeners_waits_for_every_signal() {
        let shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        let signal_clone = signal.clone();
        let second = shutdown.to_signal();
        let mut drain = Box::pin(shutdown.wait_for_listeners());

        shutdown.trigger();
        assert!(poll_once(&mut drain).is_pending());
        drop(signal);
        assert!(poll_once(&mut drain).is_pending());
        drop(second);
        // The clone is still a listener
        assert!(poll_once(&mut drain).is_pending());
        drop(signal_clone);
        assert!(poll_once(&mut drain).is_ready());
    }

    #[test]
    fn wait_for_listeners_ready_without_listeners() {
        let shutdown = Shutdown::new();
        let mut drain = Box::pin(shutdown.wait_for_listeners());
        assert!(poll_once(&mut drain).is_ready());
    }

    #[tokio::test]
    async fn wait_for_listeners_wakes_when_tasks_exit() {
        let shutdown = Shutdown::new();
        let tasks = (0..4)
            .map(|_| {
                let mut signal = shutdown.to_signal();
                task::spawn(async move {
                    (&mut signal).await;
                    // Simulate cleanup work while still holding the signal
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    drop(signal);
                })
            })
            .collect::<Vec<_>>();
        shutdown.trigger();
        tokio::time::timeout(Duration::from_secs(5), shutdown.wait_for_listeners())
            .await
            .expect("drain should complete once every task has exited");
        for t in tasks {
            assert!(t.is_finished());
        }
    }

    #[test]
    fn signal_drained_excludes_itself() {
        let shutdown = Shutdown::new();
        let signal = shutdown.to_signal();
        let other = shutdown.to_signal();
        let mut drain = Box::pin(signal.drained());
        assert!(poll_once(&mut drain).is_pending());
        drop(other);
        assert!(poll_once(&mut drain).is_ready());
    }
}
