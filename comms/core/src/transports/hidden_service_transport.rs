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

use std::{io, io::ErrorKind, sync::Arc};

use log::*;
use multiaddr::Multiaddr;
use tokio::sync::RwLock;

use crate::{
    tor::{HiddenServiceController, TorIdentity},
    transports::{SocksTransport, Transport},
};

const LOG_TARGET: &str = "comms::transports::hidden_service_transport";

#[derive(thiserror::Error, Debug)]
pub enum HiddenServiceTransportError {
    #[error("Tor hidden service transport error: `{0}`")]
    HiddenServiceControllerError(#[from] crate::tor::HiddenServiceControllerError),
    #[error("Tor hidden service socks error: `{0}`")]
    SocksTransportError(#[from] io::Error),
}

struct HiddenServiceTransportInner {
    socks_transport: Option<SocksTransport>,
    hidden_service_ctl: HiddenServiceController,
}

#[derive(Clone)]
pub struct HiddenServiceTransport<F: Fn(TorIdentity)> {
    inner: Arc<RwLock<HiddenServiceTransportInner>>,
    after_init: F,
}

impl<F: Fn(TorIdentity)> HiddenServiceTransport<F> {
    pub fn new(hidden_service_ctl: HiddenServiceController, after_init: F) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HiddenServiceTransportInner {
                socks_transport: None,
                hidden_service_ctl,
            })),
            after_init,
        }
    }

    async fn ensure_initialized(&self) -> Result<(), io::Error> {
        let inner = self.inner.read().await;
        if inner.socks_transport.is_none() {
            drop(inner);
            let mut mut_inner = self.inner.write().await;
            if mut_inner.socks_transport.is_none() {
                let transport = mut_inner.hidden_service_ctl.initialize_transport().await.map_err(|e| {
                    error!(
                        target: LOG_TARGET,
                        "Error initializing hidden transport service stack{}",
                        e
                    );
                    io::Error::new(ErrorKind::Other, e.to_string())
                })?;
                (self.after_init)(
                    mut_inner
                        .hidden_service_ctl
                        .identity
                        .clone()
                        .ok_or(io::Error::new(ErrorKind::Other, "Missing tor identity".to_string()))?,
                );
                mut_inner.socks_transport = Some(transport);
            }
        }
        Ok(())
    }
}
#[crate::async_trait]
impl<F: Fn(TorIdentity) + Send + Sync> Transport for HiddenServiceTransport<F> {
    type Error = <SocksTransport as Transport>::Error;
    type Listener = <SocksTransport as Transport>::Listener;
    type Output = <SocksTransport as Transport>::Output;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        self.ensure_initialized().await?;
        let inner = self.inner.read().await;

        Ok(inner.socks_transport.as_ref().unwrap().listen(addr).await?)
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        self.ensure_initialized().await?;
        let inner = self.inner.read().await;
        Ok(inner.socks_transport.as_ref().unwrap().dial(addr).await?)
    }
}
