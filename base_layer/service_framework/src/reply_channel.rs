// Copyright 2019 The Tari Project
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

use futures::{
    channel::{
        mpsc::{self, SendError},
        oneshot,
    },
    ready,
    stream::FusedStream,
    task::Context,
    Future,
    FutureExt,
    Stream,
    StreamExt,
};
use std::{pin::Pin, task::Poll};
use thiserror::Error;
use tower_service::Service;

/// Create a new Requester/Responder pair which wraps and calls the given service
pub fn unbounded<TReq, TResp>() -> (SenderService<TReq, TResp>, Receiver<TReq, TResp>) {
    let (tx, rx) = mpsc::unbounded();
    (SenderService::new(tx), Receiver::new(rx))
}

/// Receiver for a (Request, Reply) tuple, where Reply is a oneshot::Sender
type Rx<TReq, TRes> = mpsc::UnboundedReceiver<(TReq, oneshot::Sender<TRes>)>;
/// Sender for a (Request, Reply) tuple, where Reply is a oneshot::Sender
type Tx<TReq, TRes> = mpsc::UnboundedSender<(TReq, oneshot::Sender<TRes>)>;

pub type TrySenderService<TReq, TResp, TErr> = SenderService<TReq, Result<TResp, TErr>>;
pub type TryReceiver<TReq, TResp, TErr> = Receiver<TReq, Result<TResp, TErr>>;

/// Requester sends `TReq` requests on a given `Tx` sender, and returns an
/// AwaitResponseFuture which will resolve to the generic `TRes`.
///
/// This should be used to make requests which require a response.
///
/// This implements `tower_service::Service`, therefore the `poll_ready` and `call`
/// methods should be used to make a request.
pub struct SenderService<TReq, TRes> {
    /// Used to send the request
    tx: Tx<TReq, TRes>,
}

impl<TReq, TRes> SenderService<TReq, TRes> {
    /// Create a new Requester
    pub fn new(tx: Tx<TReq, TRes>) -> Self {
        Self { tx }
    }
}

impl<TReq, TRes> Clone for SenderService<TReq, TRes> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

impl<TReq, TRes> Service<TReq> for SenderService<TReq, TRes> {
    type Error = TransportChannelError;
    type Future = TransportResponseFuture<TRes>;
    type Response = TRes;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.tx.poll_ready(cx).map_err(|err| {
            if err.is_disconnected() {
                return TransportChannelError::ChannelClosed;
            }

            unreachable!("unbounded channels can never be full");
        })
    }

    fn call(&mut self, request: TReq) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        if self.tx.unbounded_send((request, tx)).is_ok() {
            TransportResponseFuture::new(rx)
        } else {
            // We're not able to send (rx closed) so return a future which resolves to
            // a ChannelClosed error
            TransportResponseFuture::closed()
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq, Clone)]
pub enum TransportChannelError {
    #[error("Error occurred when sending: `{0}`")]
    SendError(#[from] SendError),
    #[error("Request was canceled")]
    Canceled,
    #[error("The response channel has closed")]
    ChannelClosed,
}

/// Response future for Results received over a given oneshot channel Receiver.
pub struct TransportResponseFuture<T> {
    rx: Option<oneshot::Receiver<T>>,
}

impl<T> TransportResponseFuture<T> {
    /// Create a new AwaitResponseFuture
    pub fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx: Some(rx) }
    }

    /// Create a closed AwaitResponseFuture. If this is polled
    /// an RequestorError::ChannelClosed error is returned.
    pub fn closed() -> Self {
        Self { rx: None }
    }
}

impl<T> Future for TransportResponseFuture<T> {
    type Output = Result<T, TransportChannelError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.rx {
            Some(ref mut rx) => rx.poll_unpin(cx).map_err(|_| TransportChannelError::Canceled),
            None => Poll::Ready(Err(TransportChannelError::ChannelClosed)),
        }
    }
}

/// This is the object received on the Receiver-side of this channel.
/// It's a simple wrapper with some convenience functions used to reply to the
/// request.
pub struct RequestContext<TReq, TResp> {
    reply_tx: oneshot::Sender<TResp>,
    request: Option<TReq>,
}

impl<TReq, TResp> RequestContext<TReq, TResp> {
    /// Create a new RequestContect
    pub fn new(request: TReq, reply_tx: oneshot::Sender<TResp>) -> Self {
        Self {
            request: Some(request),
            reply_tx,
        }
    }

    /// Return a reference to the request object. None is returned after take_request has
    /// been called.
    pub fn request(&self) -> Option<&TReq> {
        self.request.as_ref()
    }

    /// Take ownership of the request object, if ownership has not already been taken,
    /// otherwise None is returned.
    pub fn take_request(&mut self) -> Option<TReq> {
        self.request.take()
    }

    /// Consume this object and return it's parts. Namely, the request object and
    /// the reply oneshot channel.
    pub fn split(self) -> (TReq, oneshot::Sender<TResp>) {
        (
            self.request.expect("RequestContext must be initialized with a request"),
            self.reply_tx,
        )
    }

    /// Sends a reply to the caller
    pub fn reply(self, resp: TResp) -> Result<(), TResp> {
        self.reply_tx.send(resp)
    }
}

/// Receiver side of the reply channel.
/// This is functionally equivalent to `rx.map(|(req, reply_tx)| RequestContext::new(req, reply_tx))`
/// but is ergonomically better to use with the `futures::select` macro (implements FusedStream)
/// and has a short type signature.
pub struct Receiver<TReq, TResp> {
    rx: Rx<TReq, TResp>,
}

impl<TReq, TResp> FusedStream for Receiver<TReq, TResp> {
    fn is_terminated(&self) -> bool {
        self.rx.is_terminated()
    }
}

impl<TReq, TResp> Receiver<TReq, TResp> {
    // Create a new Responder
    pub fn new(rx: Rx<TReq, TResp>) -> Self {
        Self { rx }
    }

    pub fn close(&mut self) {
        self.rx.close();
    }
}

impl<TReq, TResp> Stream for Receiver<TReq, TResp> {
    type Item = RequestContext<TReq, TResp>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.rx.poll_next_unpin(cx)) {
            Some((req, tx)) => Poll::Ready(Some(RequestContext::new(req, tx))),
            // Stream has closed, so we're done
            None => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{executor::block_on, future};
    use std::fmt::Debug;
    use tari_test_utils::unpack_enum;
    use tower::ServiceExt;

    #[test]
    fn await_response_future_new() {
        let (tx, rx) = oneshot::channel::<Result<(), ()>>();
        tx.send(Ok(())).unwrap();
        block_on(TransportResponseFuture::new(rx)).unwrap().unwrap();
    }

    #[test]
    fn await_response_future_closed() {
        let err = block_on(TransportResponseFuture::<()>::closed()).unwrap_err();
        unpack_enum!(TransportChannelError::ChannelClosed = err);
    }

    async fn reply<TReq, TResp>(mut rx: Rx<TReq, TResp>, msg: TResp)
    where TResp: Debug {
        match rx.next().await {
            Some((_, tx)) => {
                tx.send(msg).unwrap();
            },
            _ => panic!("Expected receiver to have something to receive"),
        }
    }

    #[test]
    fn requestor_call() {
        let (tx, rx) = mpsc::unbounded();
        let requestor = SenderService::<_, _>::new(tx);

        let fut = future::join(requestor.oneshot("PING"), reply(rx, "PONG"));

        let msg = block_on(fut.map(|(r, _)| r.unwrap()));
        assert_eq!(msg, "PONG");
    }

    #[test]
    fn requestor_channel_closed() {
        let (requestor, mut request_stream) = super::unbounded::<_, ()>();
        request_stream.close();

        let err = block_on(requestor.oneshot(())).unwrap_err();
        // Behaviour change in futures 0.3 - the sender does not indicate that the channel is disconnected
        unpack_enum!(TransportChannelError::ChannelClosed = err);
    }

    #[test]
    fn request_response_request_abort() {
        let (mut requestor, mut request_stream) = super::unbounded::<_, &str>();

        block_on(future::join(
            async move {
                // `_` drops the response receiver, so when a reply is sent it will fail
                let _ = requestor.call("PING");
            },
            async move {
                let a = request_stream.next().await.unwrap();
                let req = a.reply_tx.send("PONG").unwrap_err();
                assert_eq!(req, "PONG");
            },
        ));
    }

    #[test]
    fn request_response_response_canceled() {
        let (mut requestor, mut request_stream) = super::unbounded::<_, &str>();

        block_on(future::join(
            async move {
                let err = requestor.ready_and().await.unwrap().call("PING").await.unwrap_err();
                assert_eq!(err, TransportChannelError::Canceled);
            },
            async move {
                let req = request_stream.next().await.unwrap();
                drop(req);
            },
        ));
    }

    #[test]
    fn request_response_success() {
        let (requestor, mut request_stream) = super::unbounded::<_, &str>();

        let (result, _) = block_on(future::join(requestor.oneshot("PING"), async move {
            let req = request_stream.next().await.unwrap();
            req.reply("PONG").unwrap();
        }));

        assert_eq!(result.unwrap(), "PONG");
    }
}
