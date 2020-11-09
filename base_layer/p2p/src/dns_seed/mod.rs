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

use futures::future;
use log::*;
use std::{future::Future, net::SocketAddr, str::FromStr};
use tari_comms::{multiaddr::Multiaddr, types::CommsPublicKey};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::{net::UdpSocket, task};
use trust_dns_client::{
    client::AsyncClient,
    op::{DnsResponse, Query},
    proto::{udp::UdpResponse, DnsHandle},
    rr::{DNSClass, RecordType},
    serialize::binary::BinEncoder,
    udp::UdpClientStream,
};

const LOG_TARGET: &str = "p2p::dns_seed";

/// Resolves DNS TXT records and parses them into [`SeedPeer`]s.
///
/// Example TXT record:
/// ```text
/// 06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234
/// ```
pub struct DnsSeedResolver<R>
where R: Future<Output = Result<DnsResponse, ProtoError>> + Send + Unpin + 'static
{
    client: AsyncClient<R>,
}

impl DnsSeedResolver<UdpResponse> {
    pub async fn connect(name_server: SocketAddr, shutdown_signal: ShutdownSignal) -> Result<Self, DnsSeedError> {
        let stream = UdpClientStream::<UdpSocket>::new(name_server);
        let (client, background) = AsyncClient::connect(stream).await?;
        task::spawn(future::select(shutdown_signal, background));

        Ok(Self { client })
    }
}

impl<R> DnsSeedResolver<R>
where R: Future<Output = Result<DnsResponse, ProtoError>> + Send + Unpin + 'static
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
                let txt = String::from_utf8_lossy(&txt[1..]);
                trace!(target: LOG_TARGET, "Processing TXT record {}", txt);
                let mut parts = txt.split("::");
                let public_key = parts.next()?;
                trace!(target: LOG_TARGET, "TXT record has a first part");
                let public_key = CommsPublicKey::from_hex(&public_key).ok()?;
                trace!(target: LOG_TARGET, "TXT record has valid public key `{}`", public_key);
                let addresses = parts.map(Multiaddr::from_str).collect::<Result<Vec<_>, _>>().ok()?;
                if addresses.is_empty() || addresses.iter().any(|a| a.len() == 0) {
                    return None;
                }
                trace!(target: LOG_TARGET, "TXT record has {} valid addresses", addresses.len());
                Some(SeedPeer { public_key, addresses })
            })
            .collect();

        Ok(peers)
    }
}

/// Parsed information from a DNS seed record
#[derive(Debug, Clone)]
pub struct SeedPeer {
    pub public_key: CommsPublicKey,
    pub addresses: Vec<Multiaddr>,
}
