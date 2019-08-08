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

use derive_error::Error;
use futures::{
    sync::{mpsc, oneshot},
    Future,
    Poll,
};
use tower_service::Service;

/// Receiver for a (Request, Reply) channel, where Reply is a oneshot::Sender
// TODO: Use this
#[cfg(test)]
pub type Rx<TReq, TRes> = mpsc::UnboundedReceiver<(TReq, oneshot::Sender<TRes>)>;
/// Sender for channel replies
pub type Tx<TReq, TRes> = mpsc::UnboundedSender<(TReq, oneshot::Sender<TRes>)>;

/// Trait for ServiceHandles
///
/// This is empty and simply serves as a marker trait, but could in future
/// contain common code for ServiceHandles.
pub trait ServiceHandle {}

/// ChannelServiceHandle is sends requests on a given `Tx` sender and returns a
/// ChannelResponseFuture which will resolve to a `Result`.
///
/// This implements `tower_service::Service`, therefore the `poll_ready` and `call`
/// methods should be used to make a request.
pub struct ChannelServiceHandle<TReq, TResp, TErr> {
    // Sender to the task
    tx: Tx<TReq, Result<TResp, TErr>>,
}

impl<TReq, TResp, TErr> ChannelServiceHandle<TReq, TResp, TErr> {
    /// Create a new ChannelServiceHandle
    pub fn new(tx: Tx<TReq, Result<TResp, TErr>>) -> Self {
        Self { tx }
    }
}

impl<TReq, TResp, TErr> ServiceHandle for ChannelServiceHandle<TReq, TResp, TErr> {}

impl<TReq, TResp, TErr> Service<TReq> for ChannelServiceHandle<TReq, TResp, TErr> {
    type Error = ServiceHandleError;
    type Future = ChannelResponseFuture<TResp, TErr>;
    type Response = Result<TResp, TErr>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, request: TReq) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        if self.tx.unbounded_send((request, tx)).is_ok() {
            ChannelResponseFuture::new(rx)
        } else {
            ChannelResponseFuture::closed()
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ServiceHandleError {
    /// Request was canceled
    Canceled,
    /// The response channel has closed
    ChannelClosed,
}

/// Response future for Results received over a given oneshot channel Receiver.
pub struct ChannelResponseFuture<T, E> {
    rx: Option<oneshot::Receiver<Result<T, E>>>,
}

impl<T, E> ChannelResponseFuture<T, E> {
    /// Create a new ChannelResponseFuture
    pub fn new(rx: oneshot::Receiver<Result<T, E>>) -> Self {
        Self { rx: Some(rx) }
    }

    /// Create a closed ChannelResponseFuture. If this is polled
    /// an ServiceHandleError::ChannelClosed error is returned.
    pub fn closed() -> Self {
        Self { rx: None }
    }
}

impl<T, E> Future for ChannelResponseFuture<T, E> {
    type Error = ServiceHandleError;
    type Item = Result<T, E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.rx {
            Some(ref mut rx) => rx.poll().map_err(|_| ServiceHandleError::Canceled),
            None => Err(ServiceHandleError::ChannelClosed),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{Async, Stream};
    use serde::export::fmt::Debug;

    #[test]
    fn channel_response_future_new() {
        let (tx, rx) = oneshot::channel::<Result<(), ()>>();
        tx.send(Ok(())).unwrap();
        let mut fut = ChannelResponseFuture::new(rx);
        match fut.poll().unwrap() {
            Async::Ready(res) => assert!(res.is_ok()),
            _ => panic!("expected future to be ready"),
        }
    }

    #[test]
    fn channel_response_future_closed() {
        let mut fut = ChannelResponseFuture::<(), ()>::closed();
        assert_eq!(fut.poll().unwrap_err(), ServiceHandleError::ChannelClosed);
    }

    fn reply<TReq, TResp, TErr>(mut rx: Rx<TReq, Result<TResp, TErr>>, msg: Result<TResp, TErr>)
    where
        TResp: Debug,
        TErr: Debug,
    {
        match rx.poll().unwrap() {
            Async::Ready(Some((_, tx))) => {
                tx.send(msg).unwrap();
            },
            _ => panic!("expected future to be ready"),
        }
    }

    #[test]
    fn channel_service_handle() {
        // task::current() used by unbounded channel
        let mut task = tokio_mock_task::MockTask::new();
        task.enter(|| {
            let (tx, rx) = mpsc::unbounded();
            let mut handle = ChannelServiceHandle::<_, _, ()>::new(tx);

            let mut fut = handle.call("PING");
            assert!(fut.poll().unwrap().is_not_ready());
            reply(rx, Ok("PONG"));
            match fut.poll().unwrap() {
                Async::Ready(Ok(msg)) => assert_eq!(msg, "PONG"),
                _ => panic!("Unexpected poll result"),
            }
        });
    }

    #[test]
    fn channel_service_handle_rx_drop() {
        let (tx, rx) = mpsc::unbounded();
        drop(rx);
        let mut handle = ChannelServiceHandle::<_, (), ()>::new(tx);

        let mut fut = handle.call(());
        assert_eq!(fut.poll().unwrap_err(), ServiceHandleError::ChannelClosed);
    }
}
