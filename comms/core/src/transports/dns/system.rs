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

use std::{
    fmt::Display,
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use futures::{FutureExt, future, future::BoxFuture};
use log::*;
use tokio::sync::Semaphore;

use super::{DnsResolver, DnsResolverError};
use crate::{
    multiaddr::{Multiaddr, Protocol},
    transports::dns::common,
};

const LOG_TARGET: &str = "comms::dns::system_resolver";
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);
/// Lookups that time out keep their blocking thread until the system resolver returns, so the number of lookups in
/// progress is capped.
const MAX_CONCURRENT_DNS_LOOKUPS: usize = 16;
static DNS_LOOKUP_PERMITS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_DNS_LOOKUPS);

/// Resolves DNS addresses using the system settings
pub struct SystemDnsResolver;

impl DnsResolver for SystemDnsResolver {
    fn resolve(&self, addr: Multiaddr) -> BoxFuture<'static, Result<SocketAddr, DnsResolverError>> {
        let protos = match common::extract_protocols(&addr) {
            Ok(p) => p,
            Err(err) => return boxed_ready(Err(err)),
        };

        match protos {
            (Protocol::Dns(domain), Protocol::Tcp(port)) | (Protocol::Dns4(domain), Protocol::Tcp(port)) => {
                dns_lookup(format!("{domain}:{port}")).boxed()
            },
            (Protocol::Ip4(host), Protocol::Tcp(port)) => boxed_ready(Ok((host, port).into())),
            (Protocol::Ip6(host), Protocol::Tcp(port)) => boxed_ready(Ok((host, port).into())),
            _ => boxed_ready(Err(DnsResolverError::UnsupportedAddress(addr))),
        }
    }
}

/// Performs an non-blocking DNS lookup of the given address. If [MAX_CONCURRENT_DNS_LOOKUPS] lookups are in progress,
/// waits for one to finish. Gives up after [DNS_LOOKUP_TIMEOUT], which covers the wait and the lookup; the blocking
/// thread cannot be cancelled and finishes in the background.
async fn dns_lookup<T>(addr: T) -> Result<SocketAddr, DnsResolverError>
where T: ToSocketAddrs + Display + Send + Sync + 'static {
    let lookup = async move {
        let permit = DNS_LOOKUP_PERMITS
            .acquire()
            .await
            .expect("DNS lookup semaphore is never closed");
        tokio::task::spawn_blocking(move || {
            // Held until the system resolver returns, even if the caller timed out
            let _permit = permit;
            debug!(target: LOG_TARGET, "Resolving address `{addr}` using system resolver");
            addr.to_socket_addrs()
                .map_err(|err| DnsResolverError::NameResolutionFailed {
                    source: err,
                    address_str: addr.to_string(),
                })?
                .next()
                .ok_or(DnsResolverError::DnsAddressNotFound)
        })
        .await?
    };
    tokio::time::timeout(DNS_LOOKUP_TIMEOUT, lookup)
        .await
        .map_err(|_| DnsResolverError::Timeout(DNS_LOOKUP_TIMEOUT))?
}

#[inline]
fn boxed_ready<T: Send + 'static>(t: T) -> BoxFuture<'static, T> {
    Box::pin(future::ready(t))
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn lookups_wait_for_a_free_slot() {
        let all_permits = DNS_LOOKUP_PERMITS
            .acquire_many(u32::try_from(MAX_CONCURRENT_DNS_LOOKUPS).unwrap())
            .await
            .unwrap();
        let lookup = tokio::spawn(dns_lookup("localhost:18189"));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!lookup.is_finished(), "the lookup did not wait for a slot");

        drop(all_permits);
        tokio::time::timeout(Duration::from_secs(5), lookup)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
