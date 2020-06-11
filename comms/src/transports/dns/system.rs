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

use super::{DnsResolver, DnsResolverError};
use crate::{
    multiaddr::{Multiaddr, Protocol},
    transports::dns::common,
};
use futures::{future, future::BoxFuture, FutureExt};
use log::*;
use std::{
    fmt::Display,
    net::{SocketAddr, ToSocketAddrs},
};
use tokio::task::spawn_blocking;

const LOG_TARGET: &str = "comms::dns::system_resolver";

/// Resolves DNS addresses using the system settings
pub struct SystemDnsResolver;

impl DnsResolver for SystemDnsResolver {
    fn resolve(&self, addr: Multiaddr) -> BoxFuture<'static, Result<SocketAddr, DnsResolverError>> {
        let protos = match common::extract_protocols(&addr) {
            Ok(p) => p,
            Err(err) => return boxed_ready(Err(err)),
        };

        match protos {
            (Protocol::Dns4(domain), Protocol::Tcp(port)) => dns_lookup(format!("{}:{}", domain, port)).boxed(),
            (Protocol::Ip4(host), Protocol::Tcp(port)) => boxed_ready(Ok((host, port).into())),
            (Protocol::Ip6(host), Protocol::Tcp(port)) => boxed_ready(Ok((host, port).into())),
            _ => boxed_ready(Err(DnsResolverError::UnsupportedAddress(addr))),
        }
    }
}

/// Performs an non-blocking DNS lookup of the given address
async fn dns_lookup<T>(addr: T) -> Result<SocketAddr, DnsResolverError>
where T: ToSocketAddrs + Display + Send + Sync + 'static {
    spawn_blocking(move || {
        debug!(target: LOG_TARGET, "Resolving address `{}` using system resolver", addr);
        addr.to_socket_addrs()
            .map_err(|err| DnsResolverError::NameResolutionFailed {
                source: err,
                address_str: addr.to_string(),
            })?
            .next()
            .ok_or_else(|| DnsResolverError::DnsAddressNotFound)
    })
    .await?
}

#[inline]
fn boxed_ready<T: Send + 'static>(t: T) -> BoxFuture<'static, T> {
    Box::pin(future::ready(t))
}
