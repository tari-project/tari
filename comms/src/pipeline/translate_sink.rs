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

use futures::Sink;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

/// A sink which takes inputs sent to it, translates them and sends them on a given sink
pub struct TranslateSink<F, S, I> {
    translater: F,
    sink: S,
    _i: PhantomData<I>,
}

impl<F, S, I> Unpin for TranslateSink<F, S, I> {}

impl<F, S, I> TranslateSink<F, S, I> {
    pub fn new(sink: S, translater: F) -> Self {
        Self {
            translater,
            sink,
            _i: PhantomData,
        }
    }
}

impl<F, S, I> Sink<I> for TranslateSink<F, S, I>
where
    F: Translate<I>,
    I: Unpin,
    S: Sink<F::Output> + Unpin,
{
    type Error = S::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink).poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        match self.translater.translate(item) {
            Some(translated) => Pin::new(&mut self.sink).start_send(translated),
            None => Ok(()),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink).poll_close(cx)
    }
}

pub trait Translate<I> {
    type Output;
    fn translate(&mut self, input: I) -> Option<Self::Output>;
}

impl<I, F, O> Translate<I> for F
where F: FnMut(I) -> Option<O>
{
    type Output = O;

    fn translate(&mut self, input: I) -> Option<Self::Output> {
        (self)(input)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::runtime;
    use futures::{channel::mpsc, SinkExt, StreamExt};

    #[runtime::test_basic]
    async fn check_translates() {
        let (tx, mut rx) = mpsc::channel(1);

        let mut translate_sink = TranslateSink::new(tx, |input: u32| {
            if input % 2 == 0 {
                Some(format!("Even: {}", input))
            } else {
                None
            }
        });

        translate_sink.send(123).await.unwrap();
        translate_sink.send(124).await.unwrap();

        let result = rx.next().await.unwrap();
        assert_eq!(result, "Even: 124");
    }
}
