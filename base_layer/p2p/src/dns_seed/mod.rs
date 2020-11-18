#[cfg(test)]
mod test;

mod error;
pub use error::DnsSeedError;

// Re-exports
pub use trust_dns_client::{
    error::ClientError,
    proto::error::ProtoError,
    rr::{IntoName, Name},
};

use crate::seed_peer::SeedPeer;
use futures::future;
use std::{net::SocketAddr, sync::Arc};
use tari_shutdown::Shutdown;
use tokio::{net::UdpSocket, task};
use trust_dns_client::{
    client::{AsyncClient, AsyncDnssecClient},
    op::Query,
    proto::{rr::dnssec::public_key::Rsa, udp::UdpResponse, DnsHandle},
    rr::{dnssec::TrustAnchor, DNSClass, RecordType},
    serialize::binary::BinEncoder,
    udp::UdpClientStream,
};

// This was copied from the trust-dns crate.
const ROOT_ANCHOR_ORIG: &[u8] = include_bytes!("roots/19036.rsa");
// This was generated from the `.` root domain in 10/2020.
const ROOT_ANCHOR_CURRENT: &[u8] = include_bytes!("roots/20326.rsa");

#[derive(Clone)]
pub struct DnsSeedResolver {
    inner: Inner,
}

#[derive(Clone)]
enum Inner {
    Secure(Resolver<AsyncDnssecClient<UdpResponse>>),
    Normal(Resolver<AsyncClient<UdpResponse>>),
}

#[inline]
fn default_trust_anchor() -> TrustAnchor {
    let mut anchor = TrustAnchor::new();
    anchor.insert_trust_anchor(&Rsa::from_public_bytes(ROOT_ANCHOR_ORIG).expect("Invalid ROOT_ANCHOR_ORIG"));
    anchor.insert_trust_anchor(&Rsa::from_public_bytes(ROOT_ANCHOR_CURRENT).expect("Invalid ROOT_ANCHOR_CURRENT"));
    anchor
}

impl DnsSeedResolver {
    /// Connect to DNS host with DNSSEC protection using default root DNSKEY public keys
    /// obtained from root DNS.
    ///
    /// ## Arguments
    /// -`name_server` - the DNS name server to use to resolve records
    pub async fn connect_secure(name_server: SocketAddr) -> Result<Self, DnsSeedError> {
        let resolver = Resolver::connect_secure(name_server, default_trust_anchor()).await?;
        Ok(Self {
            inner: Inner::Secure(resolver),
        })
    }

    /// Connect without DNSSEC protection
    ///
    /// ## Arguments
    /// -`name_server` - the DNS name server to use to resolve records
    pub async fn connect(name_server: SocketAddr) -> Result<Self, DnsSeedError> {
        let resolver = Resolver::connect(name_server).await?;
        Ok(Self {
            inner: Inner::Normal(resolver),
        })
    }
}

impl DnsSeedResolver {
    pub async fn resolve<T: IntoName>(&mut self, addr: T) -> Result<Vec<SeedPeer>, DnsSeedError> {
        match self.inner {
            Inner::Secure(ref mut inner) => inner.resolve(addr).await,
            Inner::Normal(ref mut inner) => inner.resolve(addr).await,
        }
    }
}

/// Resolves DNS TXT records and parses them into [`SeedPeer`]s.
///
/// Example TXT record:
/// ```text
/// 06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234
/// ```
#[derive(Clone)]
struct Resolver<C> {
    client: C,
    shutdown: Arc<Shutdown>,
}

impl Resolver<AsyncDnssecClient<UdpResponse>> {
    pub async fn connect_secure(name_server: SocketAddr, trust_anchor: TrustAnchor) -> Result<Self, DnsSeedError> {
        let shutdown = Shutdown::new();
        let stream = UdpClientStream::<UdpSocket>::new(name_server);
        let (client, background) = AsyncDnssecClient::builder(stream)
            .trust_anchor(trust_anchor)
            .build()
            .await?;
        task::spawn(future::select(shutdown.to_signal(), background));

        Ok(Self {
            client,
            shutdown: Arc::new(shutdown),
        })
    }
}

impl Resolver<AsyncClient<UdpResponse>> {
    pub async fn connect(name_server: SocketAddr) -> Result<Self, DnsSeedError> {
        let shutdown = Shutdown::new();
        let stream = UdpClientStream::<UdpSocket>::new(name_server);
        let (client, background) = AsyncClient::connect(stream).await?;
        task::spawn(future::select(shutdown.to_signal(), background));

        Ok(Self {
            client,
            shutdown: Arc::new(shutdown),
        })
    }
}

impl<C> Resolver<C>
where C: DnsHandle
{
    pub async fn resolve<T: IntoName>(&mut self, addr: T) -> Result<Vec<SeedPeer>, DnsSeedError> {
        let mut query = Query::new();
        query
            .set_name(addr.into_name()?)
            .set_query_class(DNSClass::IN)
            .set_query_type(RecordType::TXT);

        let response = self.client.lookup(query, Default::default()).await?;

        let peers = response
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
                let txt = String::from_utf8_lossy(&txt[1..]);
                txt.parse().ok()
            })
            .collect();

        Ok(peers)
    }
}
