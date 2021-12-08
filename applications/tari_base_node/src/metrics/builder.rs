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
use std::{
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use reqwest::{IntoUrl, Url};
use tari_shutdown::ShutdownSignal;
use tokio::task;

use super::{push, server};

#[derive(Debug, Clone)]
pub struct MetricsBuilder {
    listener_addr: Option<SocketAddr>,
    push_endpoint: Option<Url>,
    push_interval: Duration,
}

impl MetricsBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_scrape_server<T: ToSocketAddrs>(mut self, address: T) -> Self {
        let listener_addr = address
            .to_socket_addrs()
            .expect("bad metric server bind address")
            .next()
            .unwrap();
        self.listener_addr = Some(listener_addr);
        self
    }

    pub fn with_push_gateway<T: IntoUrl>(mut self, endpoint: T) -> Self {
        let endpoint = endpoint.into_url().unwrap();
        self.push_endpoint = Some(endpoint);
        self
    }

    pub fn spawn_services(self, shutdown: ShutdownSignal) {
        let registry = tari_metrics::get_default_registry();

        if let Some(listener_addr) = self.listener_addr {
            task::spawn(
                shutdown
                    .clone()
                    .select(Box::pin(server::start(listener_addr, registry.clone()))),
            );
        }

        if let Some(endpoint) = self.push_endpoint {
            task::spawn(shutdown.select(Box::pin(push::start(endpoint, self.push_interval, registry))));
        }
    }
}

impl Default for MetricsBuilder {
    fn default() -> Self {
        Self {
            listener_addr: None,
            push_endpoint: None,
            push_interval: Duration::from_secs(10),
        }
    }
}
