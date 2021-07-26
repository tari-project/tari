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

pub mod mock;
#[cfg(test)]
mod tests;

use std::{
    net::{Shutdown, SocketAddr},
    sync::Arc,
};
use trust_dns_client::{
    client::{AsyncClient, AsyncDnssecClient},
    op::{DnsResponse, Query},
    proto::{
        udp::{UdpClientStream, UdpResponse, UdpSocket},
        xfer::DnsRequestOptions,
        DnsHandle,
    },
    rr::dnssec::TrustAnchor,
};

pub struct DnsClient<C> {
    inner: C,
    shutdown: Arc<Shutdown>,
}

impl DnsClient<AsyncDnssecClient<UdpResponse>> {
    pub async fn connect_secure(name_server: SocketAddr, trust_anchor: TrustAnchor) -> Result<Self, DnsSeedError> {
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

impl DnsClient<AsyncClient<UdpResponse>> {
    pub async fn connect(name_server: SocketAddr) -> Result<Self, DnsSeedError> {
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

impl<C> DnsClient<C>
where C: DnsHandle
{
    pub async fn lookup(&mut self, query: Query, options: DnsRequestOptions) -> Result<DnsResponse, DnsSeedError> {
        let resp = self.inner.lookup(query, options).await?;
        Ok(resp)
    }
}
