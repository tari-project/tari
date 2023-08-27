//  Copyright 2020, The Taiji Project
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

//! Rate limited flow control implementation that allows a certain number of items to be obtained from the stream within
//! a given time interval. The underlying stream will begin to buffer and produce backpressure if producers exceed the
//! capacity and restock_intervals.

// This is slightly changed from the libra rate limiter implementation

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures::FutureExt;
use pin_project::pin_project;
use tokio::{
    sync::{AcquireError, OwnedSemaphorePermit, Semaphore},
    time,
    time::{Interval, MissedTickBehavior},
};
use tokio_stream::Stream;

pub trait RateLimit: Stream {
    /// Consumes the stream and returns a rate-limited stream that only polls the underlying stream
    /// a maximum of `capacity` times within `restock_interval`.
    fn rate_limit(self, capacity: usize, restock_interval: Duration) -> RateLimiter<Self>
    where Self: Sized {
        RateLimiter::new(self, capacity, restock_interval)
    }
}

impl<T: Stream> RateLimit for T {}

/// Rate limiter for a Stream
#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct RateLimiter<T> {
    /// The inner stream to poll when a permit has been acquired
    #[pin]
    stream: T,
    /// An interval stream that "restocks" the permits
    #[pin]
    interval: Interval,
    /// The maximum permits to issue
    capacity: usize,
    /// A semaphore that holds the permits
    permits: Arc<Semaphore>,
    #[allow(clippy::type_complexity)]
    permit_future: Option<Pin<Box<dyn Future<Output = Result<OwnedSemaphorePermit, AcquireError>> + Send>>>,
    permit_acquired: bool,
}

impl<T: Stream> RateLimiter<T> {
    pub(self) fn new(stream: T, capacity: usize, restock_interval: Duration) -> Self {
        let mut interval = time::interval(restock_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);
        RateLimiter {
            stream,
            capacity,

            interval,
            // `interval` starts immediately, so we can start with zero permits
            permits: Arc::new(Semaphore::new(0)),
            permit_future: None,
            permit_acquired: false,
        }
    }
}

impl<T: Stream> Stream for RateLimiter<T> {
    type Item = T::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // "Restock" permits once interval is ready
        if self.as_mut().project().interval.poll_tick(cx).is_ready() {
            self.permits
                .add_permits(self.capacity - self.permits.available_permits());
        }

        // Attempt to acquire a permit
        if !self.permit_acquired {
            // Set the permit future
            if self.permit_future.is_none() {
                let permits = self.permits.clone();
                *self.as_mut().project().permit_future = Some(permits.acquire_owned().boxed());
            }

            // Wait until a permit is acquired
            // `unwrap()` is safe because acquire_owned only panics if the semaphore has closed, but we never close it
            // for the lifetime of this instance
            let permit = futures::ready!(self
                .as_mut()
                .project()
                .permit_future
                .as_mut()
                .unwrap()
                .as_mut()
                .poll(cx))
            .unwrap();
            // Don't release the permit on drop, `interval` will restock permits
            permit.forget();
            let this = self.as_mut().project();
            *this.permit_acquired = true;
            *this.permit_future = None;
        }

        // A permit is acquired, poll the underlying stream
        let item = futures::ready!(self.as_mut().project().stream.poll_next(cx));
        // Reset to allow a new permit to be acquired on the next message
        *self.as_mut().project().permit_acquired = false;
        Poll::Ready(item)
    }
}

#[cfg(test)]
mod test {
    use futures::{stream, StreamExt};

    use super::*;

    #[tokio::test]
    async fn rate_limit() {
        let repeater = stream::repeat(());

        let mut rate_limited = repeater.rate_limit(10, Duration::from_secs(100));

        let timeout = time::sleep(Duration::from_millis(50));
        tokio::pin!(timeout);
        let mut count = 0usize;
        loop {
            let item = tokio::select! {
                biased;
                _ = &mut timeout => None,
                item = rate_limited.next() => item,
            };

            match item {
                Some(_) => {
                    count += 1;
                },
                None => break,
            }
        }
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn rate_limit_restock() {
        let repeater = stream::repeat(());

        let mut rate_limited = repeater.rate_limit(10, Duration::from_millis(10));

        let timeout = time::sleep(Duration::from_millis(50));
        tokio::pin!(timeout);
        let mut count = 0usize;
        loop {
            let item = tokio::select! {
                biased;
                _ = &mut timeout => None,
                item = rate_limited.next() => item,
            };
            match item {
                Some(_) => {
                    count += 1;
                },
                None => break,
            }
        }
        // Test that at least 1 restock happens.
        assert!(count > 10);
    }
}
