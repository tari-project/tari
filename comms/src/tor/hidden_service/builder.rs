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
    tor::{Authentication, HiddenService, PortMapping, PrivateKey, TorClientError, TorControlPortClient},
    utils::multiaddr::socketaddr_to_multiaddr,
};
use derive_error::Error;
use log::*;
use std::net::SocketAddr;

const LOG_TARGET: &str = "comms::tor::hidden_service";

#[derive(Debug, Error)]
pub enum HiddenServiceBuilderError {
    /// Failed to parse SOCKS address returned by control port
    FailedToParseSocksAddress,
    /// The proxied port mapping was not provided. Use `with_proxied_port_mapping` to set it.
    ProxiedPortMappingNotProvided,
    /// The control server address was not provided. Use `with_control_server_address` to set it.
    TorControlServerAddressNotProvided,
    TorClientError(TorClientError),
}

/// Builder for Tor Hidden Services
#[derive(Default)]
pub struct HiddenServiceBuilder {
    onion_private_key: Option<PrivateKey>,
    port_mapping: Option<PortMapping>,
    control_server_addr: Option<Multiaddr>,
    control_server_auth: Authentication,
    socks_auth: socks::Authentication,
}

impl HiddenServiceBuilder {
    pub fn new() -> Self {
        Default::default()
    }
}

impl HiddenServiceBuilder {
    /// The address of the Tor Control Port. An error will result if this is not provided.
    setter!(with_control_server_address, control_server_addr, Option<Multiaddr>);

    /// Authentication settings for the Tor Control Port.
    setter!(with_control_server_auth, control_server_auth, Authentication);

    /// Authentication to use for the SOCKS5 proxy.
    setter!(with_socks_authentication, socks_auth, socks::Authentication);

    /// The `PrivateKey` of the hidden service. When set, this key is used to enable routing from the Tor network to
    /// this address. If this is not set, a new private key will be requested from the Tor Control Port.
    setter!(with_onion_private_key, onion_private_key, Option<PrivateKey>);

    /// Set the PortMapping to use when creating this hidden service. A PortMapping maps a Tor port to a proxied address
    /// (usually local). An error will result if this is not provided.
    pub fn with_port_mapping<P: Into<PortMapping>>(mut self, port_mapping: P) -> Self {
        self.port_mapping = Some(port_mapping.into());
        self
    }
}

impl HiddenServiceBuilder {
    /// Create a HiddenService witht he given builder parameters.
    pub async fn finish(self) -> Result<HiddenService, HiddenServiceBuilderError> {
        let proxied_port_mapping = self
            .port_mapping
            .ok_or(HiddenServiceBuilderError::ProxiedPortMappingNotProvided)?;
        let control_server_addr = self
            .control_server_addr
            .ok_or(HiddenServiceBuilderError::TorControlServerAddressNotProvided)?;

        debug!(
            target: LOG_TARGET,
            "Building tor hidden service with control port '{}' and port mapping '{}'",
            control_server_addr,
            proxied_port_mapping
        );

        let mut client = TorControlPortClient::connect(control_server_addr).await?;
        client.authenticate(self.control_server_auth).await?;

        // Get configured SOCK5 address from Tor
        let socks_addr = client
            .get_info("net/listeners/socks")
            .await?
            .parse::<SocketAddr>()
            .map(|addr| socketaddr_to_multiaddr(&addr))
            .map_err(|_| HiddenServiceBuilderError::FailedToParseSocksAddress)?;

        let proxied_addr = socketaddr_to_multiaddr(proxied_port_mapping.proxied_address());

        // Initialize a onion hidden service - either from the given private key or by creating a new one
        let onion_private_key;
        let add_onion_resp = match self.onion_private_key {
            Some(private_key) => {
                onion_private_key = private_key;
                client
                    .add_onion_from_private_key(&onion_private_key, vec![], proxied_port_mapping, None)
                    .await?
            },
            // TODO: Once multiaddr supports onion3 addresses, change this to create v3 hidden services
            None => {
                let resp = client.add_onion_v2(vec![], proxied_port_mapping, None).await?;
                onion_private_key = resp
                    .private_key
                    .clone()
                    .expect("Tor server MUST return private key according to spec");
                resp
            },
        };

        debug!(
            target: LOG_TARGET,
            "Added hidden service with service id '{}' on port '{}'",
            add_onion_resp.service_id,
            add_onion_resp.onion_port
        );

        Ok(HiddenService {
            socks_addr,
            socks_auth: self.socks_auth,
            service_id: add_onion_resp.service_id.into_owned(),
            onion_port: add_onion_resp.onion_port,
            private_key: onion_private_key,
            proxied_addr,
            client,
        })
    }
}
