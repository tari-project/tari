// Copyright 2020, The Tari Project
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

use crate::{
    multiaddr::Multiaddr,
    socks,
    tor::{PrivateKey, TorClientError, TorControlPortClient},
    transports::{SocksTransport, TcpTransport, Transport},
};

/// Handle for a Tor Hidden Service. This handle keeps the session to the Tor control port alive.
/// Once this is dropped, the hidden service will cease to be accessible.
pub struct HiddenService<'a> {
    /// The client connection to the Tor Control Port. `AddOnionFlag::Detach` is not used, so we have to keep the Tor
    /// Control port connection open.
    pub(super) client: TorControlPortClient<<TcpTransport as Transport>::Output>,
    /// The service id of the hidden service.
    pub(super) service_id: String,
    /// The SOCKS5 address obtained by querying the Tor control port and used to configure the `SocksTransport`.
    pub(super) socks_addr: Multiaddr,
    /// SOCKS5 authentication details used to configure the `SocksTransport`.
    pub(super) socks_auth: socks::Authentication,
    /// The Private Key for the hidden service.
    pub(super) private_key: PrivateKey<'a>,
    /// The onion port used.
    pub(super) onion_port: u16,
    /// The .onion address.
    pub(super) onion_addr: Multiaddr,
    /// The address where incoming traffic to the `onion_addr` will be forwarded to.
    pub(super) proxied_addr: Multiaddr,
}

impl<'a> HiddenService<'a> {
    /// Delete the hidden service. Once this is called the hidden service will no longer be accessible.
    pub async fn delete(&mut self) -> Result<(), TorClientError> {
        self.client.del_onion(&self.service_id).await.map_err(Into::into)
    }

    pub fn onion_address(&self) -> &Multiaddr {
        &self.onion_addr
    }

    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    pub fn private_key(&self) -> &PrivateKey<'a> {
        &self.private_key
    }

    pub fn proxied_address(&self) -> &Multiaddr {
        &self.proxied_addr
    }

    pub fn onion_port(&self) -> u16 {
        self.onion_port
    }

    pub fn get_transport(&self) -> SocksTransport {
        SocksTransport::new(self.socks_addr.clone(), self.socks_auth.clone())
    }
}
