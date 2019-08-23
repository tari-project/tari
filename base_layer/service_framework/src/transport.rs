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
    stream::FuturesUnordered,
    sync::{mpsc, oneshot},
    Async,
    Future,
    Poll,
    Stream,
};
use std::marker::PhantomData;
use tower_service::Service;

/// Create a new Requester/Responder pair which wraps and calls the given service
pub fn channel<S, TReq>(service: S) -> (Requester<TReq, S::Response>, Responder<S, TReq>)
where S: Service<TReq> {
    let (tx, rx) = mpsc::unbounded();
    (Requester::new(tx), Responder::new(rx, service))
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
pub struct Requester<TReq, TRes> {
    /// Used to send the request
    tx: Tx<TReq, TRes>,
}

impl<TReq, TRes> Requester<TReq, TRes> {
    /// Create a new Requester
    pub fn new(tx: Tx<TReq, TRes>) -> Self {
        Self { tx }
    }
}

impl<TReq, TRes> Clone for Requester<TReq, TRes> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

impl<TReq, TRes> Service<TReq> for Requester<TReq, TRes> {
    type Error = AwaitResponseError;
    type Future = AwaitResponseFuture<TRes>;
    type Response = TRes;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, request: TReq) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        if self.tx.unbounded_send((request, tx)).is_ok() {
            AwaitResponseFuture::new(rx)
        } else {
            // We're not able to send (rx closed) so return a future which resolves to
            // a ChannelClosed error
            AwaitResponseFuture::closed()
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AwaitResponseError {
    /// Request was canceled
    Canceled,
    /// The response channel has closed
    ChannelClosed,
}

/// Response future for Results received over a given oneshot channel Receiver.
pub struct AwaitResponseFuture<T> {
    rx: Option<oneshot::Receiver<T>>,
}

impl<T> AwaitResponseFuture<T> {
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

impl<T> Future for AwaitResponseFuture<T> {
    type Error = AwaitResponseError;
    type Item = T;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.rx {
            Some(ref mut rx) => rx.poll().map_err(|_| AwaitResponseError::Canceled),
            None => Err(AwaitResponseError::ChannelClosed),
        }
    }
}

/// This wraps an inner future and sends the result on a oneshot sender
pub struct ResponseFuture<F, T, E> {
    inner: F,
    tx: Option<oneshot::Sender<T>>,
    _err: PhantomData<E>,
}

impl<F, T, E> ResponseFuture<F, T, E> {
    /// Create a new ResponderFuture from a Future and a oneshot Sender
    pub fn new(inner: F, tx: oneshot::Sender<T>) -> Self {
        Self {
            inner,
            tx: Some(tx),
            _err: PhantomData,
        }
    }
}

impl<F, T, E> Future for ResponseFuture<F, T, E>
where F: Future<Item = T, Error = E>
{
    type Error = E;
    type Item = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self
            .tx
            .as_mut()
            .expect("ResponderFuture cannot be polled after inner future is ready")
            .poll_cancel()
        {
            Err(_) | Ok(Async::Ready(_)) => {
                // The receiver will not receive a response,
                // so let's abandon processing the inner future
                return Ok(().into());
            },
            _ => {},
        }

        // Progress on the inner future
        let res = try_ready!(self.inner.poll());

        let tx = self.tx.take().expect("cannot happen (ResponderFuture)");
        // Send the response
        // If we get an error here, the receiver cancelled/closed so discard the Result
        // TODO: Add tracing logs
        let _ = tx.send(res);
        Ok(().into())
    }
}

/// Future that calls a given Service with requests that are received from a mpsc Receiver
/// and sends the response back on the requests oneshot channel.
///
/// As requests come through the futures resulting from Service::call is added to a pending queue
/// for concurrent processing.
pub struct Responder<S, TReq>
where S: Service<TReq>
{
    service: S,
    rx: Rx<TReq, S::Response>,
    in_flight: usize,
    pending: FuturesUnordered<ResponseFuture<S::Future, S::Response, S::Error>>,
}

impl<S, TReq> Responder<S, TReq>
where S: Service<TReq>
{
    /// Create a new Responder
    pub fn new(rx: Rx<TReq, S::Response>, service: S) -> Self {
        Self {
            rx,
            service,
            in_flight: 0,
            pending: FuturesUnordered::new(),
        }
    }
}

impl<S, TReq> Future for Responder<S, TReq>
where S: Service<TReq>
{
    type Error = ();
    type Item = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            if !self.pending.is_empty() {
                loop {
                    // Make progress on the pending futures
                    match self.pending.poll() {
                        // Continue polling the pending futures until empty or NotReady
                        Ok(Async::Ready(Some(_))) => {
                            self.in_flight -= 1;
                            continue;
                        },
                        Err(_) => {
                            // Service error occurred
                            // TODO: Deal with this error
                            self.in_flight -= 1;
                            continue;
                        },
                        _ => break,
                    }
                }
            }

            // Check the service is ready
            // TODO: Log an error returned from a service or deal with it in some way
            match self.service.poll_ready().map_err(|_| ())? {
                Async::Ready(_) => {
                    // Receive any new requests
                    match self.rx.poll().expect("poll error not possible for unbounded receiver") {
                        Async::Ready(Some((req, tx))) => {
                            // Call the service and add the resultant future to the pending queue
                            let fut = ResponseFuture::new(self.service.call(req), tx);
                            self.in_flight += 1;
                            self.pending.push(fut);
                        },
                        // Stream has closed, so we're done
                        Async::Ready(None) => {
                            return Ok(Async::Ready(()));
                        },
                        Async::NotReady => {
                            return Ok(Async::NotReady);
                        },
                    }
                },
                Async::NotReady => return Ok(Async::NotReady),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{
        future::{self, Either},
        Async,
        Stream,
    };
    use std::{fmt::Debug, iter::repeat_with};
    use tokio_mock_task::MockTask;
    use tower_util::service_fn;

    #[test]
    fn await_response_future_new() {
        let (tx, rx) = oneshot::channel::<Result<(), ()>>();
        tx.send(Ok(())).unwrap();
        let mut fut = AwaitResponseFuture::new(rx);
        match fut.poll().unwrap() {
            Async::Ready(res) => assert!(res.is_ok()),
            _ => panic!("expected future to be ready"),
        }
    }

    #[test]
    fn await_response_future_closed() {
        let mut fut = AwaitResponseFuture::<()>::closed();
        assert_eq!(fut.poll().unwrap_err(), AwaitResponseError::ChannelClosed);
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
    fn requestor_call() {
        // task::current() used by unbounded channel
        let mut task = MockTask::new();
        task.enter(|| {
            let (tx, rx) = mpsc::unbounded();
            let mut requestor = Requester::<_, _>::new(tx);

            let mut fut = requestor.call("PING");
            assert!(fut.poll().unwrap().is_not_ready());
            reply::<_, _, ()>(rx, Ok("PONG"));
            match fut.poll().unwrap() {
                Async::Ready(Ok(msg)) => assert_eq!(msg, "PONG"),
                _ => panic!("Unexpected poll result"),
            }
        });
    }

    #[test]
    fn requestor_channel_closed() {
        let (mut requestor, responder) = super::channel(service_fn(|_: ()| future::ok::<_, ()>(())));
        drop(responder);

        let mut fut = requestor.call(());
        assert_eq!(fut.poll().unwrap_err(), AwaitResponseError::ChannelClosed);
    }

    #[test]
    fn channel_request_response() {
        let mut task = MockTask::new();
        task.enter(|| {
            let (mut requestor, mut responder) = super::channel(service_fn(|_| future::ok::<_, ()>("PONG")));

            let mut fut = requestor.call("PING");
            // Allow responder to receive the request and respond
            let _ = responder.poll();
            match fut.poll().unwrap() {
                Async::Ready(msg) => assert_eq!(msg, "PONG"),
                Async::NotReady => panic!("expected future to be Ready"),
            }
        });
    }

    #[test]
    fn channel_responder_inflight_out_of_order() {
        let mut task = MockTask::new();
        task.enter(|| {
            let (tx, rx) = oneshot::channel();
            struct EchoService(Option<oneshot::Receiver<()>>);
            impl Service<String> for EchoService {
                type Error = ();
                type Future = impl Future<Item = String, Error = ()>;
                type Response = String;

                fn poll_ready(&mut self) -> Poll<(), Self::Error> {
                    Ok(().into())
                }

                fn call(&mut self, msg: String) -> Self::Future {
                    if let Some(rx) = self.0.take() {
                        Either::A(rx.map(|_| msg).map_err(|_| ()))
                    } else {
                        // Called more than once, return a future which resolves immediately
                        Either::B(future::ok(msg))
                    }
                }
            }

            let service = EchoService(Some(rx));
            let (mut requestor, mut responder) = super::channel(service);

            // Make concurrent requests to the service
            let mut fut1 = requestor.call("first".to_string());
            let mut fut2 = requestor.call("second".to_string());
            assert_eq!(responder.in_flight, 0);

            // When Responder is polled it will:
            // Receive all the requests,
            // call the service and then,
            // poll pending futures (that is, response is sent to fut2)
            responder.poll().unwrap();
            assert!(fut1.poll().unwrap().is_not_ready());
            match fut2.poll().unwrap() {
                Async::Ready(v) => assert_eq!(v, "second"),
                _ => panic!(),
            }
            assert_eq!(responder.in_flight, 1);

            // Signal the first call to be Ready so that the result is sent to fut1
            tx.send(()).unwrap();
            // Progress on the pending futures (i.e response for fut1)
            responder.poll().unwrap();

            match fut1.poll().unwrap() {
                Async::Ready(v) => assert_eq!(v, "first"),
                _ => panic!(),
            }
        });
    }

    #[test]
    fn channel_responder_inflight() {
        let mut task = MockTask::new();
        task.enter(|| {
            let service = service_fn(|rx: oneshot::Receiver<()>| rx);
            let (mut requestor, mut responder) = super::channel(service);

            // Make 100 concurrent requests
            let (txs, futs): (Vec<_>, Vec<_>) = repeat_with(|| {
                let (tx, rx) = oneshot::channel();
                (tx, requestor.call(rx))
            })
            .take(100)
            .unzip();

            // Call the service and collect the futures
            responder.poll().unwrap();
            // Check that all are in-flight
            assert_eq!(responder.in_flight, 100);
            // Send the ready signal
            for tx in txs.into_iter() {
                tx.send(()).unwrap();
            }
            // Ensure progress is made on tasks
            responder.poll().unwrap();
            // Ensure we have no more unresolved requests
            assert_eq!(responder.in_flight, 0);
            // Check that all futures have completed
            assert!(futs.into_iter().all(|mut f| f.poll().unwrap().is_ready()));
        });
    }
}
