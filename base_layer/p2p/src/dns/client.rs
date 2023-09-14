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

use std::{sync::Arc, time::Duration};

use futures::{future, FutureExt};
use rustls::{ClientConfig, RootCertStore};
use tari_common::DnsNameServer;
use tari_shutdown::Shutdown;
use tokio::task;
use trust_dns_client::{
    client::{AsyncClient, AsyncDnssecClient, ClientHandle},
    op::Query,
    proto::{
        error::ProtoError,
        iocompat::AsyncIoTokioAsStd,
        rr::dnssec::{SigSigner, TrustAnchor},
        rustls::tls_client_connect,
        xfer::DnsResponse,
        DnsHandle,
        DnsMultiplexer,
    },
    rr::{DNSClass, IntoName, RecordType},
    serialize::binary::{BinEncodable, BinEncoder},
};

use super::DnsClientError;
#[cfg(test)]
use crate::dns::mock::{DefaultOnSend, MockClientHandle};
use crate::dns::roots;

#[derive(Clone)]
pub enum DnsClient {
    Secure(Client<AsyncDnssecClient>),
    Normal(Client<AsyncClient>),
    #[cfg(test)]
    Mock(Client<MockClientHandle<DefaultOnSend, ProtoError>>),
}

impl DnsClient {
    pub async fn connect_secure(name_server: DnsNameServer, trust_anchor: TrustAnchor) -> Result<Self, DnsClientError> {
        let client = Client::connect_secure(name_server, trust_anchor).await?;
        Ok(DnsClient::Secure(client))
    }

    pub async fn connect(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let client = Client::connect(name_server).await?;
        Ok(DnsClient::Normal(client))
    }

    #[cfg(test)]
    pub async fn connect_mock(messages: Vec<Result<DnsResponse, ProtoError>>) -> Result<Self, DnsClientError> {
        let client = Client::connect_mock(messages).await?;
        Ok(DnsClient::Mock(client))
    }

    pub async fn lookup(&mut self, query: Query) -> Result<DnsResponse, DnsClientError> {
        #[cfg(test)]
        use DnsClient::Mock;
        use DnsClient::{Normal, Secure};
        match self {
            Secure(ref mut client) => client.lookup(query).await,
            Normal(ref mut client) => client.lookup(query).await,
            #[cfg(test)]
            Mock(ref mut client) => client.lookup(query).await,
        }
    }

    pub async fn query_txt<T: IntoName>(&mut self, name: T) -> Result<Vec<String>, DnsClientError> {
        let mut query = Query::new();
        query
            .set_name(name.into_name()?)
            .set_query_class(DNSClass::IN)
            .set_query_type(RecordType::TXT);

        let responses = self.lookup(query).await?;

        let records = responses
            .answers()
            .iter()
            .map(|answer| {
                let data = if let Some(d) = answer.data() {
                    d
                } else {
                    return Err(DnsClientError::NoRecordDataPresent);
                };
                let mut buf = Vec::new();
                let mut decoder = BinEncoder::new(&mut buf);
                data.emit(&mut decoder).unwrap();
                Ok(buf)
            })
            .collect::<Result<Vec<Vec<u8>>, DnsClientError>>()?
            .iter()
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
    _shutdown: Arc<Shutdown>,
}

impl Client<AsyncDnssecClient> {
    pub async fn connect_secure(name_server: DnsNameServer, trust_anchor: TrustAnchor) -> Result<Self, DnsClientError> {
        let shutdown = Shutdown::new();
        let timeout = Duration::from_secs(5);
        let (stream, handle) = tls_client_connect::<AsyncIoTokioAsStd<tokio::net::TcpStream>>(
            name_server.addr,
            name_server.dns_name,
            default_client_config(),
        );

        let dns_muxer = DnsMultiplexer::<_, SigSigner>::with_timeout(stream, handle, timeout, None);
        let (client, background) = AsyncDnssecClient::builder(dns_muxer)
            .trust_anchor(trust_anchor)
            .build()
            .await?;

        task::spawn(future::select(shutdown.to_signal(), background.fuse()));

        Ok(Self {
            inner: client,
            _shutdown: Arc::new(shutdown),
        })
    }
}

impl Client<AsyncClient> {
    pub async fn connect(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let shutdown = Shutdown::new();

        let timeout = Duration::from_secs(5);
        let (stream, handle) = tls_client_connect::<AsyncIoTokioAsStd<tokio::net::TcpStream>>(
            name_server.addr,
            name_server.dns_name,
            default_client_config(),
        );

        let (client, background) = AsyncClient::with_timeout(stream, handle, timeout, None).await?;
        task::spawn(future::select(shutdown.to_signal(), background.fuse()));

        Ok(Self {
            inner: client,
            _shutdown: Arc::new(shutdown),
        })
    }
}

impl<C> Client<C>
where C: DnsHandle<Error = ProtoError>
{
    pub async fn lookup(&mut self, query: Query) -> Result<DnsResponse, DnsClientError> {
        let client_resp = self
            .inner
            .query(query.name().clone(), query.query_class(), query.query_type())
            .await?;

        Ok(client_resp)
    }
}

fn default_client_config() -> Arc<ClientConfig> {
    let mut root_store = RootCertStore::empty();
    root_store.add_trust_anchors(roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
        rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(ta.subject, ta.spki, ta.name_constraints)
    }));

    let client_config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Arc::new(client_config)
}

#[cfg(test)]
mod mock {
    use std::sync::Arc;

    use tari_shutdown::Shutdown;
    use trust_dns_client::proto::error::ProtoError;

    use super::*;
    use crate::dns::mock::{DefaultOnSend, MockClientHandle};

    impl Client<MockClientHandle<DefaultOnSend, ProtoError>> {
        pub async fn connect_mock(messages: Vec<Result<DnsResponse, ProtoError>>) -> Result<Self, ProtoError> {
            let client = MockClientHandle::mock(messages);
            Ok(Self {
                inner: client,
                _shutdown: Arc::new(Shutdown::new()),
            })
        }
    }
}
