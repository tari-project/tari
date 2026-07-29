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

use std::sync::Arc;

use hickory_proto::rr::{IntoName, RData};
use hickory_resolver::{
    TokioResolver,
    config::{NameServerConfig, ResolverConfig, ResolverOpts},
};
use log::*;
use tari_common::DnsNameServer;

use super::DnsClientError;

const LOG_TARGET: &str = "tari::p2p::dns::client";

#[derive(Clone)]
pub struct DnsClient {
    resolver: TokioResolver,
    /// When set, an answer that is not cryptographically proven authentic by DNSSEC is rejected rather than
    /// returned to the caller.
    require_dnssec: bool,
}

impl DnsClient {
    /// Connect to the given name server and perform DNSSEC validation on every answer.
    ///
    /// Validation is anchored on the built-in root DNSKEY trust anchors and is applied end-to-end, so it holds even
    /// if the recursive resolver we talk to is malicious. Answers that are not proven authentic are rejected (see
    /// [`DnsClient::query_txt`]). A `Custom` name server is additionally contacted over DNS-over-TLS; a `System` name
    /// server is contacted over whatever transport the operating system configured, which does not weaken the DNSSEC
    /// guarantee.
    pub fn connect_secure(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let resolver = match name_server {
            DnsNameServer::System => TokioResolver::builder_tokio()?
                .with_options(Self::resolver_opts(true))
                .build()?,
            DnsNameServer::Custom { addr, dns_name } => {
                Self::create_resolver(NameServerConfig::tls(addr.ip(), Arc::from(dns_name.as_str())), true)?
            },
        };

        Ok(Self {
            resolver,
            require_dnssec: true,
        })
    }

    /// Connect to the given name server without any DNSSEC validation. Answers are trusted as-is, so a compromised
    /// or on-path resolver is able to forge them.
    pub fn connect(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let resolver = match name_server {
            DnsNameServer::System => TokioResolver::builder_tokio()?
                .with_options(Self::resolver_opts(false))
                .build()?,
            DnsNameServer::Custom { addr, dns_name: _ } => {
                Self::create_resolver(NameServerConfig::udp(addr.ip()), false)?
            },
        };

        Ok(Self {
            resolver,
            require_dnssec: false,
        })
    }

    fn create_resolver(name_server: NameServerConfig, validate: bool) -> Result<TokioResolver, DnsClientError> {
        let mut conf = ResolverConfig::default();
        conf.add_name_server(name_server);

        Ok(TokioResolver::builder_with_config(conf, Default::default())
            .with_options(Self::resolver_opts(validate))
            .build()?)
    }

    fn resolver_opts(validate: bool) -> ResolverOpts {
        let mut opts = ResolverOpts::default();
        opts.edns0 = true;
        opts.try_tcp_on_error = true;
        opts.timeout = std::time::Duration::from_secs(1);
        // This is what actually walks the DNSSEC chain of trust. Without it the resolver never sets the DO bit and
        // every record comes back `Indeterminate`, i.e. unvalidated.
        opts.validate = validate;
        opts
    }

    /// Queries the TXT records for the given name.
    ///
    /// If this client was created with [`DnsClient::connect_secure`], any TXT record that is not proven authentic by
    /// DNSSEC causes the whole query to fail. Failing closed is deliberate: these records determine the peers a node
    /// bootstraps from, so silently accepting an unvalidated answer would hand that choice to whoever is able to
    /// answer the query.
    pub async fn query_txt<T: IntoName>(&mut self, name: T) -> Result<Vec<String>, DnsClientError> {
        let name = name.into_name()?;
        let lookup = self.resolver.txt_lookup(name.clone()).await?;

        let mut records = Vec::new();
        for record in lookup.answers() {
            let RData::TXT(txt) = &record.data else {
                // DNSSEC signature records are returned alongside the answer when validation is enabled.
                if !record.record_type().is_dnssec() {
                    warn!(
                        target: LOG_TARGET,
                        "Unexpected record type in TXT lookup: {:?}",
                        record.data
                    );
                }
                continue;
            };

            let proof = record.proof;
            if self.require_dnssec && !proof.is_secure() {
                warn!(
                    target: LOG_TARGET,
                    "DNSSEC validation failed for TXT record `{name}`: proof is {proof}"
                );
                return Err(DnsClientError::DnssecValidationFailed {
                    name: name.to_string(),
                    proof: proof.to_string(),
                });
            }

            let text = txt
                .txt_data
                .iter()
                .map(|chunk| String::from_utf8_lossy(chunk).to_string())
                .collect::<String>();
            if !text.is_empty() {
                records.push(text);
            }
        }

        Ok(records)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// A name server that is never contacted. `Custom` is used rather than `System` so that these tests do not depend
    /// on the host having a usable `/etc/resolv.conf` — building a `System` resolver reads it and fails without one.
    fn test_name_server() -> DnsNameServer {
        DnsNameServer::custom("127.0.0.1:53".parse().unwrap(), "localhost".to_string())
    }

    #[test]
    fn connect_secure_requires_dnssec() {
        let client = DnsClient::connect_secure(test_name_server()).unwrap();
        assert!(client.require_dnssec);
    }

    #[test]
    fn connect_does_not_require_dnssec() {
        let client = DnsClient::connect(test_name_server()).unwrap();
        assert!(!client.require_dnssec);
    }

    #[test]
    fn resolver_opts_only_validates_when_asked() {
        assert!(DnsClient::resolver_opts(true).validate);
        assert!(!DnsClient::resolver_opts(false).validate);
    }

    #[tokio::test]
    #[ignore = "Useful for developer testing but requires network access to DNSSEC signed zones."]
    async fn it_rejects_records_that_are_not_provably_authentic() {
        // github.com has TXT records but is not signed, so no chain of trust can be built for it
        let mut client = DnsClient::connect_secure(DnsNameServer::System).unwrap();
        assert!(matches!(
            client.query_txt("github.com").await.unwrap_err(),
            DnsClientError::DnssecValidationFailed { .. }
        ));

        // dnssec-failed.org is intentionally served with broken signatures
        let mut client = DnsClient::connect_secure(DnsNameServer::System).unwrap();
        client.query_txt("dnssec-failed.org").await.unwrap_err();

        // ...while the seed records are signed and validate
        let mut client = DnsClient::connect_secure(DnsNameServer::System).unwrap();
        assert!(!client.query_txt("seeds.esmeralda.tari.com").await.unwrap().is_empty());
    }
}
