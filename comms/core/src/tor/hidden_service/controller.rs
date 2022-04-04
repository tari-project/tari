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

use std::{net::SocketAddr, sync::Arc, time::Duration};

use futures::{future, future::Either, pin_mut, StreamExt};
use log::*;
use tari_shutdown::OptionalShutdownSignal;
use thiserror::Error;
use tokio::{sync::broadcast, time};

use crate::{
    multiaddr::Multiaddr,
    runtime::task,
    socks,
    tor::{
        control_client::{
            commands::{AddOnionFlag, AddOnionResponse},
            TorControlEvent,
        },
        hidden_service::TorProxyOpts,
        Authentication,
        HiddenService,
        HsFlags,
        PortMapping,
        TorClientError,
        TorControlPortClient,
        TorIdentity,
    },
    transports::{SocksConfig, SocksTransport},
    utils::multiaddr::{multiaddr_to_socketaddr, socketaddr_to_multiaddr},
};

const LOG_TARGET: &str = "comms::tor::hidden_service_controller";

#[derive(Debug, Error)]
pub enum HiddenServiceControllerError {
    #[error("Tor client is not connected")]
    NotConnected,
    #[error("Failed to parse SOCKS address returned by control port")]
    FailedToParseSocksAddress,
    #[error("TorClientError: {0}")]
    TorClientError(#[from] TorClientError),
    #[error("Unable to connect to the Tor control port")]
    TorControlPortOffline,
    #[error("The given tor service id is not a valid detached service id")]
    InvalidDetachedServiceId,
    #[error("The shutdown signal interrupted the HiddenServiceController")]
    ShutdownSignalInterrupt,
}

pub struct HiddenServiceController {
    client: Option<TorControlPortClient>,
    control_server_addr: Multiaddr,
    control_server_auth: Authentication,
    proxied_port_mapping: PortMapping,
    socks_address_override: Option<Multiaddr>,
    socks_auth: socks::Authentication,
    identity: Option<TorIdentity>,
    hs_flags: HsFlags,
    is_authenticated: bool,
    proxy_opts: TorProxyOpts,
    shutdown_signal: OptionalShutdownSignal,
}

impl HiddenServiceController {
    pub(super) fn new(
        control_server_addr: Multiaddr,
        control_server_auth: Authentication,
        proxied_port_mapping: PortMapping,
        socks_address_override: Option<Multiaddr>,
        socks_auth: socks::Authentication,
        identity: Option<TorIdentity>,
        hs_flags: HsFlags,
        proxy_opts: TorProxyOpts,
        shutdown_signal: OptionalShutdownSignal,
    ) -> Self {
        Self {
            client: None,
            control_server_addr,
            control_server_auth,
            socks_address_override,
            proxied_port_mapping,
            socks_auth,
            hs_flags,
            identity,
            is_authenticated: false,
            proxy_opts,
            shutdown_signal,
        }
    }

    /// The address to which all tor traffic is proxied. A TCP socket should be bound to this address to receive traffic
    /// for this hidden service.
    pub fn proxied_address(&self) -> Multiaddr {
        socketaddr_to_multiaddr(self.proxied_port_mapping.proxied_address())
    }

    pub async fn initialize_transport(&mut self) -> Result<SocksTransport, HiddenServiceControllerError> {
        self.connect_and_auth().await?;
        let socks_addr = self.get_socks_address().await?;
        Ok(SocksTransport::new(SocksConfig {
            proxy_address: socks_addr,
            authentication: self.socks_auth.clone(),
            proxy_bypass_predicate: Arc::new(self.proxy_opts.to_bypass_predicate()),
        }))
    }

    /// Connects, authenticates to the Tor control port and creates a hidden service using the tor identity if provided,
    /// otherwise a new tor identity will be created. The creation of a hidden service is idempotent i.e. if the
    /// hidden service exists, the
    pub async fn create_hidden_service(mut self) -> Result<HiddenService, HiddenServiceControllerError> {
        self.connect_and_auth().await?;
        self.set_events().await?;

        let hidden_service = self.create_hidden_service_from_identity().await?;
        let mut shutdown_signal = hidden_service.shutdown_signal.clone();
        let mut event_stream = self.client.as_ref().unwrap().get_event_stream();

        task::spawn({
            async move {
                loop {
                    let either = future::select(&mut shutdown_signal, event_stream.next()).await;
                    match either {
                        Either::Left(_) => {
                            debug!(
                                target: LOG_TARGET,
                                "Tor controller shut down because the shutdown signal was received"
                            );
                            break;
                        },
                        Either::Right((Some(Ok(TorControlEvent::TorControlDisconnected)), shutdown_signal)) => {
                            let event_tx = self
                                .client
                                .as_ref()
                                .map(|c| c.event_sender().clone())
                                .expect("HiddenServiceController::client was None");
                            warn!(
                                target: LOG_TARGET,
                                "Tor control server disconnected. Attempting to reestablish connection..."
                            );
                            if let Err(err) = self.reestablish_hidden_service(event_tx, shutdown_signal).await {
                                error!(
                                    target: LOG_TARGET,
                                    "Failed to reestablish connection to tor control server because '{:?}'", err
                                );
                                break;
                            }
                        },
                        Either::Right((Some(Ok(evt)), _)) => {
                            trace!(target: LOG_TARGET, "Tor control event: {:?}", evt);
                        },
                        _ => {},
                    }
                }
            }
        });

        Ok(hidden_service)
    }

    pub async fn connect_and_auth(&mut self) -> Result<(), HiddenServiceControllerError> {
        if !self.is_authenticated {
            self.connect().await?;
            self.authenticate().await?;
            self.is_authenticated = true;
        }
        Ok(())
    }

    async fn reestablish_hidden_service(
        &mut self,
        event_tx: broadcast::Sender<TorControlEvent>,
        shutdown_signal: &mut OptionalShutdownSignal,
    ) -> Result<(), HiddenServiceControllerError> {
        let mut signal = Some(shutdown_signal);
        loop {
            warn!(
                target: LOG_TARGET,
                "Attempting to reestablish control port connection at '{}'", self.control_server_addr
            );
            let connect_fut = TorControlPortClient::connect(self.control_server_addr.clone(), event_tx.clone());
            pin_mut!(connect_fut);
            let either = future::select(connect_fut, signal.take().expect("signal was None")).await;
            match either {
                Either::Left((Ok(client), _)) => {
                    self.client = Some(client);
                    self.authenticate().await?;
                    self.set_events().await?;
                    let _ = self.create_hidden_service_from_identity().await;
                    break Ok(());
                },
                Either::Left((Err(err), shutdown_signal)) => {
                    signal = Some(shutdown_signal);
                    warn!(
                        target: LOG_TARGET,
                        "Failed to reestablish connection with tor control server because '{:?}'", err
                    );
                    warn!(target: LOG_TARGET, "Will attempt again in 5 seconds...");
                    time::sleep(Duration::from_secs(5)).await;
                },

                Either::Right(_) => {
                    break Err(HiddenServiceControllerError::ShutdownSignalInterrupt);
                },
            }
        }
    }

    fn client_mut(&mut self) -> Result<&mut TorControlPortClient, HiddenServiceControllerError> {
        self.client
            .as_mut()
            .filter(|c| c.is_connected())
            .ok_or(HiddenServiceControllerError::NotConnected)
    }

    async fn connect(&mut self) -> Result<(), HiddenServiceControllerError> {
        if self.client.is_some() {
            return Ok(());
        }

        let (event_tx, _) = broadcast::channel(20);
        let client = TorControlPortClient::connect(self.control_server_addr.clone(), event_tx)
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "Tor client error: {:?}", err);
                HiddenServiceControllerError::TorControlPortOffline
            })?;

        self.client = Some(client);
        Ok(())
    }

    async fn authenticate(&mut self) -> Result<(), HiddenServiceControllerError> {
        let auth = self.control_server_auth.clone();
        self.client_mut()?.authenticate(&auth).await?;
        Ok(())
    }

    async fn set_events(&mut self) -> Result<(), HiddenServiceControllerError> {
        self.client_mut()?.set_events(&["NETWORK_LIVENESS"]).await?;
        Ok(())
    }

    async fn get_socks_address(&mut self) -> Result<Multiaddr, HiddenServiceControllerError> {
        match self.socks_address_override {
            Some(ref addr) => {
                debug!(
                    target: LOG_TARGET,
                    "Using SOCKS override '{}' for tor SOCKS proxy", addr
                );
                Ok(addr.clone())
            },
            None => {
                // Get configured SOCK5 address from Tor
                let socks_addrs = self.client_mut()?.get_info("net/listeners/socks").await?;

                let addr = socks_addrs
                    .iter()
                    .map(|addr| addr.parse::<SocketAddr>())
                    .filter_map(Result::ok)
                    .map(|addr| socketaddr_to_multiaddr(&addr))
                    .next()
                    .ok_or(HiddenServiceControllerError::FailedToParseSocksAddress)?;

                Ok(addr)
            },
        }
    }

    async fn create_hidden_service_from_identity(&mut self) -> Result<HiddenService, HiddenServiceControllerError> {
        let socks_addr = self.get_socks_address().await?;
        debug!(target: LOG_TARGET, "Tor SOCKS address is '{}'", socks_addr);

        // Initialize a onion hidden service - either from the given private key or by creating a new one
        match self.identity.take() {
            Some(identity) => {
                let resp = self.create_or_reuse_onion(&identity).await?;
                self.identity = Some(TorIdentity {
                    onion_port: resp.onion_port,
                    ..identity
                });
            },
            None => {
                let port_mapping = self.proxied_port_mapping;
                let resp = self.client_mut()?.add_onion(vec![], port_mapping, None).await?;
                let private_key = resp
                    .private_key
                    .clone()
                    .expect("Tor server MUST return private key according to spec");

                self.identity = Some(TorIdentity {
                    private_key,
                    service_id: resp.service_id,
                    onion_port: resp.onion_port,
                });
            },
        };

        let identity = self.identity.as_ref().map(Clone::clone).expect("already checked");
        debug!(
            target: LOG_TARGET,
            "Added hidden service with service id '{}' on port '{}'", identity.service_id, identity.onion_port
        );

        let proxied_addr = socketaddr_to_multiaddr(self.proxied_port_mapping.proxied_address());

        Ok(HiddenService {
            identity,
            proxied_addr,
            shutdown_signal: self.shutdown_signal.clone(),
        })
    }

    pub fn set_proxied_addr(&mut self, addr: Multiaddr) {
        self.proxied_port_mapping.set_proxied_addr(
            multiaddr_to_socketaddr(&addr).expect("set_proxied_addr: multiaddr must be a valid TCP socket address"),
        )
    }

    async fn create_or_reuse_onion(
        &mut self,
        identity: &TorIdentity,
    ) -> Result<AddOnionResponse, HiddenServiceControllerError> {
        let mut flags = Vec::new();
        if self.hs_flags.contains(HsFlags::DETACH) {
            flags.push(AddOnionFlag::Detach);
        }

        let port_mapping = self.proxied_port_mapping;

        let client = self.client_mut()?;

        loop {
            let result = client
                .add_onion_from_private_key(&identity.private_key, flags.clone(), port_mapping, None)
                .await;

            match result {
                Ok(resp) => break Ok(resp),
                Err(TorClientError::OnionAddressCollision) => {
                    debug!(target: LOG_TARGET, "Onion address is already registered.");

                    let detached = client.get_info("onions/detached").await?;
                    debug!(
                        target: LOG_TARGET,
                        "Checking that the active detached service IDs '{}' to expected service id '{}'",
                        detached.join(", "),
                        identity.service_id
                    );

                    if detached.iter().all(|svc_id| **svc_id != identity.service_id) {
                        return Err(HiddenServiceControllerError::InvalidDetachedServiceId);
                    }
                    debug!(
                        target: LOG_TARGET,
                        "Deleting duplicate onion service `{}` and then recreating it.", identity.service_id
                    );
                    client.del_onion(&identity.service_id).await?;
                    continue;
                },
                Err(err) => break Err(err.into()),
            }
        }
    }
}
