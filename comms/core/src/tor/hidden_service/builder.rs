// Copyright 2020, The Taiji Project
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

use std::sync::Arc;

use bitflags::bitflags;
use log::*;
use taiji_shutdown::{OptionalShutdownSignal, ShutdownSignal};
use thiserror::Error;

use super::controller::HiddenServiceControllerError;
use crate::{
    multiaddr::Multiaddr,
    socks,
    tor::{
        hidden_service::{controller::HiddenServiceController, TorProxyOpts},
        Authentication,
        PortMapping,
        TorIdentity,
    },
};

const LOG_TARGET: &str = "comms::tor::hidden_service";

#[derive(Debug, Error)]
pub enum HiddenServiceBuilderError {
    #[error("The proxied port mapping was not provided. Use `with_proxied_port_mapping` to set it.")]
    ProxiedPortMappingNotProvided,
    #[error("The control server address was not provided. Use `with_control_server_address` to set it.")]
    TorControlServerAddressNotProvided,
    #[error("HiddenServiceControllerError: {0}")]
    HiddenServiceControllerError(#[from] HiddenServiceControllerError),
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
    socks_addr_override: Option<Multiaddr>,
    control_server_addr: Option<Multiaddr>,
    proxy_opts: TorProxyOpts,
    control_server_auth: Authentication,
    socks_auth: socks::Authentication,
    hs_flags: HsFlags,
    shutdown_signal: OptionalShutdownSignal,
}

impl HiddenServiceBuilder {
    pub fn new() -> Self {
        Default::default()
    }
}

impl HiddenServiceBuilder {
    setter!(
        /// The address of the Tor Control Port. An error will result if this is not provided.
        with_control_server_address,
        control_server_addr,
        Option<Multiaddr>
    );

    setter!(
        /// Configure the underlying SOCKS transport to bypass the proxy and connect directly to these addresses
        with_bypass_proxy_addresses,
        proxy_opts.bypass_addresses,
        Arc<Vec<Multiaddr>>
    );

    setter!(
        /// Authentication settings for the Tor Control Port.
        with_control_server_auth,
        control_server_auth,
        Authentication
    );

    setter!(
        /// Authentication to use for the SOCKS5 proxy.
        with_socks_authentication,
        socks_auth,
        socks::Authentication
    );

    setter!(
        /// The identity of the hidden service. When set, this key is used to enable routing from the Tor network to
        /// this address. If this is not set, a new service will be requested from the Tor Control Port.
        with_tor_identity,
        identity,
        Option<TorIdentity>
    );

    setter!(
        /// Configuration flags for the hidden service
        with_hs_flags,
        hs_flags,
        HsFlags
    );

    /// Use a direct TCP/IP connection if a TCP address is given instead of the tor proxy. This is worse for privacy
    /// but can use the full available connection bandwidth
    pub fn bypass_tor_for_tcp_addresses(mut self) -> Self {
        self.proxy_opts.bypass_for_tcpip = true;
        self
    }

    /// The address of the SOCKS5 server. If an address is None, the hidden service builder will use the SOCKS
    /// listener address as given by the tor control port.
    pub fn with_shutdown_signal(mut self, shutdown_signal: ShutdownSignal) -> Self {
        self.shutdown_signal.set(shutdown_signal);
        self
    }

    /// The address of the SOCKS5 server. If an address is None, the hidden service builder will use the SOCKS
    /// listener address as given by the tor control port.
    pub fn with_socks_address_override(mut self, socks_addr_override: Option<Multiaddr>) -> Self {
        self.socks_addr_override = socks_addr_override;
        self
    }

    /// Set the PortMapping to use when creating this hidden service. A PortMapping maps a Tor port to a proxied address
    /// (usually local). An error will result if this is not provided.
    pub fn with_port_mapping<P: Into<PortMapping>>(mut self, port_mapping: P) -> Self {
        self.port_mapping = Some(port_mapping.into());
        self
    }
}

impl HiddenServiceBuilder {
    /// Create a HiddenService with the given builder parameters.
    pub fn build(self) -> Result<HiddenServiceController, HiddenServiceBuilderError> {
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

        let controller = HiddenServiceController::new(
            control_server_addr,
            self.control_server_auth,
            proxied_port_mapping,
            self.socks_addr_override,
            self.socks_auth,
            self.identity,
            self.hs_flags,
            self.proxy_opts,
            self.shutdown_signal,
        );

        Ok(controller)
    }
}
