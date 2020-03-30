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

mod builder;
pub use builder::{HiddenServiceBuilder, HiddenServiceBuilderError, HsFlags};

mod controller;

use crate::{
    multiaddr::Multiaddr,
    socks,
    tor::{PrivateKey, TorClientError},
    transports::{SocksConfig, SocksTransport},
};
use serde_derive::{Deserialize, Serialize};
use tari_shutdown::Shutdown;

/// Handle for a Tor Hidden Service. This handle keeps the session to the Tor control port alive.
/// Once this is dropped, the hidden service will cease to be accessible.
pub struct HiddenService {
    /// The identity of the hidden service
    pub(super) identity: TorIdentity,
    /// The SOCKS5 address obtained by querying the Tor control port and used to configure the `SocksTransport`.
    pub(super) socks_addr: Multiaddr,
    /// SOCKS5 authentication details used to configure the `SocksTransport`.
    pub(super) socks_auth: socks::Authentication,
    /// The address where incoming traffic to the `onion_addr` will be forwarded to.
    pub(super) proxied_addr: Multiaddr,
    /// Shutdown signal for hidden service
    pub(super) shutdown: Shutdown,
}

impl HiddenService {
    pub fn get_onion_address(&self) -> Multiaddr {
        // service_id should always come from the tor control server, so the length can be relied on
        multiaddr_from_service_id_and_port(self.service_id(), self.identity.onion_port)
            .expect("failed to create onion address from HiddenService service_id and onion_port")
    }

    pub fn service_id(&self) -> &str {
        &self.identity.service_id
    }

    pub fn proxied_address(&self) -> &Multiaddr {
        &self.proxied_addr
    }

    pub fn get_transport(&self) -> SocksTransport {
        SocksTransport::new(SocksConfig {
            proxy_address: self.socks_addr.clone(),
            authentication: self.socks_auth.clone(),
        })
    }

    pub fn tor_identity(&self) -> &TorIdentity {
        &self.identity
    }
}

fn multiaddr_from_service_id_and_port(service_id: &str, onion_port: u16) -> Result<Multiaddr, TorClientError> {
    const ONION_V2_LEN: usize = 16;
    const ONION_V3_LEN: usize = 56;
    match service_id.len() {
        ONION_V2_LEN => format!("/onion/{}:{}", service_id, onion_port)
            .parse()
            .map_err(|_| TorClientError::InvalidServiceId),
        ONION_V3_LEN => {
            // This will fail until this PR is released (https://github.com/libp2p/rust-libp2p/pull/1354)
            format!("/onion3/{}:{}", service_id, onion_port)
                .parse()
                .map_err(|_| TorClientError::InvalidServiceId)
        },
        _ => Err(TorClientError::InvalidServiceId),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TorIdentity {
    pub private_key: PrivateKey,
    pub service_id: String,
    pub onion_port: u16,
}

impl TorIdentity {
    pub fn try_get_onion_address(&self) -> Result<Multiaddr, TorClientError> {
        multiaddr_from_service_id_and_port(&self.service_id, self.onion_port)
    }
}
