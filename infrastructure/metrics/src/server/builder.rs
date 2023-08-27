//  Copyright 2021, The Taiji Project
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
    future::Future,
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use futures::{future, FutureExt};
use reqwest::{IntoUrl, Url};

use super::{pull, push};

#[derive(Debug, Clone)]
pub struct MetricsServerBuilder {
    listener_addr: Option<SocketAddr>,
    push_endpoint: Option<Url>,
    push_interval: Duration,
}

impl MetricsServerBuilder {
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

    pub async fn start(self, shutdown: impl Future<Output = ()> + Send) {
        let registry = crate::get_default_registry();

        let mut tasks = vec![];
        if let Some(listener_addr) = self.listener_addr {
            tasks.push(pull::start(listener_addr, registry.clone()).boxed());
        }

        if let Some(endpoint) = self.push_endpoint {
            tasks.push(push::start(endpoint, self.push_interval, registry).boxed());
        }

        if !tasks.is_empty() {
            future::select(shutdown.boxed(), future::join_all(tasks)).await;
        }
    }
}

impl Default for MetricsServerBuilder {
    fn default() -> Self {
        Self {
            listener_addr: None,
            push_endpoint: None,
            push_interval: Duration::from_secs(10),
        }
    }
}
