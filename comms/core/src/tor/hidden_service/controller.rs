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

use std::{io, net::SocketAddr, path::Path, sync::Arc, time::Duration};

use futures::{StreamExt, future, future::Either};
use log::*;
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::{sync::broadcast, time};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    multiaddr::Multiaddr,
    socks,
    tor::{
        Authentication,
        HiddenService,
        HsFlags,
        PortMapping,
        TorClientError,
        TorControlPortClient,
        TorIdentity,
        control_client::{
            TorControlEvent,
            commands::{AddOnionFlag, AddOnionResponse},
            read_cookie_file_hex,
        },
        hidden_service::TorProxyOpts,
    },
    transports::{SocksConfig, SocksTransport},
    utils::multiaddr::{multiaddr_to_socketaddr, socketaddr_to_multiaddr},
};

const LOG_TARGET: &str = "comms::tor::hidden_service_controller";
const DEFAULT_RECONNECT_RETRY_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum HiddenServiceControllerError {
    #[error("Tor client is not connected")]
    NotConnected,
    #[error("Failed to parse SOCKS address returned by control port")]
    FailedToParseSocksAddress,
    #[error("TorClientError: {0}")]
    TorClientError(#[from] TorClientError),
    #[error(
        "Unable to connect to the Tor control port. Make sure tor is running and 'ControlPort 9051' is in your torrc."
    )]
    TorControlPortOffline,
    #[error("The given tor service id is not a valid detached service id")]
    InvalidDetachedServiceId,
    #[error("The shutdown signal interrupted the HiddenServiceController")]
    ShutdownSignalInterrupt,
    #[error("Tor is configured to use a hashed password for control port auth. Please add the.")]
    HashedPasswordAuthAutoNotSupported,
    #[error("Tor is configured with an unsupported authentication method '{0}'.")]
    UnrecognizedAuthenticationMethod(String),
    #[error("Failed to load tor cookie file: {0}")]
    FailedToLoadCookieFile(io::Error),
}

pub struct HiddenServiceController {
    client: Option<TorControlPortClient>,
    control_server_addr: Multiaddr,
    control_server_auth: Authentication,
    proxied_port_mapping: PortMapping,
    socks_address_override: Option<Multiaddr>,
    socks_auth: socks::Authentication,
    pub identity: Option<TorIdentity>,
    hs_flags: HsFlags,
    is_authenticated: bool,
    proxy_opts: TorProxyOpts,
    reconnect_retry_interval: Duration,
    shutdown_signal: ShutdownSignal,
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
        shutdown_signal: ShutdownSignal,
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
            reconnect_retry_interval: DEFAULT_RECONNECT_RETRY_INTERVAL,
            shutdown_signal,
        }
    }

    #[cfg(test)]
    fn set_reconnect_retry_interval(&mut self, interval: Duration) {
        self.reconnect_retry_interval = interval;
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
        let mut event_stream = self
            .client
            .as_ref()
            .ok_or(HiddenServiceControllerError::NotConnected)?
            .get_event_stream();

        tokio::spawn({
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
                            warn!(
                                target: LOG_TARGET,
                                "Tor control server disconnected. Attempting to reestablish connection..."
                            );
                            match self.reestablish_hidden_service(shutdown_signal).await {
                                Ok(new_event_stream) => {
                                    event_stream = new_event_stream;
                                },
                                Err(err) => {
                                    error!(
                                        target: LOG_TARGET,
                                        "Failed to reestablish connection to tor control server because '{err:?}'"
                                    );
                                    break;
                                },
                            }
                        },
                        Either::Right((Some(Ok(evt)), _)) => {
                            trace!(target: LOG_TARGET, "Tor control event: {evt:?}");
                        },
                        Either::Right((Some(Err(err)), shutdown_signal)) => {
                            warn!(target: LOG_TARGET, "Tor control event stream lagged: {err:?}");
                            if !self.client.as_ref().is_some_and(TorControlPortClient::is_connected) {
                                warn!(
                                    target: LOG_TARGET,
                                    "Tor control server disconnected while event stream lagged. Attempting to \
                                     reestablish connection..."
                                );
                                match self.reestablish_hidden_service(shutdown_signal).await {
                                    Ok(new_event_stream) => {
                                        event_stream = new_event_stream;
                                    },
                                    Err(reconn_err) => {
                                        error!(
                                            target: LOG_TARGET,
                                            "Failed to reestablish connection to tor control server because \
                                             '{reconn_err:?}'"
                                        );
                                        break;
                                    },
                                }
                            }
                        },
                        Either::Right((None, _)) => {
                            debug!(
                                target: LOG_TARGET,
                                "Tor control event stream ended. Shutting down hidden service controller."
                            );
                            break;
                        },
                    }
                }
            }
        });

        Ok(hidden_service)
    }

    pub async fn connect_and_auth(&mut self) -> Result<(), HiddenServiceControllerError> {
        if !self.client.as_ref().is_some_and(TorControlPortClient::is_connected) {
            self.client = None;
            self.is_authenticated = false;
        }
        if !self.is_authenticated {
            self.connect().await?;
            self.authenticate().await?;
        }
        Ok(())
    }

    async fn reestablish_hidden_service(
        &mut self,
        shutdown_signal: &mut ShutdownSignal,
    ) -> Result<BroadcastStream<TorControlEvent>, HiddenServiceControllerError> {
        self.client = None;
        self.is_authenticated = false;

        // Every await below is raced against the shutdown signal, which is polled first, so a tor control port that is
        // down or unresponsive never delays shutdown.
        loop {
            warn!(
                target: LOG_TARGET,
                "Attempting to reestablish control port connection at '{}'", self.control_server_addr
            );
            let (event_tx, event_rx) = broadcast::channel(20);
            let connect_fut = TorControlPortClient::connect(self.control_server_addr.clone(), event_tx);
            let result = tokio::select! {
                biased;
                _ = &mut *shutdown_signal => return Err(HiddenServiceControllerError::ShutdownSignalInterrupt),
                result = connect_fut => result,
            };
            match result {
                Ok(client) => {
                    info!(target: LOG_TARGET, "Connection to tor control port re-established");
                    self.client = Some(client);
                    self.is_authenticated = false;
                    let reinit_result = tokio::select! {
                        biased;
                        _ = &mut *shutdown_signal => return Err(HiddenServiceControllerError::ShutdownSignalInterrupt),
                        result = self.reinitialize_after_reconnect() => result,
                    };
                    match reinit_result {
                        Ok(()) => {
                            info!(
                                target: LOG_TARGET,
                                "Tor hidden service re-established after control port reconnect"
                            );
                            return Ok(BroadcastStream::new(event_rx));
                        },
                        Err(err) => {
                            self.client = None;
                            self.is_authenticated = false;
                            warn!(
                                target: LOG_TARGET,
                                "Failed to reinitialize hidden service after reconnecting to tor control server \
                                 because '{err:?}'. Will attempt again in {:?}...",
                                self.reconnect_retry_interval
                            );
                        },
                    }
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to reestablish connection with tor control server because '{err:?}'"
                    );
                    warn!(
                        target: LOG_TARGET,
                        "Will attempt again in {:?}...",
                        self.reconnect_retry_interval
                    );
                },
            }

            tokio::select! {
                biased;
                _ = &mut *shutdown_signal => return Err(HiddenServiceControllerError::ShutdownSignalInterrupt),
                _ = time::sleep(self.reconnect_retry_interval) => {},
            }
        }
    }

    /// Authenticate, subscribe to events and re-create the hidden service on a freshly reconnected control port.
    async fn reinitialize_after_reconnect(&mut self) -> Result<(), HiddenServiceControllerError> {
        self.authenticate().await?;
        self.set_events().await?;
        drop(self.create_hidden_service_from_identity().await?);
        Ok(())
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
                error!(target: LOG_TARGET, "Tor client error: {err:?}");
                HiddenServiceControllerError::TorControlPortOffline
            })?;

        self.client = Some(client);
        Ok(())
    }

    async fn detect_authentication(&mut self) -> Result<Authentication, HiddenServiceControllerError> {
        let client = self.client_mut()?;
        let info = client.protocol_info().await?;
        info!(
            target: LOG_TARGET,
            "Detected tor v{} configured with control port auth: {}",
            info.tor_version,
            info.auth_methods.methods.join(", ")
        );
        if info.auth_methods.methods.iter().any(|s| s == "COOKIE") {
            let cookie_path = info.auth_methods.cookie_file.ok_or_else(|| {
                TorClientError::ServerInvalidResponse(
                    "PROTOCOLINFO: COOKIE auth however no cookie file was specified".to_string(),
                )
            })?;

            let data = read_cookie_file_hex(Path::new(&cookie_path))
                .map_err(HiddenServiceControllerError::FailedToLoadCookieFile)?;
            Ok(Authentication::Cookie(data))
        } else if info.auth_methods.methods.iter().any(|s| s == "NULL") {
            Ok(Authentication::None)
        } else if info.auth_methods.methods.iter().any(|s| s == "HASHEDPASSWORD") {
            Err(HiddenServiceControllerError::HashedPasswordAuthAutoNotSupported)
        } else {
            Err(HiddenServiceControllerError::UnrecognizedAuthenticationMethod(
                info.auth_methods
                    .methods
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "<none>".to_string()),
            ))
        }
    }

    async fn authenticate(&mut self) -> Result<(), HiddenServiceControllerError> {
        let auth = match &self.control_server_auth {
            Authentication::Auto => self.detect_authentication().await?,
            Authentication::CookieFile(path) => {
                let data = read_cookie_file_hex(path).map_err(HiddenServiceControllerError::FailedToLoadCookieFile)?;
                Authentication::Cookie(data)
            },
            other => other.clone(),
        };
        self.client_mut()?.authenticate(&auth).await?;
        self.is_authenticated = true;
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
                    "Using SOCKS override '{addr}' for tor SOCKS proxy"
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
        debug!(target: LOG_TARGET, "Tor SOCKS address is '{socks_addr}'" );

        // Initialize a onion hidden service - either from the given private key or by creating a new one
        match self.identity.clone() {
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

        let identity = self.identity.clone().expect("already checked");
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

    pub fn set_proxied_addr(&mut self, addr: &Multiaddr) {
        self.proxied_port_mapping.set_proxied_addr(
            multiaddr_to_socketaddr(addr).expect("set_proxied_addr: multiaddr must be a valid TCP socket address"),
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

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::{fs, path::PathBuf};

    use futures::SinkExt;
    use tari_shutdown::Shutdown;
    use tari_utilities::hex::Hex;
    use tokio::{
        net::{TcpListener, TcpStream},
        sync::{Mutex, mpsc, oneshot},
    };
    use tokio_util::codec::{Framed, LinesCodec};

    use super::*;
    use crate::tor::HiddenServiceBuilder;

    const TEST_SERVICE_ID: &str = "qigbgbs4ue3ghbupsotgh73cmmkjrin2aprlyxsrnrvpmcmzy3g4wbid";
    const TEST_ED25519_KEY: &str =
        "Pg3GEyssauPRW3jP6mHwKOxvl_fMsF0QsZC3DvQ8jZ9AxmfRvSP35m9l0vOYyOxkOqWM6ufjdYuM8Ae6cR2UdreG6";

    #[derive(Clone)]
    struct MockControlServer {
        addr: Multiaddr,
        expected_cookie_hex: Arc<Mutex<String>>,
        recorded_commands: Arc<Mutex<Vec<String>>>,
        failed_auth_tx: mpsc::UnboundedSender<String>,
        active_sessions: Arc<Mutex<Vec<oneshot::Sender<()>>>>,
    }

    impl MockControlServer {
        async fn spawn(
            initial_cookie_hex: String,
            cookie_file_path: PathBuf,
        ) -> (Self, mpsc::UnboundedReceiver<String>, mpsc::UnboundedReceiver<String>) {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let addr = socketaddr_to_multiaddr(&listener.local_addr().unwrap());
            let (add_onion_tx, add_onion_rx) = mpsc::unbounded_channel();
            let (failed_auth_tx, failed_auth_rx) = mpsc::unbounded_channel();

            let server = Self {
                addr,
                expected_cookie_hex: Arc::new(Mutex::new(initial_cookie_hex)),
                recorded_commands: Arc::new(Mutex::new(Vec::new())),
                failed_auth_tx,
                active_sessions: Arc::new(Mutex::new(Vec::new())),
            };

            let server_clone = server.clone();
            tokio::spawn(async move {
                while let Ok((socket, _)) = listener.accept().await {
                    let (kill_tx, kill_rx) = oneshot::channel();
                    server_clone.active_sessions.lock().await.push(kill_tx);
                    let session_server = server_clone.clone();
                    let session_add_onion_tx = add_onion_tx.clone();
                    let session_cookie_path = cookie_file_path.clone();
                    tokio::spawn(async move {
                        handle_mock_session(
                            socket,
                            session_server,
                            session_cookie_path,
                            session_add_onion_tx,
                            kill_rx,
                        )
                        .await;
                    });
                }
            });

            (server, add_onion_rx, failed_auth_rx)
        }

        async fn set_expected_cookie(&self, cookie_bytes: &[u8]) {
            *self.expected_cookie_hex.lock().await = cookie_bytes.to_vec().to_hex();
        }

        async fn disconnect_active_sessions(&self) {
            let killers: Vec<_> = self.active_sessions.lock().await.drain(..).collect();
            for tx in killers {
                let _ignored = tx.send(());
            }
        }

        async fn recorded_commands(&self) -> Vec<String> {
            self.recorded_commands.lock().await.clone()
        }
    }

    async fn handle_mock_session(
        socket: TcpStream,
        server: MockControlServer,
        cookie_file_path: PathBuf,
        add_onion_tx: mpsc::UnboundedSender<String>,
        mut kill_rx: oneshot::Receiver<()>,
    ) {
        let mut framed = Framed::new(socket, LinesCodec::new());
        let mut authenticated = false;
        let escaped_cookie_path = cookie_file_path
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");

        loop {
            let line = tokio::select! {
                biased;
                _ = &mut kill_rx => break,
                next = framed.next() => match next {
                    Some(Ok(line)) => line,
                    _ => break,
                },
            };

            server.recorded_commands.lock().await.push(line.clone());

            if line == "PROTOCOLINFO" {
                let lines = [
                    "250-PROTOCOLINFO 1".to_string(),
                    format!("250-AUTH METHODS=COOKIE,SAFECOOKIE COOKIEFILE=\"{escaped_cookie_path}\""),
                    "250-VERSION Tor=\"0.4.8.12\"".to_string(),
                    "250 OK".to_string(),
                ];
                for resp in lines {
                    if framed.send(resp).await.is_err() {
                        return;
                    }
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("AUTHENTICATE") {
                let supplied = rest.trim();
                let expected = server.expected_cookie_hex.lock().await.clone();
                if !supplied.is_empty() && supplied.eq_ignore_ascii_case(&expected) {
                    authenticated = true;
                    let _ignored = framed.send("250 OK".to_string()).await;
                } else {
                    let _ignored = server.failed_auth_tx.send(supplied.to_string());
                    let _ignored = framed
                        .send(
                            "515 Authentication failed: Authentication cookie did not match expected value."
                                .to_string(),
                        )
                        .await;
                    break;
                }
                continue;
            }

            if !authenticated {
                let _ignored = framed.send("514 Authentication required.".to_string()).await;
                break;
            }

            if line == "SETEVENTS NETWORK_LIVENESS" {
                if framed.send("250 OK".to_string()).await.is_err() {
                    break;
                }
            } else if line == "GETINFO net/listeners/socks" {
                for resp in ["250-net/listeners/socks=\"127.0.0.1:9050\"", "250 OK"] {
                    if framed.send(resp.to_string()).await.is_err() {
                        return;
                    }
                }
            } else if line.starts_with("ADD_ONION NEW:BEST ") {
                let responses = [
                    format!("250-ServiceID={TEST_SERVICE_ID}"),
                    format!("250-PrivateKey=ED25519-V3:{TEST_ED25519_KEY}"),
                    "250 OK".to_string(),
                ];
                for resp in responses {
                    if framed.send(resp).await.is_err() {
                        return;
                    }
                }
                let _ignored = add_onion_tx.send(line);
            } else if line.starts_with("ADD_ONION ED25519-V3:") {
                let responses = [format!("250-ServiceID={TEST_SERVICE_ID}"), "250 OK".to_string()];
                for resp in responses {
                    if framed.send(resp).await.is_err() {
                        return;
                    }
                }
                let _ignored = add_onion_tx.send(line);
            } else {
                let _ignored = framed.send("552 Unrecognized command".to_string()).await;
            }
        }
    }

    fn build_test_controller(
        control_addr: Multiaddr,
        auth: Authentication,
        shutdown_signal: ShutdownSignal,
    ) -> HiddenServiceController {
        let port_mapping = PortMapping::new(18141, ([127, 0, 0, 1], 18142).into());
        let mut ctl = HiddenServiceBuilder::new()
            .with_control_server_address(control_addr)
            .with_control_server_auth(auth)
            .with_port_mapping(port_mapping)
            .with_shutdown_signal(shutdown_signal)
            .build()
            .unwrap();
        ctl.set_reconnect_retry_interval(Duration::from_millis(20));
        ctl
    }

    #[tokio::test]
    async fn cookie_file_reconnect_rereads_rotated_cookie_and_recreates_onion() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cookie_path = temp_dir.path().join("control_auth_cookie");
        let cookie_a = [0xaau8; 32];
        let cookie_b = [0xbbu8; 32];
        let cookie_c = [0xccu8; 32];
        fs::write(&cookie_path, cookie_a).unwrap();

        let (server, mut add_onion_rx, _failed_auth_rx) =
            MockControlServer::spawn(cookie_a.to_hex(), cookie_path.clone()).await;

        let shutdown = Shutdown::new();
        let ctl = build_test_controller(
            server.addr.clone(),
            Authentication::CookieFile(cookie_path.clone()),
            shutdown.to_signal(),
        );

        let hs = ctl.create_hidden_service().await.unwrap();
        assert_eq!(hs.service_id(), TEST_SERVICE_ID);

        let initial_add_onion = time::timeout(Duration::from_secs(5), add_onion_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(initial_add_onion, "ADD_ONION NEW:BEST Port=18141,127.0.0.1:18142");

        // Simulate Tor restart #1: rotate cookie file to cookie_b and drop active control port session.
        fs::write(&cookie_path, cookie_b).unwrap();
        server.set_expected_cookie(&cookie_b).await;
        server.disconnect_active_sessions().await;

        let reconnect_1_add_onion = time::timeout(Duration::from_secs(5), add_onion_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            reconnect_1_add_onion,
            format!("ADD_ONION ED25519-V3:{TEST_ED25519_KEY} Port=18141,127.0.0.1:18142")
        );

        // Simulate Tor restart #2: rotate cookie file to cookie_c and drop active control port session again.
        fs::write(&cookie_path, cookie_c).unwrap();
        server.set_expected_cookie(&cookie_c).await;
        server.disconnect_active_sessions().await;

        let reconnect_2_add_onion = time::timeout(Duration::from_secs(5), add_onion_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            reconnect_2_add_onion,
            format!("ADD_ONION ED25519-V3:{TEST_ED25519_KEY} Port=18141,127.0.0.1:18142")
        );

        let auth_cmds: Vec<_> = server
            .recorded_commands()
            .await
            .into_iter()
            .filter(|cmd| cmd.starts_with("AUTHENTICATE"))
            .collect();
        assert_eq!(auth_cmds, vec![
            format!("AUTHENTICATE {}", cookie_a.to_hex()),
            format!("AUTHENTICATE {}", cookie_b.to_hex()),
            format!("AUTHENTICATE {}", cookie_c.to_hex()),
        ]);

        shutdown.trigger();
    }

    #[tokio::test]
    async fn reconnect_retries_when_cookie_file_is_temporarily_missing_or_empty_during_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cookie_path = temp_dir.path().join("control_auth_cookie");
        let cookie_a = [0x11u8; 32];
        let cookie_b = [0x22u8; 32];
        fs::write(&cookie_path, cookie_a).unwrap();

        let (server, mut add_onion_rx, mut failed_auth_rx) =
            MockControlServer::spawn(cookie_a.to_hex(), cookie_path.clone()).await;

        let shutdown = Shutdown::new();
        let ctl = build_test_controller(
            server.addr.clone(),
            Authentication::CookieFile(cookie_path.clone()),
            shutdown.to_signal(),
        );

        let hs = ctl.create_hidden_service().await.unwrap();
        assert_eq!(hs.service_id(), TEST_SERVICE_ID);
        let _initial = time::timeout(Duration::from_secs(5), add_onion_rx.recv())
            .await
            .unwrap()
            .unwrap();

        // Simulate Universe deleting tor-data (cookie file missing, then empty, then stale, then valid cookie_b).
        fs::remove_file(&cookie_path).unwrap();
        server.set_expected_cookie(&cookie_b).await;
        server.disconnect_active_sessions().await;

        time::sleep(Duration::from_millis(50)).await;
        // Write truncated 0-byte cookie file mid-restart.
        fs::write(&cookie_path, []).unwrap();
        time::sleep(Duration::from_millis(50)).await;
        // Write stale cookie_a first so the server rejects AUTHENTICATE with 515.
        fs::write(&cookie_path, cookie_a).unwrap();
        let rejected_hex = time::timeout(Duration::from_secs(5), failed_auth_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rejected_hex, cookie_a.to_hex());

        // Now write rotated cookie_b; controller must retry and restore the exact onion identity.
        fs::write(&cookie_path, cookie_b).unwrap();
        let reestablished_onion = time::timeout(Duration::from_secs(5), add_onion_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            reestablished_onion,
            format!("ADD_ONION ED25519-V3:{TEST_ED25519_KEY} Port=18141,127.0.0.1:18142")
        );

        shutdown.trigger();
    }

    #[tokio::test]
    async fn auto_auth_reconnect_rereads_rotated_cookie_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cookie_path = temp_dir.path().join("control_auth_cookie");
        let cookie_a = [0x33u8; 32];
        let cookie_b = [0x44u8; 32];
        fs::write(&cookie_path, cookie_a).unwrap();

        let (server, mut add_onion_rx, _failed_auth_rx) =
            MockControlServer::spawn(cookie_a.to_hex(), cookie_path.clone()).await;

        let shutdown = Shutdown::new();
        let ctl = build_test_controller(server.addr.clone(), Authentication::Auto, shutdown.to_signal());

        let hs = ctl.create_hidden_service().await.unwrap();
        assert_eq!(hs.service_id(), TEST_SERVICE_ID);
        let _initial = time::timeout(Duration::from_secs(5), add_onion_rx.recv())
            .await
            .unwrap()
            .unwrap();

        fs::write(&cookie_path, cookie_b).unwrap();
        server.set_expected_cookie(&cookie_b).await;
        server.disconnect_active_sessions().await;

        let reestablished = time::timeout(Duration::from_secs(5), add_onion_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            reestablished,
            format!("ADD_ONION ED25519-V3:{TEST_ED25519_KEY} Port=18141,127.0.0.1:18142")
        );

        shutdown.trigger();
    }
}
