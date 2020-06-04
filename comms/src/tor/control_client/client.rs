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

use super::{
    commands,
    commands::{AddOnionFlag, AddOnionResponse, TorCommand},
    error::TorClientError,
    response::ResponseLine,
    types::{KeyBlob, KeyType, PortMapping},
    PrivateKey,
    LOG_TARGET,
};
use crate::{
    multiaddr::Multiaddr,
    tor::control_client::{event::TorControlEvent, monitor::spawn_monitor},
    transports::{TcpTransport, Transport},
};
use futures::{channel::mpsc, AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use std::{borrow::Cow, fmt, fmt::Display, num::NonZeroU16};
use tokio::sync::broadcast;

/// Client for the Tor control port.
///
/// See the [Tor Control Port Spec](https://gitweb.torproject.org/torspec.git/tree/control-spec.txt) for more details.
pub struct TorControlPortClient {
    cmd_tx: mpsc::Sender<String>,
    output_stream: mpsc::Receiver<ResponseLine>,
    event_tx: broadcast::Sender<TorControlEvent>,
}

impl TorControlPortClient {
    /// Connect using TCP to the given address.
    pub async fn connect(
        addr: Multiaddr,
        event_tx: broadcast::Sender<TorControlEvent>,
    ) -> Result<Self, TorClientError>
    {
        let mut tcp = TcpTransport::new();
        tcp.set_nodelay(true);
        let socket = tcp.dial(addr)?.await?;
        Ok(Self::new(socket, event_tx))
    }

    /// Create a new TorControlPortClient using the given socket
    pub fn new<TSocket>(socket: TSocket, event_tx: broadcast::Sender<TorControlEvent>) -> Self
    where TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static {
        let (cmd_tx, cmd_rx) = mpsc::channel(10);
        let output_stream = spawn_monitor(cmd_rx, socket, event_tx.clone());
        Self {
            cmd_tx,
            output_stream,
            event_tx,
        }
    }

    pub fn is_connected(&self) -> bool {
        !self.cmd_tx.is_closed()
    }

    pub(in crate::tor) fn event_sender(&self) -> &broadcast::Sender<TorControlEvent> {
        &self.event_tx
    }

    pub fn get_event_stream(&self) -> broadcast::Receiver<TorControlEvent> {
        self.event_tx.subscribe()
    }

    /// Authenticate with the tor control port
    pub async fn authenticate(&mut self, authentication: &Authentication) -> Result<(), TorClientError> {
        match authentication {
            Authentication::None => {
                self.send_line("AUTHENTICATE".to_string()).await?;
            },
            Authentication::HashedPassword(passwd) => {
                self.send_line(format!("AUTHENTICATE \"{}\"", passwd.replace("\"", "\\\"")))
                    .await?;
            },
            Authentication::Cookie(cookie) => {
                self.send_line(format!("AUTHENTICATE {}", cookie)).await?;
            },
        }

        self.recv_ok().await?;

        Ok(())
    }

    /// The GETCONF command. Returns configuration keys matching the `conf_name`.
    #[allow(clippy::needless_lifetimes)]
    pub async fn get_conf<'a>(&mut self, conf_name: &'a str) -> Result<Vec<Cow<'a, str>>, TorClientError> {
        let command = commands::get_conf(conf_name);
        self.request_response(command).await
    }

    /// The GETINFO command. Returns configuration keys matching the `conf_name`.
    #[allow(clippy::needless_lifetimes)]
    pub async fn get_info<'a>(&mut self, key_name: &'a str) -> Result<Vec<Cow<'a, str>>, TorClientError> {
        let command = commands::get_info(key_name);
        let response = self.request_response(command).await?;
        if response.is_empty() {
            return Err(TorClientError::ServerNoResponse);
        }
        Ok(response)
    }

    /// The SETEVENTS command.
    pub async fn set_events(&mut self, events: &[&str]) -> Result<(), TorClientError> {
        let command = commands::set_events(events);
        let _ = self.request_response(command).await?;
        Ok(())
    }

    /// The ADD_ONION command, used to create onion hidden services.
    pub async fn add_onion_custom<P: Into<PortMapping>>(
        &mut self,
        key_type: KeyType,
        key_blob: KeyBlob<'_>,
        flags: Vec<AddOnionFlag>,
        port: P,
        num_streams: Option<NonZeroU16>,
    ) -> Result<AddOnionResponse, TorClientError>
    {
        let command = commands::AddOnion::new(key_type, key_blob, flags, port.into(), num_streams);
        self.request_response(command).await
    }

    /// The ADD_ONION command using a v2 key
    pub async fn add_onion_v2<P: Into<PortMapping>>(
        &mut self,
        flags: Vec<AddOnionFlag>,
        port: P,
        num_streams: Option<NonZeroU16>,
    ) -> Result<AddOnionResponse, TorClientError>
    {
        self.add_onion_custom(KeyType::New, KeyBlob::Rsa1024, flags, port, num_streams)
            .await
    }

    /// The ADD_ONION command using the 'best' key. The 'best' key is determined by the tor proxy. At the time of
    /// writing tor will select a Ed25519 key.
    pub async fn add_onion<P: Into<PortMapping>>(
        &mut self,
        flags: Vec<AddOnionFlag>,
        port: P,
        num_streams: Option<NonZeroU16>,
    ) -> Result<AddOnionResponse, TorClientError>
    {
        self.add_onion_custom(KeyType::New, KeyBlob::Best, flags, port, num_streams)
            .await
    }

    /// The ADD_ONION command using the given `PrivateKey`.
    pub async fn add_onion_from_private_key<P: Into<PortMapping>>(
        &mut self,
        private_key: &PrivateKey,
        flags: Vec<AddOnionFlag>,
        port: P,
        num_streams: Option<NonZeroU16>,
    ) -> Result<AddOnionResponse, TorClientError>
    {
        let (key_type, key_blob) = match private_key {
            PrivateKey::Rsa1024(key) => (KeyType::Rsa1024, KeyBlob::String(key)),
            PrivateKey::Ed25519V3(key) => (KeyType::Ed25519V3, KeyBlob::String(key)),
        };
        self.add_onion_custom(key_type, key_blob, flags, port, num_streams)
            .await
    }

    /// The DEL_ONION command.
    pub async fn del_onion(&mut self, service_id: &str) -> Result<(), TorClientError> {
        let command = commands::DelOnion::new(service_id);
        self.request_response(command).await
    }

    async fn request_response<T: TorCommand + Display>(&mut self, command: T) -> Result<T::Output, TorClientError>
    where T::Error: Into<TorClientError> {
        trace!(target: LOG_TARGET, "Sent command: {}", command);
        let cmd_str = command.to_command_string().map_err(Into::into)?;
        self.send_line(cmd_str).await?;
        let responses = self.recv_next_responses().await?;
        trace!(target: LOG_TARGET, "Response from tor: {:?}", responses);
        if responses.is_empty() {
            return Err(TorClientError::ServerNoResponse);
        }
        let response = command.parse_responses(responses).map_err(Into::into)?;
        Ok(response)
    }

    async fn send_line(&mut self, line: String) -> Result<(), TorClientError> {
        self.cmd_tx
            .send(line)
            .await
            .map_err(|_| TorClientError::CommandSenderDisconnected)
    }

    async fn recv_ok(&mut self) -> Result<(), TorClientError> {
        let resp = self.receive_line().await?;
        if resp.is_ok() {
            Ok(())
        } else {
            Err(TorClientError::TorCommandFailed(resp.value))
        }
    }

    async fn recv_next_responses(&mut self) -> Result<Vec<ResponseLine>, TorClientError> {
        let mut msgs = Vec::new();
        loop {
            let msg = self.receive_line().await?;
            let has_more = msg.has_more();
            msgs.push(msg.into_owned());
            if !has_more {
                break;
            }
        }

        Ok(msgs)
    }

    async fn receive_line(&mut self) -> Result<ResponseLine, TorClientError> {
        let line = self
            .output_stream
            .next()
            .await
            .ok_or_else(|| TorClientError::UnexpectedEof)?;

        Ok(line)
    }
}

/// Represents tor control port authentication mechanisms
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authentication {
    /// No control port authentication required
    None,
    /// A hashed password will be sent to authenticate
    HashedPassword(String),
    /// Cookie authentication. The contents of the cookie file encoded as hex
    Cookie(String),
}

impl Default for Authentication {
    fn default() -> Self {
        Authentication::None
    }
}

impl fmt::Display for Authentication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Authentication::*;
        match self {
            None => write!(f, "None"),
            HashedPassword(_) => write!(f, "HashedPassword"),
            Cookie(_) => write!(f, "Cookie"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        runtime,
        tor::control_client::{test_server, test_server::canned_responses, types::PrivateKey},
    };
    use futures::future;
    use std::net::SocketAddr;
    use tari_test_utils::unpack_enum;

    async fn setup_test() -> (TorControlPortClient, test_server::State) {
        let (_, mock_state, socket) = test_server::spawn().await;
        let (event_tx, _) = broadcast::channel(1);
        let tor = TorControlPortClient::new(socket, event_tx);
        (tor, mock_state)
    }

    #[runtime::test]
    async fn connect() {
        let (mut listener, addr) = TcpTransport::default()
            .listen("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap()
            .await
            .unwrap();
        let (event_tx, _) = broadcast::channel(1);
        let (result_out, result_in) =
            future::join(TorControlPortClient::connect(addr, event_tx), listener.next()).await;

        // Check that the connection is successfully made
        result_out.unwrap();
        result_in.unwrap().unwrap().0.await.unwrap();
    }

    #[runtime::test]
    async fn authenticate() {
        let (mut tor, mock_state) = setup_test().await;

        tor.authenticate(&Authentication::None).await.unwrap();
        let mut req = mock_state.take_requests().await;
        assert_eq!(req.len(), 1);
        assert_eq!(req.remove(0), "AUTHENTICATE");

        tor.authenticate(&Authentication::HashedPassword("ab\"cde".to_string()))
            .await
            .unwrap();
        let mut req = mock_state.take_requests().await;
        assert_eq!(req.len(), 1);
        assert_eq!(req.remove(0), "AUTHENTICATE \"ab\\\"cde\"");

        tor.authenticate(&Authentication::Cookie("NOTACTUALLYHEXENCODED".to_string()))
            .await
            .unwrap();
        let mut req = mock_state.take_requests().await;
        assert_eq!(req.len(), 1);
        assert_eq!(req.remove(0), "AUTHENTICATE NOTACTUALLYHEXENCODED");
    }

    #[runtime::test]
    async fn get_conf_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state
            .set_canned_response(canned_responses::GET_CONF_HIDDEN_SERVICE_PORT_OK)
            .await;

        let results = tor.get_conf("HiddenServicePort").await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], "8080");
        assert_eq!(results[1], "8081 127.0.0.1:9000");
        assert_eq!(results[2], "8082 127.0.0.1:9001");
    }

    #[runtime::test]
    async fn get_conf_err() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ERR_552).await;

        let err = tor.get_conf("HiddenServicePort").await.unwrap_err();
        unpack_enum!(TorClientError::TorCommandFailed(_s) = err);
    }

    #[runtime::test]
    async fn get_info_multiline_kv_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state
            .set_canned_response(canned_responses::GET_INFO_NET_LISTENERS_OK)
            .await;

        let values = tor.get_info("net/listeners/socks").await.unwrap();
        assert_eq!(values, &["127.0.0.1:9050", "unix:/run/tor/socks"]);
    }

    #[runtime::test]
    async fn get_info_kv_multiline_value_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state
            .set_canned_response(canned_responses::GET_INFO_ONIONS_DETACHED_OK)
            .await;

        let values = tor.get_info("onions/detached").await.unwrap();
        assert_eq!(values, [
            "mochz2xppfziim5olr5f6q27poc4vfob2xxxxxxxxxxxxxxxxxxxxxxx",
            "nhqdqym6j35rk7tdou4cdj4gjjqagimutxxxxxxxxxxxxxxxxxxxxxxx"
        ]);
    }

    #[runtime::test]
    async fn get_info_err() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ERR_552).await;

        let err = tor.get_info("net/listeners/socks").await.unwrap_err();
        unpack_enum!(TorClientError::TorCommandFailed(_s) = err);
    }

    #[runtime::test]
    async fn add_onion_from_private_key_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state
            .set_canned_response(canned_responses::ADD_ONION_RSA1024_OK)
            .await;

        let private_key = PrivateKey::Rsa1024("dummy-key".into());
        let response = tor
            .add_onion_from_private_key(&private_key, vec![], 8080, None)
            .await
            .unwrap();

        assert_eq!(response.service_id, "62q4tswkxp74dtn7");
        assert!(response.private_key.is_none());

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(request, "ADD_ONION RSA1024:dummy-key Port=8080,127.0.0.1:8080");
    }

    #[runtime::test]
    async fn add_onion_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ADD_ONION_OK).await;

        let response = tor
            .add_onion_custom(
                KeyType::New,
                KeyBlob::Best,
                vec![],
                8080,
                Some(NonZeroU16::new(10u16).unwrap()),
            )
            .await
            .unwrap();

        assert_eq!(
            response.service_id,
            "qigbgbs4ue3ghbupsotgh73cmmkjrin2aprlyxsrnrvpmcmzy3g4wbid"
        );
        assert_eq!(
            response.private_key,
            Some(PrivateKey::Ed25519V3(
                "Pg3GEyssauPRW3jP6mHwKOxvl_fMsF0QsZC3DvQ8jZ9AxmfRvSP35m9l0vOYyOxkOqWM6ufjdYuM8Ae6cR2UdreG6".to_string()
            ))
        );

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(request, "ADD_ONION NEW:BEST NumStreams=10 Port=8080,127.0.0.1:8080");
    }

    #[runtime::test]
    async fn add_onion_discard_pk_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state
            .set_canned_response(canned_responses::ADD_ONION_DISCARDPK_OK)
            .await;

        let response = tor
            .add_onion_custom(
                KeyType::Rsa1024,
                KeyBlob::Rsa1024,
                vec![
                    AddOnionFlag::DiscardPK,
                    AddOnionFlag::Detach,
                    AddOnionFlag::BasicAuth,
                    AddOnionFlag::MaxStreamsCloseCircuit,
                    AddOnionFlag::NonAnonymous,
                ],
                PortMapping::new(8080, SocketAddr::from(([127u8, 0, 0, 1], 8081u16))),
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            response.service_id,
            "qigbgbs4ue3ghbupsotgh73cmmkjrin2aprlyxsrnrvpmcmzy3g4wbid"
        );
        assert_eq!(response.private_key, None);

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(
            request,
            "ADD_ONION RSA1024:RSA1024 Flags=DiscardPK,Detach,BasicAuth,MaxStreamsCloseCircuit,NonAnonymous \
             Port=8080,127.0.0.1:8081"
        );
    }

    #[runtime::test]
    async fn add_onion_err() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ERR_552).await;

        let err = tor
            .add_onion_custom(KeyType::Ed25519V3, KeyBlob::Ed25519V3, vec![], 8080, None)
            .await
            .unwrap_err();

        unpack_enum!(TorClientError::TorCommandFailed(_s) = err);
    }

    #[runtime::test]
    async fn del_onion_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::OK).await;

        tor.del_onion("some-fake-id").await.unwrap();

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(request, "DEL_ONION some-fake-id");
    }

    #[runtime::test]
    async fn del_onion_err() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ERR_552).await;

        tor.del_onion("some-fake-id").await.unwrap_err();

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(request, "DEL_ONION some-fake-id");
    }
}
