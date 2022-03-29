//  Copyright 2022. The Tari Project
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
use std::{num::NonZeroU16, path::PathBuf};

use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::{SocksAuthentication, TorControlAuthentication};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields, rename_all = "snake_case")]
pub enum CommsTransport {
    None,
    /// Use TCP to join the Tari network. This transport can only communicate with TCP/IP addresses, so peers with
    /// e.g. tor onion addresses will not be contactable.
    Tcp {
        listener_address: Multiaddr,
        tor_socks_address: Option<Multiaddr>,
        tor_socks_auth: Option<SocksAuthentication>,
    },
    /// Configures the node to run over a tor hidden service using the Tor proxy. This transport recognises ip/tcp,
    /// onion v2, onion v3 and DNS addresses.
    #[serde(rename = "tor")]
    TorHiddenService {
        /// The address of the control server
        tor_control_address: Multiaddr,
        socks_address_override: Option<Multiaddr>,
        /// The address used to receive proxied traffic from the tor proxy to the Tari node. This port must be
        /// available
        tor_forward_address: Multiaddr,
        tor_control_auth: TorControlAuthentication,
        tor_onion_port: NonZeroU16,
        tor_proxy_bypass_addresses: Vec<Multiaddr>,
        tor_proxy_bypass_for_outbound_tcp: bool,
        tor_identity_file: Option<PathBuf>,
    },
    /// Use a SOCKS5 proxy transport. This transport recognises any addresses supported by the proxy.
    Socks5 {
        proxy_address: Multiaddr,
        auth: SocksAuthentication,
        listener_address: Multiaddr,
    },
}

impl Default for CommsTransport {
    fn default() -> Self {
        Self::None
    }
}
