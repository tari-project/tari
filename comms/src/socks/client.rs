// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

// Acknowledgement to @sticnarf for tokio-socks on which this code is based
use super::error::SocksError;
use data_encoding::BASE32;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use multiaddr::{Multiaddr, Protocol};
use std::{
    borrow::Cow,
    net::{Ipv4Addr, Ipv6Addr},
};

pub type Result<T> = std::result::Result<T, SocksError>;

/// Authentication methods
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authentication {
    None,
    Password(String, String),
}

impl Authentication {
    fn id(&self) -> u8 {
        match self {
            Authentication::Password(_, _) => 0x02,
            Authentication::None => 0x00,
        }
    }
}

impl Default for Authentication {
    fn default() -> Self {
        Authentication::None
    }
}

#[repr(u8)]
#[derive(Clone, Debug, Copy)]
enum Command {
    Connect = 0x01,
    Bind = 0x02,
    // UDP Associate command not supported
    // Associate = 0x03,
    TorResolve = 0xF0,
    TorResolvePtr = 0xF1,
}

/// A SOCKS5 socket connection.
pub struct Socks5Client<TSocket> {
    protocol: SocksProtocol<TSocket>,
    is_authenticated: bool,
}

impl<TSocket> Socks5Client<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    /// Create a new socks5 client with a socket already connected to the target proxy
    pub fn new(socket: TSocket) -> Self {
        Self {
            protocol: SocksProtocol::new(socket),
            is_authenticated: false,
        }
    }

    pub fn with_authentication(&mut self, auth: Authentication) -> Result<&mut Self> {
        Self::validate_auth(&auth)?;
        self.protocol.set_authentication(auth);
        Ok(self)
    }

    /// Connects to a address through a SOCKS5 proxy and returns the 'upgraded' socket. This consumes the
    /// `Socks5Client` as once connected, the socks protocol does not recognise any further commands.
    pub async fn connect(mut self, address: &Multiaddr) -> Result<(TSocket, Multiaddr)> {
        let address = self.execute_command(Command::Connect, address).await?;
        Ok((self.protocol.socket, address))
    }

    /// Requests the tor proxy to resolve a DNS address is resolved into an IP address.
    /// This operation only works with the tor SOCKS proxy.
    pub async fn tor_resolve(&mut self, address: &Multiaddr) -> Result<Multiaddr> {
        // Tor resolve does not return the port back
        let (dns, rest) = multiaddr_split_first(&address);
        let mut resolved = self.execute_command(Command::TorResolve, &dns.into()).await?;
        resolved.pop();
        for r in rest {
            resolved.push(r);
        }
        Ok(resolved)
    }

    /// Requests the tor proxy to reverse resolve an IP address into a DNS address if it is able.
    /// This operation only works with the tor SOCKS proxy.
    pub async fn tor_resolve_ptr(&mut self, address: &Multiaddr) -> Result<Multiaddr> {
        self.execute_command(Command::TorResolvePtr, address).await
    }

    async fn execute_command(&mut self, command: Command, address: &Multiaddr) -> Result<Multiaddr> {
        if !self.is_authenticated {
            self.protocol.authenticate().await?;
            self.is_authenticated = true;
        }

        let address = self.protocol.send_command(command, address).await?;

        Ok(address)
    }

    fn validate_auth(auth: &Authentication) -> Result<()> {
        match auth {
            Authentication::None => {},
            Authentication::Password(username, password) => {
                let username_len = username.as_bytes().len();
                if username_len < 1 || username_len > 255 {
                    return Err(SocksError::InvalidAuthValues(
                        "username length should between 1 to 255".to_string(),
                    ));
                }
                let password_len = password.as_bytes().len();
                if password_len < 1 || password_len > 255 {
                    return Err(SocksError::InvalidAuthValues(
                        "password length should between 1 to 255".to_string(),
                    ));
                }
            },
        }
        Ok(())
    }
}

/// Split the first Protocol from the rest of the address
fn multiaddr_split_first(addr: &Multiaddr) -> (Protocol<'_>, Vec<Protocol<'_>>) {
    let mut iter = addr.iter();
    let proto = iter
        .next()
        .expect("prepare_multiaddr_for_tor_resolve: received empty `Multiaddr`");
    let rest = iter.collect();
    (proto, rest)
}

const SOCKS_BUFFER_LENGTH: usize = 513;

struct SocksProtocol<TSocket> {
    socket: TSocket,
    authentication: Authentication,
    buf: Box<[u8; SOCKS_BUFFER_LENGTH]>,
    ptr: usize,
    len: usize,
}

impl<TSocket> SocksProtocol<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    fn new(socket: TSocket) -> Self {
        SocksProtocol {
            socket,
            authentication: Default::default(),
            buf: Box::new([0; 513]),
            ptr: 0,
            len: 0,
        }
    }

    pub async fn authenticate(&mut self) -> Result<()> {
        // Write request to connect/authenticate
        self.prepare_send_auth_method_selection();
        self.write().await?;

        // Receive authentication method
        self.prepare_recv_auth_method_selection();
        self.read().await?;
        if self.buf[0] != 0x05 {
            return Err(SocksError::InvalidResponseVersion);
        }
        match self.buf[1] {
            0x00 => {
                // No auth
            },
            0x02 => {
                self.password_authentication_protocol().await?;
            },
            0xff => {
                return Err(SocksError::NoAcceptableAuthMethods);
            },
            m if m != self.authentication.id() => return Err(SocksError::UnknownAuthMethod),
            _ => unimplemented!(),
        }

        Ok(())
    }

    pub fn set_authentication(&mut self, authentication: Authentication) {
        self.authentication = authentication;
    }

    pub async fn send_command(&mut self, command: Command, address: &Multiaddr) -> Result<Multiaddr> {
        self.prepare_send_request(command, address)?;
        self.write().await?;
        self.receive_reply().await
    }

    async fn password_authentication_protocol(&mut self) -> Result<()> {
        self.prepare_send_password_auth();
        self.write().await?;

        self.prepare_recv_password_auth();
        self.read().await?;

        if self.buf[0] != 0x01 {
            return Err(SocksError::InvalidResponseVersion);
        }
        if self.buf[1] != 0x00 {
            return Err(SocksError::PasswordAuthFailure(self.buf[1]));
        }

        Ok(())
    }

    async fn receive_reply(&mut self) -> Result<Multiaddr> {
        self.prepare_recv_reply();
        self.ptr += self.read().await?;
        if self.buf[0] != 0x05 {
            return Err(SocksError::InvalidResponseVersion);
        }
        if self.buf[2] != 0x00 {
            return Err(SocksError::InvalidReservedByte);
        }

        let auth_byte = self.buf[1];
        if auth_byte != 0x00 {
            return match self.buf[1] {
                0x00 => unreachable!(),
                0x01 => Err(SocksError::GeneralSocksServerFailure),
                0x02 => Err(SocksError::ConnectionNotAllowedByRuleset),
                0x03 => Err(SocksError::NetworkUnreachable),
                0x04 => Err(SocksError::HostUnreachable),
                0x05 => Err(SocksError::ConnectionRefused),
                0x06 => Err(SocksError::TtlExpired),
                0x07 => Err(SocksError::CommandNotSupported),
                0x08 => Err(SocksError::AddressTypeNotSupported),
                _ => Err(SocksError::UnknownAuthMethod),
            };
        }

        match self.buf[3] {
            // IPv4
            0x01 => {
                self.len = 10;
            },
            // IPv6
            0x04 => {
                self.len = 22;
            },
            // Domain
            0x03 => {
                self.len = 5;
                self.ptr += self.read().await?;
                self.len += self.buf[4] as usize + 2;
            },
            _ => return Err(SocksError::UnknownAddressType),
        }

        self.ptr += self.read().await?;
        let address = match self.buf[3] {
            // IPv4
            0x01 => {
                let mut ip = [0; 4];
                ip[..].copy_from_slice(&self.buf[4..8]);
                let ip = Ipv4Addr::from(ip);
                let port = u16::from_be_bytes([self.buf[8], self.buf[9]]);
                let mut addr: Multiaddr = Protocol::Ip4(ip).into();
                addr.push(Protocol::Tcp(port));
                addr
            },
            // IPv6
            0x04 => {
                let mut ip = [0; 16];
                ip[..].copy_from_slice(&self.buf[4..20]);
                let ip = Ipv6Addr::from(ip);
                let port = u16::from_be_bytes([self.buf[20], self.buf[21]]);
                let mut addr: Multiaddr = Protocol::Ip6(ip).into();
                addr.push(Protocol::Tcp(port));
                addr
            },
            // Domain
            0x03 => {
                let domain_bytes = (&self.buf[5..(self.len - 2)]).to_vec();
                let domain = String::from_utf8(domain_bytes)
                    .map_err(|_| SocksError::InvalidTargetAddress("domain bytes are not a valid UTF-8 string"))?;
                let mut addr: Multiaddr = Protocol::Dns4(Cow::Owned(domain)).into();
                let port = u16::from_be_bytes([self.buf[self.len - 2], self.buf[self.len - 1]]);
                addr.push(Protocol::Tcp(port));
                addr
            },
            _ => unreachable!(),
        };

        Ok(address)
    }

    fn prepare_send_auth_method_selection(&mut self) {
        self.ptr = 0;
        self.buf[0] = 0x05;
        match self.authentication {
            Authentication::None => {
                self.buf[1..3].copy_from_slice(&[1, 0x00]);
                self.len = 3;
            },
            Authentication::Password { .. } => {
                self.buf[1..4].copy_from_slice(&[2, 0x00, 0x02]);
                self.len = 4;
            },
        }
    }

    fn prepare_recv_auth_method_selection(&mut self) {
        self.ptr = 0;
        self.len = 2;
    }

    fn prepare_send_password_auth(&mut self) {
        match &self.authentication {
            Authentication::Password(username, password) => {
                self.ptr = 0;
                self.buf[0] = 0x01;
                let username_bytes = username.as_bytes();
                let username_len = username_bytes.len();
                self.buf[1] = username_len as u8;
                self.buf[2..(2 + username_len)].copy_from_slice(username_bytes);
                let password_bytes = password.as_bytes();
                let password_len = password_bytes.len();
                self.len = 3 + username_len + password_len;
                self.buf[(2 + username_len)] = password_len as u8;
                self.buf[(3 + username_len)..self.len].copy_from_slice(password_bytes);
            },
            Authentication::None => unreachable!(),
        }
    }

    fn prepare_recv_password_auth(&mut self) {
        self.ptr = 0;
        self.len = 2;
    }

    fn prepare_send_request(&mut self, command: Command, address: &Multiaddr) -> Result<()> {
        self.ptr = 0;
        self.buf[..3].copy_from_slice(&[0x05, command as u8, 0x00]);
        let mut addr_iter = address.iter();
        let part1 = addr_iter
            .next()
            .ok_or_else(|| SocksError::InvalidTargetAddress("Address contained no components"))?;

        let part2 = addr_iter.next();

        match (part1, part2) {
            (Protocol::Ip4(ip), Some(Protocol::Tcp(port))) => {
                self.buf[3] = 0x01;
                self.buf[4..8].copy_from_slice(&ip.octets());
                self.buf[8..10].copy_from_slice(&port.to_be_bytes());
                self.len = 10;
            },
            (Protocol::Ip6(ip), Some(Protocol::Tcp(port))) => {
                self.buf[3] = 0x04;
                self.buf[4..20].copy_from_slice(&ip.octets());
                self.buf[20..22].copy_from_slice(&port.to_be_bytes());
                self.len = 22;
            },
            (Protocol::Dns4(domain), Some(Protocol::Tcp(port))) => {
                self.buf[3] = 0x03;
                let domain = domain.as_bytes();
                let len = domain.len();
                self.buf[4] = len as u8;
                self.buf[5..5 + len].copy_from_slice(domain);
                self.buf[(5 + len)..(7 + len)].copy_from_slice(&port.to_be_bytes());
                self.len = 7 + len;
            },
            // Special case for Tor resolve
            (Protocol::Dns4(domain), None) => {
                self.buf[3] = 0x03;
                let domain = domain.as_bytes();
                let len = domain.len();
                self.buf[4] = len as u8;
                self.buf[5..5 + len].copy_from_slice(domain);
                // Zero port
                self.buf[5 + len] = 0;
                self.buf[6 + len] = 0;
                self.len = 7 + len;
            },
            (p @ Protocol::Onion(_, _), None) => {
                self.buf[3] = 0x03;
                let (domain, port) = Self::extract_onion_address(p)?;
                let len = domain.len();
                self.buf[4] = len as u8;
                self.buf[5..5 + len].copy_from_slice(domain.as_bytes());
                self.buf[(5 + len)..(7 + len)].copy_from_slice(&port.to_be_bytes());
                self.len = 7 + len;
            },
            (Protocol::Onion3(addr), None) => {
                self.buf[3] = 0x03;
                let port = addr.port();
                let domain = format!("{}.onion", BASE32.encode(addr.hash()));
                let len = domain.len();
                self.buf[4] = len as u8;
                self.buf[5..5 + len].copy_from_slice(domain.as_bytes());
                self.buf[(5 + len)..(7 + len)].copy_from_slice(&port.to_be_bytes());
                self.len = 7 + len;
            },
            _ => return Err(SocksError::AddressTypeNotSupported),
        }
        Ok(())
    }

    fn extract_onion_address(p: Protocol<'_>) -> Result<(String, u16)> {
        let onion_addr = p.to_string();
        let mut parts = onion_addr.split('/').nth(2).expect("already checked").split(':');
        let domain = format!("{}.onion", parts.next().expect("already checked"),);
        let port = parts
            .next()
            .expect("already checked")
            .parse::<u16>()
            .map_err(|_| SocksError::InvalidTargetAddress("Invalid onion address port"))?;
        Ok((domain, port))
    }

    fn prepare_recv_reply(&mut self) {
        self.ptr = 0;
        self.len = 4;
    }

    async fn write(&mut self) -> Result<()> {
        self.socket
            .write_all(&self.buf[self.ptr..self.len])
            .await
            .map_err(Into::into)
    }

    async fn read(&mut self) -> Result<usize> {
        self.socket.read_exact(&mut self.buf[self.ptr..self.len]).await?;
        Ok(self.len - self.ptr)
    }
}
