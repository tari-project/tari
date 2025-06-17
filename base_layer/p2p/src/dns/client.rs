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

use hickory_proto::{
    rr::IntoName,
    serialize::binary::{BinEncodable, BinEncoder},
    xfer::Protocol,
};
use hickory_resolver::{
    config::{NameServerConfig, ResolverConfig, ResolverOpts},
    name_server::TokioConnectionProvider,
    TokioResolver,
};
use log::*;
use tari_common::DnsNameServer;

use super::DnsClientError;

const LOG_TARGET: &str = "tari::p2p::dns::client";

#[derive(Clone)]
pub struct DnsClient {
    resolver: TokioResolver,
}

impl DnsClient {
    pub fn connect_secure(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let resolver = match name_server {
            DnsNameServer::System => TokioResolver::builder_tokio()?.build(),
            DnsNameServer::Custom { addr, dns_name } => Self::create_resolver(addr, dns_name, Protocol::Tls),
        };

        Ok(Self { resolver })
    }

    pub fn connect(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let resolver = match name_server {
            DnsNameServer::System => TokioResolver::builder_tokio()?.build(),
            DnsNameServer::Custom { addr, dns_name } => Self::create_resolver(addr, dns_name, Protocol::default()),
        };

        Ok(Self { resolver })
    }

    fn create_resolver(
        socket_addr: std::net::SocketAddr,
        tls_dns_name: Option<String>,
        protocol: Protocol,
    ) -> TokioResolver {
        let mut conf = ResolverConfig::new();
        conf.add_name_server(NameServerConfig {
            socket_addr,
            protocol,
            tls_dns_name,
            http_endpoint: None,
            trust_negative_responses: false,
            bind_addr: None,
        });

        let mut opts = ResolverOpts::default();
        opts.edns0 = true;
        opts.try_tcp_on_error = true;
        opts.timeout = std::time::Duration::from_secs(1);
        TokioResolver::builder_with_config(conf, TokioConnectionProvider::default())
            .with_options(opts)
            .build()
    }

    pub async fn query_txt<T: IntoName>(&mut self, name: T) -> Result<Vec<String>, DnsClientError> {
        let lookup = self.resolver.txt_lookup(name).await?;

        let records = lookup
            .iter()
            .map(|answer| {
                // pub key + onion is 136 bytes
                let mut buf = Vec::with_capacity(136);
                let mut decoder = BinEncoder::new(&mut buf);
                answer.emit(&mut decoder)?;
                Ok(buf)
            })
            .filter_map(|txt| {
                txt.map(|txt| {
                    if txt.is_empty() {
                        return None;
                    }
                    let len = txt[0] as usize;
                    if len == 0 {
                        return None;
                    }
                    if len >= txt.len() {
                        warn!(
                            target: LOG_TARGET,
                            "Length byte {} is greater than the length of the TXT record {}",
                            len,
                            txt.len()
                        );
                        return None;
                    }
                    // Exclude the first length byte from the string result
                    Some(String::from_utf8_lossy(&txt[1..=len]).to_string())
                })
                .inspect_err(|e| {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to parse DNS TXT record. Error: {}", e
                    );
                })
                .transpose()
            })
            .collect::<Result<_, DnsClientError>>()?;

        Ok(records)
    }
}
