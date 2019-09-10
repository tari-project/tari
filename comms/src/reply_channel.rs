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
    task::Context,
    Future,
    FutureExt,
    Poll,
};
use std::pin::Pin;
use tower_service::Service;

/// Receiver for a (Request, Reply) tuple, where Reply is a oneshot::Sender
pub type Rx<TReq, TRes> = mpsc::UnboundedReceiver<RequestContext<TReq, TRes>>;
/// Sender for a (Request, Reply) tuple, where Reply is a oneshot::Sender
pub type Tx<TReq, TResp> = mpsc::UnboundedSender<RequestContext<TReq, TResp>>;

/// Create a new Requester/Responder pair which wraps and calls the given service
pub fn unbounded<TReq, TResp>() -> (SenderService<TReq, TResp>, Rx<TReq, TResp>) {
    let (tx, rx) = mpsc::unbounded();
    (SenderService::new(tx), rx)
}

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

    pub fn to_reply_send_error(err: SendError) -> ReplyChannelError {
        // SendError doesn't expose .kind :(
        if err.is_full() {
            ReplyChannelError::ChannelFull
        } else if err.is_disconnected() {
            ReplyChannelError::ChannelDisconnected
        } else {
            unreachable!();
        }
    }
}

impl<TReq, TRes> Clone for SenderService<TReq, TRes> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

impl<TReq, TRes> Service<TReq> for SenderService<TReq, TRes> {
    type Error = ReplyChannelError;
    type Future = ResponseFuture<TRes>;
    type Response = TRes;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.tx.poll_ready(cx).map_err(Self::to_reply_send_error)
    }

    fn call(&mut self, request: TReq) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        if self.tx.unbounded_send(RequestContext::new(request, tx)).is_ok() {
            ResponseFuture::new(rx)
        } else {
            // We're not able to send (rx closed) so return a future which resolves to
            // a ChannelClosed error
            ResponseFuture::closed()
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReplyChannelError {
    /// The channel has reached capacity
    ChannelFull,
    /// The channel has disconnected
    ChannelDisconnected,
    /// Request was canceled
    RequestCanceled,
    /// The response channel is closed
    Closed,
}

/// Response future for Results received over a given oneshot channel Receiver.
pub struct ResponseFuture<T> {
    rx: Option<oneshot::Receiver<T>>,
}

impl<T> ResponseFuture<T> {
    /// Create a new AwaitResponseFuture
    pub fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx: Some(rx) }
    }

    /// Create a closed ResponseFuture. If this is polled
    /// an ReplySendError::ChannelClosed error is returned.
    pub fn closed() -> Self {
        Self { rx: None }
    }
}

impl<T> Future for ResponseFuture<T> {
    type Output = Result<T, ReplyChannelError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.rx {
            Some(ref mut rx) => rx.poll_unpin(cx).map_err(|_| ReplyChannelError::RequestCanceled),
            None => Poll::Ready(Err(ReplyChannelError::Closed)),
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

#[cfg(test)]
mod test {
    use super::*;
    use futures::{executor::block_on, StreamExt};
    use std::fmt::Debug;
    use tari_test_utils::{counter_context, panic_context};

    #[test]
    fn response_future_new() {
        let (tx, rx) = oneshot::channel::<Result<(), ()>>();
        tx.send(Ok(())).unwrap();

        panic_context!(cx);

        let mut fut = ResponseFuture::new(rx);
        match fut.poll_unpin(&mut cx) {
            Poll::Ready(res) => assert!(res.is_ok()),
            _ => panic!("expected future to be ready"),
        }
    }

    #[test]
    fn response_future_closed() {
        let mut fut = ResponseFuture::<()>::closed();

        panic_context!(cx);

        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Err(ReplyChannelError::Closed)) => {},
            _ => panic!("unexpected poll result"),
        }
    }

    async fn reply<TReq, TResp>(mut rx: Rx<TReq, TResp>, msg: TResp)
    where TResp: Debug {
        match rx.next().await {
            Some(reply_cx) => {
                reply_cx.reply(msg).unwrap();
            },
            _ => panic!("Expected receiver to have something to receive"),
        }
    }

    #[test]
    fn requestor_call() {
        let (tx, rx) = mpsc::unbounded();
        let mut requestor = SenderService::<_, _>::new(tx);

        counter_context!(cx);

        let mut fut = requestor.call("PING");
        assert!(fut.poll_unpin(&mut cx).is_pending());

        block_on(reply(rx, "PONG"));

        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Ok(msg)) => assert_eq!(msg, "PONG"),
            x => panic!("Unexpected poll result: {:?}", x),
        }
    }

    #[test]
    fn requestor_channel_closed() {
        let (mut requestor, request_stream) = super::unbounded::<_, ()>();
        drop(request_stream);

        panic_context!(cx);

        let mut fut = requestor.call(());
        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Err(ReplyChannelError::Closed)) => {},
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
            Poll::Ready(Err(ReplyChannelError::RequestCanceled)) => {},
            x => panic!("Unexpected poll result: {:?}", x),
        }
    }
}
