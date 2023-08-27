//  Copyright 2021, The Taiji Project
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

// Copied from futures rs

use std::pin::Pin;

use futures::{
    future::{FusedFuture, Future},
    ready,
    stream::{Fuse, StreamExt},
    task::{Context, Poll},
    Sink,
    Stream,
    TryStream,
};
use pin_project::pin_project;

/// Future for the [`forward`](super::StreamExt::forward) method.
#[pin_project(project = ForwardProj)]
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Forward<St, Si, Item> {
    #[pin]
    sink: Option<Si>,
    #[pin]
    stream: Fuse<St>,
    buffered_item: Option<Item>,
}

impl<St, Si, Item> Forward<St, Si, Item>
where St: TryStream
{
    pub(crate) fn new(stream: St, sink: Si) -> Self {
        Self {
            sink: Some(sink),
            stream: stream.fuse(),
            buffered_item: None,
        }
    }
}

impl<St, Si, Item, E> FusedFuture for Forward<St, Si, Item>
where
    Si: Sink<Item, Error = E>,
    St: Stream<Item = Result<Item, E>>,
{
    fn is_terminated(&self) -> bool {
        self.sink.is_none()
    }
}

impl<St, Si, Item, E> Future for Forward<St, Si, Item>
where
    Si: Sink<Item, Error = E>,
    St: Stream<Item = Result<Item, E>>,
{
    type Output = Result<(), E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ForwardProj {
            mut sink,
            mut stream,
            buffered_item,
        } = self.project();
        let mut si = sink.as_mut().as_pin_mut().expect("polled `Forward` after completion");

        loop {
            // If we've got an item buffered already, we need to write it to the
            // sink before we can do anything else
            if buffered_item.is_some() {
                ready!(si.as_mut().poll_ready(cx))?;
                si.as_mut().start_send(buffered_item.take().unwrap())?;
            }

            match stream.as_mut().poll_next(cx)? {
                Poll::Ready(Some(item)) => {
                    *buffered_item = Some(item);
                },
                Poll::Ready(None) => {
                    ready!(si.poll_close(cx))?;
                    sink.set(None);
                    return Poll::Ready(Ok(()));
                },
                Poll::Pending => {
                    ready!(si.poll_flush(cx))?;
                    return Poll::Pending;
                },
            }
        }
    }
}
