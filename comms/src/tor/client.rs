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
    commands::AddOnionFlag,
    error::TorClientError,
    types::{KeyBlob, KeyType},
};
use crate::{
    compat::IoCompat,
    multiaddr::Multiaddr,
    tor::{
        commands,
        commands::{AddOnionResponse, TorCommand},
        parsers,
        response::{ResponseLine, EVENT_CODE},
    },
    transports::{TcpTransport, Transport},
};
use futures::{AsyncRead, AsyncWrite, SinkExt, StreamExt};
use std::{borrow::Cow, io, net::SocketAddr, num::NonZeroU16};
use tokio_util::codec::{Framed, LinesCodec};

/// Client for the Tor control port.
///
/// See the [Tor Control Port Spec](https://gitweb.torproject.org/torspec.git/tree/control-spec.txt) for more details.
pub struct TorControlPortClient<TSocket> {
    framed: Framed<IoCompat<TSocket>, LinesCodec>,
}

impl TorControlPortClient<<TcpTransport as Transport>::Output> {
    /// Connect using TCP to the given address.
    pub async fn connect(addr: Multiaddr) -> Result<Self, io::Error> {
        let mut tcp = TcpTransport::new();
        tcp.set_nodelay(true);
        let socket = tcp.dial(addr).await?;
        Ok(Self::new(socket))
    }
}

/// Represents tor control port authentication mechanisms
pub enum Authentication {
    /// No control port authentication required
    None,
    /// A hashed password will be sent to authenticate
    HashedPassword(String),
}

impl<TSocket> TorControlPortClient<TSocket>
where TSocket: AsyncRead + AsyncWrite + Unpin
{
    /// Create a new TorControlPortClient using the given socket
    pub fn new(socket: TSocket) -> Self {
        Self {
            framed: Framed::new(IoCompat::new(socket), LinesCodec::new()),
        }
    }

    /// Authenticate with the tor control port
    pub async fn authenticate(&mut self, authentication: Authentication) -> Result<(), TorClientError> {
        match authentication {
            Authentication::None => {
                self.send_line("AUTHENTICATE".to_string()).await?;
            },
            Authentication::HashedPassword(passwd) => {
                self.send_line(format!("AUTHENTICATE \"{}\"", passwd.replace("\"", "\\\"")))
                    .await?;
            },
        }

        self.recv_ok().await?;

        Ok(())
    }

    /// The GET_CONF command. Returns configuration keys matching the `conf_name`.
    pub async fn get_conf(&mut self, conf_name: &str) -> Result<Vec<Cow<'_, str>>, TorClientError> {
        let command = commands::GetConf::new(conf_name);
        self.request_response(command).await
    }

    /// The ADD_ONION command. Used to create onion hidden services.
    pub async fn add_onion(
        &mut self,
        key_type: KeyType,
        key_blob: KeyBlob,
        flags: Vec<AddOnionFlag>,
        port: (u16, Option<SocketAddr>),
        num_streams: Option<NonZeroU16>,
    ) -> Result<AddOnionResponse<'_>, TorClientError>
    {
        let command = commands::AddOnion::new(key_type, key_blob, flags, port, num_streams);
        self.request_response(command).await
    }

    pub async fn del_onion(&mut self, service_id: &str) -> Result<(), TorClientError> {
        let command = commands::DelOnion::new(service_id);
        self.request_response(command).await
    }

    async fn request_response<T: TorCommand>(&mut self, command: T) -> Result<T::Output, TorClientError>
    where T::Error: Into<TorClientError> {
        self.send_line(command.to_command_string().map_err(Into::into)?).await?;
        let responses = self.recv_next_responses().await?;
        if responses.len() == 0 {
            return Err(TorClientError::ServerNoResponse);
        }
        let response = command.parse_responses(responses).map_err(Into::into)?;
        Ok(response)
    }

    async fn send_line(&mut self, line: String) -> Result<(), TorClientError> {
        self.framed.send(line).await.map_err(Into::into)
    }

    async fn recv_ok(&mut self) -> Result<(), TorClientError> {
        let resp = self.receive_line().await?;
        if resp.is_ok() {
            Ok(())
        } else {
            Err(TorClientError::TorCommandFailed(resp.value.into_owned()))
        }
    }

    async fn recv_next_responses(&mut self) -> Result<Vec<ResponseLine<'_>>, TorClientError> {
        let mut msgs = Vec::new();
        loop {
            let msg = self.receive_line().await?;
            // Ignore event codes (for now)
            if msg.code == EVENT_CODE {
                continue;
            }
            let has_more = msg.has_more();
            msgs.push(msg.into_owned());
            if !has_more {
                break;
            }
        }

        Ok(msgs)
    }

    async fn receive_line(&mut self) -> Result<ResponseLine<'_>, TorClientError> {
        let raw = self.framed.next().await.ok_or(TorClientError::UnexpectedEof)??;
        let parsed =
            parsers::response_line(&raw).map_err(|err| TorClientError::ParseFailedResponse(err.to_string()))?;
        Ok(parsed.into_owned())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        memsocket::MemorySocket,
        tor::{test_server, test_server::canned_responses, types::PrivateKey},
    };
    use futures::future;
    use std::borrow::Cow;
    use tari_test_utils::unpack_enum;

    async fn setup_test() -> (TorControlPortClient<MemorySocket>, test_server::State) {
        let (_, mock_state, socket) = test_server::spawn().await;
        let tor = TorControlPortClient::new(socket);
        (tor, mock_state)
    }

    #[tokio_macros::test]
    async fn connect() {
        let (mut listener, addr) = TcpTransport::default()
            .listen("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .await
            .unwrap();
        let (result_out, result_in) = future::join(TorControlPortClient::connect(addr), listener.next()).await;

        // Check that the connection is successfully made
        result_out.unwrap();
        result_in.unwrap().unwrap().0.await.unwrap();
    }

    #[tokio_macros::test]
    async fn authenticate_none() {
        let (mut tor, mock_state) = setup_test().await;

        tor.authenticate(Authentication::None).await.unwrap();
        let mut req = mock_state.take_requests().await;
        assert_eq!(req.len(), 1);
        assert_eq!(req.remove(0), "AUTHENTICATE");

        tor.authenticate(Authentication::HashedPassword("ab\"cde".to_string()))
            .await
            .unwrap();
        let mut req = mock_state.take_requests().await;
        assert_eq!(req.len(), 1);
        assert_eq!(req.remove(0), "AUTHENTICATE \"ab\\\"cde\"");
    }

    #[tokio_macros::test]
    async fn get_conf_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::GET_CONF_OK).await;

        let results = tor.get_conf("HiddenServicePort").await.unwrap();
        assert_eq!(results[0], "8080");
        assert_eq!(results[1], "8081 127.0.0.1:9000");
        assert_eq!(results[2], "8082 127.0.0.1:9001");
    }

    #[tokio_macros::test]
    async fn get_conf_err() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ERR_552).await;

        let err = tor.get_conf("HiddenServicePort").await.unwrap_err();
        unpack_enum!(TorClientError::TorCommandFailed(_s) = err);
    }

    #[tokio_macros::test]
    async fn add_onion_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ADD_ONION_OK).await;

        let response = tor
            .add_onion(
                KeyType::New,
                KeyBlob::Best,
                vec![],
                (8080, None),
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
            Some(PrivateKey::Ed25519V3(Cow::from(
                "Pg3GEyssauPRW3jP6mHwKOxvl_fMsF0QsZC3DvQ8jZ9AxmfRvSP35m9l0vOYyOxkOqWM6ufjdYuM8Ae6cR2UdreG6"
            )))
        );

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(request, "ADD_ONION NEW:BEST NumStreams=10 Port=8080");
    }

    #[tokio_macros::test]
    async fn add_onion_discard_pk_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state
            .set_canned_response(canned_responses::ADD_ONION_DISCARDPK_OK)
            .await;

        let response = tor
            .add_onion(
                KeyType::Rsa1024,
                KeyBlob::Rsa1024,
                vec![
                    AddOnionFlag::DiscardPK,
                    AddOnionFlag::Detach,
                    AddOnionFlag::BasicAuth,
                    AddOnionFlag::MaxStreamsCloseCircuit,
                    AddOnionFlag::NonAnonymous,
                ],
                (8080, Some(([127u8, 0, 0, 1], 8081u16).into())),
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            response.service_id,
            "qigbgbs4ue3ghbupsotgh73cmmkjrin2aprlyxsrnrvpmcmzy3g4wbid"
        );
        assert_eq!(response.private_key, None,);

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(
            request,
            "ADD_ONION RSA1024:RSA1024 Flags=DiscardPK,Detach,BasicAuth,MaxStreamsCloseCircuit,NonAnonymous \
             Port=8080,127.0.0.1:8081"
        );
    }

    #[tokio_macros::test]
    async fn add_onion_err() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ERR_552).await;

        let err = tor
            .add_onion(KeyType::Ed25519V3, KeyBlob::Ed25519V3, vec![], (8080, None), None)
            .await
            .unwrap_err();

        unpack_enum!(TorClientError::TorCommandFailed(_s) = err);
    }

    #[tokio_macros::test]
    async fn del_onion_ok() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::OK).await;

        tor.del_onion("some-fake-id").await.unwrap();

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(request, "DEL_ONION some-fake-id");
    }

    #[tokio_macros::test]
    async fn del_onion_err() {
        let (mut tor, mock_state) = setup_test().await;

        mock_state.set_canned_response(canned_responses::ERR_552).await;

        tor.del_onion("some-fake-id").await.unwrap_err();

        let request = mock_state.take_requests().await.pop().unwrap();
        assert_eq!(request, "DEL_ONION some-fake-id");
    }
}
