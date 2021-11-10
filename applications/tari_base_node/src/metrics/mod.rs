//  Copyright 2021, The Tari Project
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

mod builder;
mod push;
mod server;

use builder::MetricsBuilder;
use std::collections::HashMap;
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig};
use tari_comms::NodeIdentity;
use tari_metrics::Registry;
use tari_shutdown::ShutdownSignal;

pub fn install(
    application: ApplicationType,
    identity: &NodeIdentity,
    config: &GlobalConfig,
    bootstrap: &ConfigBootstrap,
    shutdown: ShutdownSignal,
) {
    let metrics_registry = create_metrics_registry(application, identity);
    tari_metrics::set_default_registry(metrics_registry);

    let mut metrics = MetricsBuilder::new();
    if let Some(addr) = bootstrap
        .metrics_server_bind_addr
        .as_ref()
        .or(config.metrics.prometheus_scraper_bind_addr.as_ref())
    {
        metrics = metrics.with_scrape_server(addr);
    }
    if let Some(endpoint) = bootstrap
        .metrics_push_endpoint
        .as_ref()
        .or(config.metrics.prometheus_push_endpoint.as_ref())
    {
        // http://localhost:9091/metrics/job/base-node
        metrics = metrics.with_push_gateway(endpoint);
    }
    metrics.spawn_services(shutdown);
}

fn create_metrics_registry(application: ApplicationType, identity: &NodeIdentity) -> Registry {
    let mut labels = HashMap::with_capacity(3);
    labels.insert("app".to_string(), application.as_config_str().to_string());
    labels.insert("role".to_string(), identity.features().as_role_str().to_string());
    labels.insert("node_id".to_string(), identity.node_id().to_string());
    labels.insert("public_key".to_string(), identity.public_key().to_string());

    tari_metrics::Registry::new_custom(Some("tari".to_string()), Some(labels)).unwrap()
}
