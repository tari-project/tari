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
    tor::{
        control_client::commands::{AddOnionFlag, AddOnionResponse},
        Authentication,
        HiddenService,
        PortMapping,
        TorClientError,
        TorControlPortClient,
        TorIdentity,
    },
    utils::multiaddr::socketaddr_to_multiaddr,
};
use bitflags::bitflags;
use derive_error::Error;
use futures::{AsyncRead, AsyncWrite};
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
    /// The given tor service id is not a valid detached service id
    InvalidDetachedServiceId,
}

bitflags! {
    /// Hidden service flags
    #[derive(Default)]
    pub struct HsFlags: u32 {
        const NONE = 0x0;
        /// Detach the service from the control server connection. This keeps the hidden service active even if comms is shutdown.
        const DETACH = 0x1;
    }
}

/// Builder for Tor Hidden Services
#[derive(Default)]
pub struct HiddenServiceBuilder {
    identity: Option<TorIdentity>,
    port_mapping: Option<PortMapping>,
    control_server_addr: Option<Multiaddr>,
    control_server_auth: Authentication,
    socks_auth: socks::Authentication,
    hs_flags: HsFlags,
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

    /// The identity of the hidden service. When set, this key is used to enable routing from the Tor network to
    /// this address. If this is not set, a new service will be requested from the Tor Control Port.
    setter!(with_tor_identity, identity, Option<TorIdentity>);

    /// Configuration flags for the hidden service
    setter!(with_hs_flags, hs_flags, HsFlags);

    /// Set the PortMapping to use when creating this hidden service. A PortMapping maps a Tor port to a proxied address
    /// (usually local). An error will result if this is not provided.
    pub fn with_port_mapping<P: Into<PortMapping>>(mut self, port_mapping: P) -> Self {
        self.port_mapping = Some(port_mapping.into());
        self
    }
}

impl HiddenServiceBuilder {
    /// Create a HiddenService with the given builder parameters.
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
        client.authenticate(&self.control_server_auth).await?;

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
        let add_onion_resp = match self.identity {
            Some(identity) => {
                onion_private_key = identity.private_key.clone();
                Self::ensure_onion(&mut client, identity, proxied_port_mapping, self.hs_flags).await?
            },
            None => {
                let resp = client.add_onion(vec![], proxied_port_mapping, None).await?;
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
            service_id: add_onion_resp.service_id,
            onion_port: add_onion_resp.onion_port,
            private_key: onion_private_key,
            proxied_addr,
            client,
        })
    }

    async fn ensure_onion<TSocket>(
        client: &mut TorControlPortClient<TSocket>,
        identity: TorIdentity,
        port_mapping: PortMapping,
        hs_flags: HsFlags,
    ) -> Result<AddOnionResponse, HiddenServiceBuilderError>
    where
        TSocket: AsyncRead + AsyncWrite + Unpin,
    {
        let mut flags = Vec::new();
        if hs_flags.contains(HsFlags::DETACH) {
            flags.push(AddOnionFlag::Detach);
        }

        let result = client
            .add_onion_from_private_key(&identity.private_key, flags, port_mapping.clone(), None)
            .await;

        match result {
            Ok(resp) => Ok(resp),
            Err(TorClientError::OnionAddressCollision) => {
                debug!(target: LOG_TARGET, "Onion address is already registered.");

                let detached_str = client.get_info("onions/detached").await?;
                debug!(
                    target: LOG_TARGET,
                    "Comparing active detached service IDs '{}' to expected service id '{}'",
                    detached_str.replace('\n', ", "),
                    identity.service_id
                );
                let mut detached = detached_str.split('\n');

                if detached.all(|svc_id| svc_id != identity.service_id) {
                    return Err(HiddenServiceBuilderError::InvalidDetachedServiceId);
                }

                Ok(AddOnionResponse {
                    // TODO(sdbondi): This could be a different ORPort than the one requested in port mapping, I was not
                    //                able to find a way to find the port mapping for the service.
                    //                Setting the onion_port to be the same as the original port may cause
                    //                confusion/break "just works"(tm)
                    onion_port: identity.onion_port,
                    service_id: identity.service_id,
                    private_key: Some(identity.private_key),
                })
            },
            Err(err) => Err(err.into()),
        }
    }
}
