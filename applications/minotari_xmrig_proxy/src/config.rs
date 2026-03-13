// Copyright 2025. The Tari Project
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

use std::path::{Path, PathBuf};

use minotari_wallet_grpc_client::GrpcAuthentication;
use serde::{Deserialize, Serialize};
use tari_common::{SubConfigPath, configuration::Network};
use tari_common_types::tari_address::TariAddress;
use tari_comms::multiaddr::Multiaddr;
use tari_transaction_components::transaction_components::RangeProofType;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct XmrigProxyConfig {
    override_from: Option<String>,
    /// The Minotari base node's gRPC address
    pub base_node_grpc_address: Option<String>,
    /// gRPC authentication for base node
    pub base_node_grpc_authentication: GrpcAuthentication,
    /// gRPC domain name for node TLS validation
    pub base_node_grpc_tls_domain_name: Option<String>,
    /// gRPC CA cert name for TLS
    pub base_node_grpc_ca_cert_filename: String,
    /// Address on which the proxy listens for XMRig connections
    pub listener_address: Multiaddr,
    /// The Tari wallet address where mining rewards will be sent
    pub wallet_payment_address: String,
    /// Range proof type for coinbase outputs (default = revealed_value)
    pub range_proof_type: RangeProofType,
    /// Extra data to store in the coinbase (e.g. pool info, limited to a few bytes)
    pub coinbase_extra: String,
    /// The proxy waits for the base node to complete initial sync before accepting work requests
    pub wait_for_initial_sync_at_startup: bool,
    /// Selected network
    pub network: Network,
    /// The relative path to store persistent config
    pub config_dir: PathBuf,
}

impl Default for XmrigProxyConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            base_node_grpc_address: None,
            base_node_grpc_authentication: GrpcAuthentication::default(),
            base_node_grpc_tls_domain_name: None,
            base_node_grpc_ca_cert_filename: "node_ca.pem".to_string(),
            listener_address: "/ip4/127.0.0.1/tcp/18085".parse().unwrap(),
            wallet_payment_address: TariAddress::default().to_base58(),
            range_proof_type: RangeProofType::RevealedValue,
            coinbase_extra: "tari_xmrig_proxy".to_string(),
            wait_for_initial_sync_at_startup: true,
            network: Default::default(),
            config_dir: PathBuf::from("config/xmrig_proxy"),
        }
    }
}

impl XmrigProxyConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.config_dir.is_absolute() {
            self.config_dir = base_path.as_ref().join(self.config_dir.as_path());
        }
    }
}

impl SubConfigPath for XmrigProxyConfig {
    fn main_key_prefix() -> &'static str {
        "xmrig_proxy"
    }
}
