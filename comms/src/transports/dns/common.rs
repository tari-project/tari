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

use super::error::DnsResolverError;
use crate::multiaddr::{Multiaddr, Protocol};
use std::net::SocketAddr;

pub fn is_dns4_addr(addr: &Multiaddr) -> bool {
    let proto = addr.iter().next();
    matches!(proto, Some(Protocol::Dns4(_)))
}

pub fn convert_tcpip_multiaddr_to_socketaddr(addr: &Multiaddr) -> Result<SocketAddr, DnsResolverError> {
    match extract_protocols(addr)? {
        (Protocol::Ip4(host), Protocol::Tcp(port)) => Ok((host, port).into()),
        (Protocol::Ip6(host), Protocol::Tcp(port)) => Ok((host, port).into()),
        _ => Err(DnsResolverError::ExpectedTcpIpAddress(addr.clone())),
    }
}

pub fn extract_protocols(addr: &Multiaddr) -> Result<(Protocol<'_>, Protocol<'_>), DnsResolverError> {
    let mut addr_iter = addr.iter();
    let proto1 = addr_iter.next().ok_or_else(|| DnsResolverError::EmptyAddress)?;
    let proto2 = addr_iter.next().ok_or_else(|| DnsResolverError::InvalidAddress {
        address: addr.clone(),
        message: "Address does not consist of at least 2 parts".into(),
    })?;

    Ok((proto1, proto2))
}
