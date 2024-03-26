//  Copyright 2020, The Tari Project
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

use std::{fmt, marker::PhantomData, pin::Pin};

use bytes::BytesMut;
use futures::{
    ready,
    stream::BoxStream,
    task::{Context, Poll},
    Stream,
    StreamExt,
};
use pin_project::pin_project;
use prost::bytes::Buf;
use tokio::sync::mpsc;

use crate::{
    message::MessageExt,
    protocol::rpc::{Response, RpcStatus},
    Bytes,
};

pub trait IntoBody {
    fn into_body(self) -> Body;
}

impl<T: prost::Message> IntoBody for T {
    fn into_body(self) -> Body {
        Body::single(self.to_encoded_bytes())
    }
}

#[pin_project]
#[derive(Debug)]
pub struct Body {
    #[pin]
    kind: BodyKind,
    is_complete: bool,
    is_terminated: bool,
}

impl Body {
    pub fn single<T: Into<Bytes>>(body: T) -> Self {
        Self {
            kind: BodyKind::Single(Some(body.into())),
            is_complete: false,
            is_terminated: false,
        }
    }

    pub fn streaming<S>(stream: S) -> Self
    where S: Stream<Item = Result<Bytes, RpcStatus>> + Send + 'static {
        Self {
            kind: BodyKind::Streaming(stream.boxed()),
            is_complete: false,
            is_terminated: false,
        }
    }

    pub fn is_single(&self) -> bool {
        matches!(self.kind, BodyKind::Single(_))
    }

    pub fn is_streaming(&self) -> bool {
        matches!(self.kind, BodyKind::Streaming(_))
    }
}

impl Stream for Body {
    type Item = Result<BodyBytes, RpcStatus>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        let mut next_item = None;

        if !*this.is_complete {
            match this.kind.project() {
                BodyKindProj::Single(mut item) => {
                    next_item = item.take().map(Ok);
                    assert!(next_item.is_some(), "BodyKind::Single contained no message");
                    *this.is_complete = true;
                    *this.is_terminated = true;
                },
                BodyKindProj::Streaming(stream) => {
                    next_item = ready!(stream.poll_next(cx));
                    *this.is_complete = next_item.is_none();
                },
            }
        }

        match next_item.take() {
            Some(Ok(bytes)) => Poll::Ready(Some(Ok(BodyBytes::new(bytes, *this.is_terminated)))),
            Some(Err(err)) => {
                *this.is_complete = true;
                *this.is_terminated = true;
                Poll::Ready(Some(Err(err)))
            },
            None => {
                if *this.is_terminated {
                    Poll::Ready(None)
                } else {
                    *this.is_terminated = true;
                    Poll::Ready(Some(Ok(BodyBytes::terminated())))
                }
            },
        }
    }
}

#[pin_project(project = BodyKindProj)]
pub enum BodyKind {
    Single(#[pin] Option<Bytes>),
    Streaming(#[pin] BoxStream<'static, Result<Bytes, RpcStatus>>),
}

impl fmt::Debug for BodyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BodyKind::Single(b) => write!(
                f,
                "BodyKind::Single({})",
                b.as_ref()
                    .map(|b| format!("{} byte(s)", b.len()))
                    .unwrap_or_else(|| "<empty>".to_string())
            ),
            BodyKind::Streaming(_) => write!(f, "BodyKind::Streaming(BoxStream<...>)"),
        }
    }
}

pub struct BodyBytes(Option<Bytes>, bool);

impl BodyBytes {
    pub fn new(bytes: Bytes, is_terminated: bool) -> Self {
        Self(Some(bytes), is_terminated)
    }

    pub fn terminated() -> Self {
        Self(None, true)
    }

    pub fn is_finished(&self) -> bool {
        self.1
    }

    pub fn into_bytes_mut(self) -> BytesMut {
        self.0.map(|v| v.into_iter().collect()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.0.as_ref().map(|b| b.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.0.map(|bytes| bytes.into()).unwrap_or_default()
    }

    pub fn into_bytes(self) -> Option<Bytes> {
        self.0
    }
}

#[allow(clippy::from_over_into)]
impl Into<Bytes> for BodyBytes {
    fn into(self) -> Bytes {
        self.0.map(Bytes::from).unwrap_or_default()
    }
}

impl From<BodyBytes> for Vec<u8> {
    fn from(body: BodyBytes) -> Self {
        body.into_vec()
    }
}

#[allow(clippy::from_over_into)]
impl Into<BytesMut> for BodyBytes {
    fn into(self) -> BytesMut {
        self.into_bytes_mut()
    }
}

impl Buf for BodyBytes {
    fn remaining(&self) -> usize {
        self.0.as_ref().map(Buf::remaining).unwrap_or(0)
    }

    fn chunk(&self) -> &[u8] {
        self.0.as_ref().map(Buf::chunk).unwrap_or(&[])
    }

    fn advance(&mut self, cnt: usize) {
        if let Some(b) = self.0.as_mut() {
            b.advance(cnt);
        }
    }
}

#[derive(Debug)]
pub struct Streaming<T> {
    inner: mpsc::Receiver<Result<T, RpcStatus>>,
}

impl<T> Streaming<T> {
    pub fn new(inner: mpsc::Receiver<Result<T, RpcStatus>>) -> Self {
        Self { inner }
    }

    pub fn empty() -> Self {
        let (_, rx) = mpsc::channel(1);
        Self { inner: rx }
    }

    pub fn into_inner(self) -> mpsc::Receiver<Result<T, RpcStatus>> {
        self.inner
    }
}

impl<T: prost::Message> Stream for Streaming<T> {
    type Item = Result<Bytes, RpcStatus>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(Pin::new(&mut self.inner).poll_recv(cx)) {
            Some(result) => {
                let result = result.map(|msg| msg.to_encoded_bytes().into());
                Poll::Ready(Some(result))
            },
            None => Poll::Ready(None),
        }
    }
}

impl<T: prost::Message + 'static> IntoBody for Streaming<T> {
    fn into_body(self) -> Body {
        Body::streaming(self)
    }
}

#[derive(Debug)]
pub struct ClientStreaming<T> {
    inner: mpsc::Receiver<Result<Response<Bytes>, RpcStatus>>,
    _out: PhantomData<T>,
}

impl<T> ClientStreaming<T> {
    pub fn new(inner: mpsc::Receiver<Result<Response<Bytes>, RpcStatus>>) -> Self {
        Self {
            inner,
            _out: PhantomData,
        }
    }
}

impl<T: prost::Message + Default + Unpin> Stream for ClientStreaming<T> {
    type Item = Result<T, RpcStatus>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(Pin::new(&mut self.inner).poll_recv(cx)) {
            Some(Ok(resp)) => {
                // The streaming protocol dictates that an empty finish flag MUST be sent to indicate a terminated
                // stream. This empty response need not be emitted to downsteam consumers.
                if resp.flags.is_fin() {
                    return Poll::Ready(None);
                }
                let result = T::decode(resp.into_message()).map_err(Into::into);
                Poll::Ready(Some(result))
            },
            Some(Err(err)) => Poll::Ready(Some(Err(err))),
            None => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use futures::{stream, StreamExt};
    use prost::Message;

    use crate::{message::MessageExt, protocol::rpc::body::Body};

    #[tokio::test]
    async fn single_body() {
        let mut body = Body::single(123u32.to_encoded_bytes());
        let bytes = body.next().await.unwrap().unwrap();
        assert!(bytes.is_finished());
        assert_eq!(u32::decode(bytes).unwrap(), 123u32);
    }

    #[tokio::test]
    async fn streaming_body() {
        let body = Body::streaming(stream::repeat(Bytes::new()).map(Ok).take(10));
        let body = body.collect::<Vec<_>>().await;
        assert_eq!(body.len(), 11);

        let body_bytes = body.into_iter().map(|r| r.unwrap()).collect::<Vec<_>>();
        assert!(body_bytes.iter().take(10).all(|b| !b.is_finished()));
        assert!(body_bytes.last().unwrap().is_finished());
    }
}
