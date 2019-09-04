// Copyright 2019 The Tari Project

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

use derive_error::Error;
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
    Poll,
    Stream,
    StreamExt,
};
use std::pin::Pin;
use tower_service::Service;

/// Create a new Requester/Responder pair which wraps and calls the given service
pub fn unbounded<TReq, TResp>() -> (SenderService<TReq, TResp>, Receiver<TReq, TResp>) {
    let (tx, rx) = mpsc::unbounded();
    (SenderService::new(tx), Receiver::new(rx))
}

/// Receiver for a (Request, Reply) tuple, where Reply is a oneshot::Sender
pub type Rx<TReq, TRes> = mpsc::UnboundedReceiver<(TReq, oneshot::Sender<TRes>)>;
/// Sender for a (Request, Reply) tuple, where Reply is a oneshot::Sender
pub type Tx<TReq, TRes> = mpsc::UnboundedSender<(TReq, oneshot::Sender<TRes>)>;

/// Requester is sends requests on a given `Tx` sender and returns a
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
        self.tx.poll_ready(cx).map_err(Into::into)
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

#[derive(Debug, Error, Eq, PartialEq)]
pub enum TransportChannelError {
    /// Error occurred when sending
    SendError(SendError),
    /// Request was canceled
    Canceled,
    /// The response channel has closed
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
    use futures::executor::block_on;
    use futures_test::task::panic_context;
    use std::fmt::Debug;
    use tari_test_utils::counter_context;

    #[test]
    fn await_response_future_new() {
        let (tx, rx) = oneshot::channel::<Result<(), ()>>();
        tx.send(Ok(())).unwrap();

        let mut cx = panic_context();

        let mut fut = TransportResponseFuture::new(rx);
        match fut.poll_unpin(&mut cx) {
            Poll::Ready(res) => assert!(res.is_ok()),
            _ => panic!("expected future to be ready"),
        }
    }

    #[test]
    fn await_response_future_closed() {
        let mut fut = TransportResponseFuture::<()>::closed();

        let mut cx = panic_context();

        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Err(TransportChannelError::ChannelClosed)) => {},
            _ => panic!("unexpected poll result"),
        }
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
        let mut requestor = SenderService::<_, _>::new(tx);

        counter_context!(cx, wake_counter);

        let mut fut = requestor.call("PING");
        assert!(fut.poll_unpin(&mut cx).is_pending());

        block_on(reply(rx, "PONG"));

        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Ok(msg)) => assert_eq!(msg, "PONG"),
            x => panic!("Unexpected poll result: {:?}", x),
        }
        assert_eq!(wake_counter.get(), 1);
    }

    #[test]
    fn requestor_channel_closed() {
        let (mut requestor, request_stream) = super::unbounded::<_, ()>();
        drop(request_stream);

        let mut cx = panic_context();

        let mut fut = requestor.call(());
        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Err(TransportChannelError::ChannelClosed)) => {},
            x => panic!("Unexpected poll result: {:?}", x),
        }
    }

    #[test]
    fn request_response_success() {
        let (mut requestor, mut request_stream) = super::unbounded::<_, &str>();

        counter_context!(cx);

        let mut fut = requestor.call("PING");
        // Receive the RequestContext and reply
        match request_stream.poll_next_unpin(&mut cx) {
            Poll::Ready(Some(req)) => req.reply("PONG").unwrap(),
            _ => panic!("Unexpected Pending result from resonder"),
        }
        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Ok(msg)) => assert_eq!(msg, "PONG"),
            x => panic!("Unexpected poll result: {:?}", x),
        }
    }

    #[test]
    fn request_response_request_abort() {
        let (mut requestor, mut request_stream) = super::unbounded::<_, &str>();

        counter_context!(cx);

        // `_` drops the response receiver, so when a reply is sent it will fail
        let _ = requestor.call("PING");

        // Receive the RequestContext and reply
        let reply_result = match request_stream.poll_next_unpin(&mut cx) {
            Poll::Ready(Some(req)) => req.reply("PONG"),
            _ => panic!("Unexpected Pending result from request_stream"),
        };

        match reply_result {
            Err(req) => assert_eq!(req, "PONG"),
            x => panic!("Unexpected reply result: {:?}", x),
        }
    }

    #[test]
    fn request_response_response_canceled() {
        let (mut requestor, mut request_stream) = super::unbounded::<_, &str>();

        counter_context!(cx);

        let mut fut = requestor.call("PING");
        // Receive the RequestContext and reply
        match request_stream.poll_next_unpin(&mut cx) {
            Poll::Ready(Some(req)) => drop(req),
            _ => panic!("Unexpected Pending result from request_stream"),
        }

        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Err(TransportChannelError::Canceled)) => {},
            x => panic!("Unexpected poll result: {:?}", x),
        }
    }
}
