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

use super::DnsClientError;
use futures::future;
use std::{net::SocketAddr, sync::Arc};
use tari_shutdown::Shutdown;
use tokio::{net::UdpSocket, task};
use trust_dns_client::{
    client::{AsyncClient, AsyncDnssecClient},
    op::{DnsResponse, Query},
    proto::{
        rr::dnssec::TrustAnchor,
        udp::{UdpClientStream, UdpResponse},
        xfer::DnsRequestOptions,
        DnsHandle,
    },
    rr::{DNSClass, IntoName, RecordType},
    serialize::binary::BinEncoder,
};

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use trust_dns_client::{proto::xfer::DnsMultiplexerSerialResponse, rr::Record};

#[derive(Clone)]
pub enum DnsClient {
    Secure(Client<AsyncDnssecClient<UdpResponse>>),
    Normal(Client<AsyncClient<UdpResponse>>),
    #[cfg(test)]
    Mock(Client<AsyncClient<DnsMultiplexerSerialResponse>>),
}

impl DnsClient {
    pub async fn connect_secure(name_server: SocketAddr, trust_anchor: TrustAnchor) -> Result<Self, DnsClientError> {
        let client = Client::connect_secure(name_server, trust_anchor).await?;
        Ok(DnsClient::Secure(client))
    }

    pub async fn connect(name_server: SocketAddr) -> Result<Self, DnsClientError> {
        let client = Client::connect(name_server).await?;
        Ok(DnsClient::Normal(client))
    }

    #[cfg(test)]
    pub async fn connect_mock(records: HashMap<&'static str, Vec<Record>>) -> Result<Self, DnsClientError> {
        let client = Client::connect_mock(records).await?;
        Ok(DnsClient::Mock(client))
    }

    pub async fn lookup(&mut self, query: Query, options: DnsRequestOptions) -> Result<DnsResponse, DnsClientError> {
        use DnsClient::*;
        match self {
            Secure(ref mut client) => client.lookup(query, options).await,
            Normal(ref mut client) => client.lookup(query, options).await,
            #[cfg(test)]
            Mock(ref mut client) => client.lookup(query, options).await,
        }
    }

    pub async fn query_txt<T: IntoName>(&mut self, name: T) -> Result<Vec<String>, DnsClientError> {
        let mut query = Query::new();
        query
            .set_name(name.into_name()?)
            .set_query_class(DNSClass::IN)
            .set_query_type(RecordType::TXT);

        let response = self.lookup(query, Default::default()).await?;

        let records = response
            .messages()
            .flat_map(|msg| msg.answers())
            .map(|answer| {
                let data = answer.rdata();
                let mut buf = Vec::new();
                let mut decoder = BinEncoder::new(&mut buf);
                data.emit(&mut decoder).unwrap();
                buf
            })
            .filter_map(|txt| {
                if txt.is_empty() {
                    return None;
                }
                // Exclude the first length octet from the string result
                Some(String::from_utf8_lossy(&txt[1..]).to_string())
            })
            .collect();

        Ok(records)
    }
}

#[derive(Clone)]
pub struct Client<C> {
    inner: C,
    shutdown: Arc<Shutdown>,
}

impl Client<AsyncDnssecClient<UdpResponse>> {
    pub async fn connect_secure(name_server: SocketAddr, trust_anchor: TrustAnchor) -> Result<Self, DnsClientError> {
        let shutdown = Shutdown::new();
        let stream = UdpClientStream::<UdpSocket>::new(name_server);
        let (client, background) = AsyncDnssecClient::builder(stream)
            .trust_anchor(trust_anchor)
            .build()
            .await?;
        task::spawn(future::select(shutdown.to_signal(), background));

        Ok(Self {
            inner: client,
            shutdown: Arc::new(shutdown),
        })
    }
}

impl Client<AsyncClient<UdpResponse>> {
    pub async fn connect(name_server: SocketAddr) -> Result<Self, DnsClientError> {
        let shutdown = Shutdown::new();
        let stream = UdpClientStream::<UdpSocket>::new(name_server);
        let (client, background) = AsyncClient::connect(stream).await?;
        task::spawn(future::select(shutdown.to_signal(), background));

        Ok(Self {
            inner: client,
            shutdown: Arc::new(shutdown),
        })
    }
}

impl<C> Client<C>
where C: DnsHandle
{
    pub async fn lookup(&mut self, query: Query, options: DnsRequestOptions) -> Result<DnsResponse, DnsClientError> {
        let resp = self.inner.lookup(query, options).await?;
        Ok(resp)
    }
}

#[cfg(test)]
mod mock {
    use super::*;
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
        answers: HashMap<&'static str, Vec<Record>>,
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
            let name = req.queries()[0].name().to_string();
            let mut msg = Message::new();
            let answers = self.answers.get(name.as_str()).into_iter().flatten().cloned();
            msg.set_id(req.id()).add_answers(answers);
            Poll::Ready(Some(Ok(SerialMessage::new(
                msg.to_vec().unwrap(),
                self.name_server_addr(),
            ))))
        }
    }

    impl Client<AsyncClient<DnsMultiplexerSerialResponse>> {
        pub async fn connect_mock(answers: HashMap<&'static str, Vec<Record>>) -> Result<Self, ProtoError> {
            let (tx, rx) = mpsc::unbounded();
            let stream = future::ready(Ok(MockStream { receiver: rx, answers }));
            let (client, background) = AsyncClient::new(stream, Box::new(StreamHandle::new(tx)), None).await?;

            let shutdown = Shutdown::new();
            task::spawn(future::select(shutdown.to_signal(), background));
            Ok(Self {
                inner: client,
                shutdown: Arc::new(shutdown),
            })
        }
    }
}
