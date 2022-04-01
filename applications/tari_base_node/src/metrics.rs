//  Copyright 2022, The Tari Project
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

use std::{collections::HashMap, net::SocketAddr};

use serde::{Deserialize, Serialize};
use tari_common::{configuration::bootstrap::ApplicationType, SubConfigPath};
use tari_comms::NodeIdentity;
use tari_metrics::{server::MetricsServerBuilder, Registry};
use tari_shutdown::ShutdownSignal;
use tokio::task;

pub fn install(
    application: ApplicationType,
    identity: &NodeIdentity,
    config: &MetricsConfig,
    shutdown: ShutdownSignal,
) {
    let metrics_registry = create_metrics_registry(application, identity);
    tari_metrics::set_default_registry(metrics_registry);

    let mut metrics = MetricsServerBuilder::new();

    if let Some(addr) = config.server_bind_address.as_ref() {
        metrics = metrics.with_scrape_server(addr);
    }

    if let Some(endpoint) = config.push_endpoint.as_ref() {
        // http://localhost:9091/metrics/job/base-node
        metrics = metrics.with_push_gateway(endpoint);
    }

    task::spawn(metrics.start(shutdown));
}

fn create_metrics_registry(application: ApplicationType, identity: &NodeIdentity) -> Registry {
    let mut labels = HashMap::with_capacity(4);
    labels.insert("app".to_string(), application.as_config_str().to_string());
    labels.insert("node_role".to_string(), identity.features().as_role_str().to_string());
    labels.insert("node_id".to_string(), identity.node_id().to_string());
    labels.insert("node_public_key".to_string(), identity.public_key().to_string());
    Registry::new_custom(Some("tari".to_string()), Some(labels)).unwrap()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    override_from: Option<String>,
    pub server_bind_address: Option<SocketAddr>,
    pub push_endpoint: Option<String>,
}

impl SubConfigPath for MetricsConfig {
    fn main_key_prefix() -> &'static str {
        "metrics"
    }
}
