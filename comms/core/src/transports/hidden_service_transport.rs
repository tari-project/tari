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
use multiaddr::{multiaddr, Multiaddr, Protocol};
use tokio::sync::RwLock;

use crate::{
    tor::{HiddenServiceController, TorIdentity},
    transports::{tcp::TcpInbound, SocksTransport, Transport},
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
    hidden_service_ctl: Option<HiddenServiceController>,
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
                hidden_service_ctl: Some(hidden_service_ctl),
            })),
            after_init,
        }
    }

    async fn is_initialized(&self) -> bool {
        self.inner.read().await.socks_transport.is_some()
    }

    async fn initialize(&self, listen_addr: &Multiaddr) -> Result<(TcpInbound, Multiaddr), io::Error> {
        let mut inner_mut = self.inner.write().await;
        let mut hs_ctl = inner_mut.hidden_service_ctl.take().ok_or(io::Error::new(
            ErrorKind::Other,
            "BUG: Hidden service controller not set in transport".to_string(),
        ))?;

        let transport = hs_ctl.initialize_transport().await.map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Error initializing hidden transport service stack{}",
                e
            );
            io::Error::new(ErrorKind::Other, e.to_string())
        })?;
        let (inbound, listen_addr) = transport.listen(listen_addr).await?;
        inner_mut.socks_transport = Some(transport);

        // Set the proxied address to the port we just listened on
        let mut proxied_addr = hs_ctl.proxied_address();
        if proxied_addr.ends_with(&multiaddr!(Tcp(0u16))) {
            if let Some(Protocol::Tcp(port)) = listen_addr.iter().last() {
                proxied_addr.pop();
                proxied_addr.push(Protocol::Tcp(port));
            }
            hs_ctl.set_proxied_addr(&proxied_addr);
        }

        let hidden_service = hs_ctl.create_hidden_service().await.map_err(|err| {
            error!(
                target: LOG_TARGET,
                "Error creating hidden service: {}",
                err
            );
            io::Error::new(ErrorKind::Other, err.to_string())
        })?;

        (self.after_init)(hidden_service.tor_identity().clone());
        Ok((inbound, listen_addr))
    }
}
#[crate::async_trait]
impl<F: Fn(TorIdentity) + Send + Sync> Transport for HiddenServiceTransport<F> {
    type Error = <SocksTransport as Transport>::Error;
    type Listener = <SocksTransport as Transport>::Listener;
    type Output = <SocksTransport as Transport>::Output;

    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error> {
        if self.is_initialized().await {
            // For now, we only can listen on a single Tor hidden service. This behaviour is not technically correct as
            // per the Transport trait, but we only ever call listen once in practice. The fix for this is to
            // improve the tor client implementation to allow for multiple hidden services.
            return Err(io::Error::new(
                ErrorKind::Other,
                "BUG: Hidden service transport already initialized".to_string(),
            ));
        }
        let (listener, addr) = self.initialize(addr).await?;
        Ok((listener, addr))
    }

    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error> {
        let inner = self.inner.read().await;
        let transport = inner.socks_transport.as_ref().ok_or_else(|| {
            io::Error::new(
                ErrorKind::Other,
                "BUG: Hidden service transport not initialized before dialling".to_string(),
            )
        })?;
        transport.dial(addr).await
    }
}
