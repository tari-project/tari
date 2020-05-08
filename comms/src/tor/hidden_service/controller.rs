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

use crate::{
    multiaddr::Multiaddr,
    socks,
    tor::{
        control_client::{
            commands::{AddOnionFlag, AddOnionResponse},
            TorControlEvent,
        },
        Authentication,
        HiddenService,
        HsFlags,
        PortMapping,
        TorClientError,
        TorControlPortClient,
        TorIdentity,
    },
    utils::multiaddr::socketaddr_to_multiaddr,
};
use derive_error::Error;
use futures::{future, future::Either, pin_mut, StreamExt};
use log::*;
use std::{net::SocketAddr, time::Duration};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{sync::broadcast, task, time};

const LOG_TARGET: &str = "comms::tor::hidden_service_controller";

#[derive(Debug, Error)]
pub enum HiddenServiceControllerError {
    /// Tor client is not connected
    NotConnected,
    /// Failed to parse SOCKS address returned by control port
    FailedToParseSocksAddress,
    TorClientError(TorClientError),
    /// Unable to connect to the Tor control port
    TorControlPortOffline,
    /// The given tor service id is not a valid detached service id
    InvalidDetachedServiceId,
    /// The shutdown signal interrupted the HiddenServiceController
    ShutdownSignalInterrupt,
}

pub struct HiddenServiceController {
    pub(super) client: Option<TorControlPortClient>,
    pub(super) control_server_addr: Multiaddr,
    pub(super) control_server_auth: Authentication,
    pub(super) proxied_port_mapping: PortMapping,
    pub(super) socks_address_override: Option<Multiaddr>,
    pub(super) socks_auth: socks::Authentication,
    pub(super) identity: Option<TorIdentity>,
    pub(super) hs_flags: HsFlags,
}

impl HiddenServiceController {
    pub async fn start_hidden_service(mut self) -> Result<HiddenService, HiddenServiceControllerError> {
        self.connect().await?;
        self.authenticate().await?;
        self.set_events().await?;

        let hidden_service = self.create_hidden_service().await?;
        let mut shutdown_signal = hidden_service.shutdown.to_signal();
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

    async fn reestablish_hidden_service(
        &mut self,
        event_tx: broadcast::Sender<TorControlEvent>,
        shutdown_signal: &mut ShutdownSignal,
    ) -> Result<(), HiddenServiceControllerError>
    {
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
                    let _ = self.create_hidden_service().await;
                    break Ok(());
                },
                Either::Left((Err(err), shutdown_signal)) => {
                    signal = Some(shutdown_signal);
                    warn!(
                        target: LOG_TARGET,
                        "Failed to reestablish connection with tor control server because '{:?}'", err
                    );
                    warn!(target: LOG_TARGET, "Will attempt again in 10 seconds...");
                    time::delay_for(Duration::from_secs(10)).await;
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
            .ok_or_else(|| HiddenServiceControllerError::NotConnected)
    }

    async fn connect(&mut self) -> Result<(), HiddenServiceControllerError> {
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
                    .ok_or_else(|| HiddenServiceControllerError::FailedToParseSocksAddress)?;

                Ok(addr)
            },
        }
    }

    async fn create_hidden_service(&mut self) -> Result<HiddenService, HiddenServiceControllerError> {
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
                let port_mapping = self.proxied_port_mapping.clone();
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
            socks_addr,
            socks_auth: self.socks_auth.clone(),
            identity,
            proxied_addr,
            shutdown: Shutdown::new(),
        })
    }

    async fn create_or_reuse_onion(
        &mut self,
        identity: &TorIdentity,
    ) -> Result<AddOnionResponse, HiddenServiceControllerError>
    {
        let mut flags = Vec::new();
        if self.hs_flags.contains(HsFlags::DETACH) {
            flags.push(AddOnionFlag::Detach);
        }

        let port_mapping = self.proxied_port_mapping.clone();

        let client = self.client_mut()?;

        let result = client
            .add_onion_from_private_key(&identity.private_key, flags, port_mapping, None)
            .await;

        match result {
            Ok(resp) => Ok(resp),
            Err(TorClientError::OnionAddressCollision) => {
                debug!(target: LOG_TARGET, "Onion address is already registered.");

                let detached = client.get_info("onions/detached").await?;
                debug!(
                    target: LOG_TARGET,
                    "Comparing active detached service IDs '{}' to expected service id '{}'",
                    detached.join(", "),
                    identity.service_id
                );

                if detached.iter().all(|svc_id| **svc_id != identity.service_id) {
                    return Err(HiddenServiceControllerError::InvalidDetachedServiceId);
                }

                Ok(AddOnionResponse {
                    // TODO(sdbondi): This could be a different ORPort than the one requested in port mapping, I was not
                    //                able to find a way to find the port mapping for the service.
                    //                Setting the onion_port to be the same as the original port may cause
                    //                confusion/break "just works"(tm)
                    onion_port: identity.onion_port,
                    service_id: identity.service_id.clone(),
                    private_key: Some(identity.private_key.clone()),
                })
            },
            Err(err) => Err(err.into()),
        }
    }
}
