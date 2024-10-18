//  Copyright 2022. The Tari Project
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
    io,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Sink, Stream};

pub struct EarlyClose<TSock> {
    inner: TSock,
}

impl<T, TSock: Stream<Item = io::Result<T>> + Unpin> EarlyClose<TSock> {
    pub fn new(inner: TSock) -> Self {
        Self { inner }
    }
}

impl<TSock: Stream + Unpin> Stream for EarlyClose<TSock> {
    type Item = TSock::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl<TItem, TSock, T> Sink<TItem> for EarlyClose<TSock>
where TSock: Sink<TItem, Error = io::Error> + Stream<Item = io::Result<T>> + Unpin
{
    type Error = EarlyCloseError<T>;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Poll::Ready(r) = Pin::new(&mut self.inner).poll_ready(cx) {
            return Poll::Ready(r.map_err(Into::into));
        }
        check_for_early_close(&mut self.inner, cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: TItem) -> Result<(), Self::Error> {
        Pin::new(&mut self.inner).start_send(item)?;
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Poll::Ready(r) = Pin::new(&mut self.inner).poll_flush(cx) {
            return Poll::Ready(r.map_err(Into::into));
        }
        check_for_early_close(&mut self.inner, cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Poll::Ready(r) = Pin::new(&mut self.inner).poll_close(cx) {
            return Poll::Ready(r.map_err(Into::into));
        }
        check_for_early_close(&mut self.inner, cx)
    }
}

fn check_for_early_close<T, TSock: Stream<Item = io::Result<T>> + Unpin>(
    sock: &mut TSock,
    cx: &mut Context<'_>,
) -> Poll<Result<(), EarlyCloseError<T>>> {
    match Pin::new(sock).poll_next(cx) {
        Poll::Ready(Some(Ok(msg))) => Poll::Ready(Err(EarlyCloseError::UnexpectedMessage(msg))),
        Poll::Ready(Some(Err(err))) if err.kind() == io::ErrorKind::WouldBlock => Poll::Pending,
        Poll::Pending => Poll::Pending,
        Poll::Ready(Some(Err(err))) => Poll::Ready(Err(err.into())),
        Poll::Ready(None) => Poll::Ready(Err(
            io::Error::new(io::ErrorKind::BrokenPipe, "Connection closed").into()
        )),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EarlyCloseError<T> {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Unexpected message")]
    UnexpectedMessage(T),
}

impl<T> EarlyCloseError<T> {
    pub fn io(&self) -> Option<&io::Error> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }

    pub fn unexpected_message(&self) -> Option<&T> {
        match self {
            EarlyCloseError::UnexpectedMessage(msg) => Some(msg),
            _ => None,
        }
    }
}
