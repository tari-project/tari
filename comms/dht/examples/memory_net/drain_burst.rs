// Copyright 2020, The Tari Project
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

use futures::{Future, Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub struct DrainBurst<'a, St>
where St: Stream + ?Sized
{
    inner: &'a mut St,
    collection: Vec<St::Item>,
}

impl<St: ?Sized + Unpin + Stream> Unpin for DrainBurst<'_, St> {}

impl<'a, St> DrainBurst<'a, St>
where St: ?Sized + Stream + Unpin
{
    pub fn new(stream: &'a mut St) -> Self {
        let (lower_bound, upper_bound) = stream.size_hint();
        Self {
            inner: stream,
            collection: Vec::with_capacity(upper_bound.or(Some(lower_bound)).unwrap()),
        }
    }
}

impl<St> Future for DrainBurst<'_, St>
where St: ?Sized + Stream + Unpin
{
    type Output = Vec<St::Item>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(item)) => {
                    self.collection.push(item);
                },
                Poll::Ready(None) | Poll::Pending => {
                    break Poll::Ready(self.collection.drain(..).collect());
                },
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::stream;

    #[tokio_macros::test_basic]
    async fn drain_terminating_stream() {
        let mut stream = stream::iter(1..10u8);
        let burst = DrainBurst::new(&mut stream).await;
        assert_eq!(burst, (1..10u8).into_iter().collect::<Vec<_>>());
    }

    #[tokio_macros::test_basic]
    async fn drain_stream_with_pending() {
        let mut stream = stream::iter(1..10u8);
        let burst = DrainBurst::new(&mut stream).await;
        assert_eq!(burst, (1..10u8).into_iter().collect::<Vec<_>>());
    }
}
