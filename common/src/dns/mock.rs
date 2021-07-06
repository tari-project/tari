//  Copyright 2021, The Tari Project
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

use crate::dns_seed::PeerSeedResolver;
use futures::{channel::mpsc, future, Stream, StreamExt};
use std::{
    fmt,
    fmt::Display,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tari_shutdown::Shutdown;
use tokio::task;
use trust_dns_client::{
    client::AsyncClient,
    op::Message,
    proto::{
        error::ProtoError,
        xfer::{DnsClientStream, DnsMultiplexerSerialResponse, SerialMessage},
        StreamHandle,
    },
    rr::Record,
};

pub struct MockStream {
    receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    answers: Vec<Record>,
}

impl DnsClientStream for MockStream {
    fn name_server_addr(&self) -> SocketAddr {
        ([0u8, 0, 0, 0], 53).into()
    }
}
impl Display for MockStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MockStream")
    }
}

impl Stream for MockStream {
    type Item = Result<SerialMessage, ProtoError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let req = match futures::ready!(self.receiver.poll_next_unpin(cx)) {
            Some(r) => r,
            None => return Poll::Ready(None),
        };
        let req = Message::from_vec(&req).unwrap();
        let mut msg = Message::new();
        msg.set_id(req.id()).add_answers(self.answers.iter().cloned());
        Poll::Ready(Some(Ok(SerialMessage::new(
            msg.to_vec().unwrap(),
            self.name_server_addr(),
        ))))
    }
}

impl PeerSeedResolver<AsyncClient<DnsMultiplexerSerialResponse>> {
    pub async fn connect_test(answers: Vec<Record>) -> Result<Self, ProtoError> {
        let (tx, rx) = mpsc::unbounded();
        let stream = future::ready(Ok(MockStream { receiver: rx, answers }));
        let (client, background) = AsyncClient::new(stream, Box::new(StreamHandle::new(tx)), None).await?;

        let shutdown = Shutdown::new();
        task::spawn(future::select(shutdown.to_signal(), background));
        Ok(Self {
            client,
            shutdown: Arc::new(shutdown),
        })
    }
}
