// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
};

use tari_network::multiaddr::{Multiaddr, Protocol};

/// Convert a multiaddr to a socket address required for `TcpStream`
/// This function resolves DNS4 addresses to an ip address.
pub fn multiaddr_to_socketaddr(addr: &Multiaddr) -> io::Result<SocketAddr> {
    let mut addr_iter = addr.iter();
    let network_proto = addr_iter
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid address '{}'", addr)))?;
    let transport_proto = addr_iter
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid address '{}'", addr)))?;

    if addr_iter.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{}'", addr),
        ));
    }

    match (network_proto, transport_proto) {
        (Protocol::Dns4(domain), Protocol::Tcp(port)) => {
            let addr = format!("{}:{}", domain, port);
            addr.to_socket_addrs()
                .map_err(|_e| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid domain '{}'", domain)))?
                .next()
                .map_or_else(
                    || {
                        Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Invalid domain '{}'", domain),
                        ))
                    },
                    Ok,
                )
        },
        (Protocol::Ip4(host), Protocol::Tcp(port)) => Ok((host, port).into()),
        (Protocol::Ip6(host), Protocol::Tcp(port)) => Ok((host, port).into()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{}'", addr),
        )),
    }
}
