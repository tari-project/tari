// Copyright 2019, The Tari Project
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

// This file is heavily influenced by the Libra Noise protocol implementation.

use std::{sync::Arc, time::Duration};

use log::*;
use snow::params::NoiseParams;
use tari_utilities::ByteArray;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    connection_manager::ConnectionDirection,
    noise::{
        crypto_resolver::TariCryptoResolver,
        error::NoiseError,
        socket::{Handshake, NoiseSocket},
    },
    peer_manager::NodeIdentity,
};

const LOG_TARGET: &str = "comms::noise";
pub(super) const NOISE_PARAMETERS: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2b";

/// The Noise protocol configuration to be used to perform a protocol upgrade on an underlying
/// socket.
#[derive(Clone, Debug)]
pub struct NoiseConfig {
    node_identity: Arc<NodeIdentity>,
    parameters: NoiseParams,
    recv_timeout: Duration,
}

impl NoiseConfig {
    /// Create a new NoiseConfig with the provided keypair
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        let parameters: NoiseParams = NOISE_PARAMETERS.parse().expect("Invalid noise parameters");
        Self {
            node_identity,
            parameters,
            recv_timeout: Duration::from_secs(3),
        }
    }

    /// Sets a custom receive timeout when waiting for handshake responses.
    pub fn with_recv_timeout(mut self, recv_timeout: Duration) -> Self {
        self.recv_timeout = recv_timeout;
        self
    }

    /// Upgrades the given socket to using the noise protocol. The upgraded socket and the peer's static key
    /// is returned.
    pub async fn upgrade_socket<TSocket>(
        &self,
        socket: TSocket,
        direction: ConnectionDirection,
    ) -> Result<NoiseSocket<TSocket>, NoiseError>
    where
        TSocket: AsyncWrite + AsyncRead + Unpin,
    {
        const TARI_PROLOGUE: &[u8] = b"com.tari.comms.noise.prologue";

        let handshake_state = {
            let builder = snow::Builder::with_resolver(self.parameters.clone(), Box::<TariCryptoResolver>::default())
                .prologue(TARI_PROLOGUE)
                .local_private_key(self.node_identity.secret_key().as_bytes());

            match direction {
                ConnectionDirection::Outbound => {
                    trace!(target: LOG_TARGET, "Starting noise initiator handshake ");
                    builder.build_initiator()?
                },
                ConnectionDirection::Inbound => {
                    trace!(target: LOG_TARGET, "Starting noise responder handshake");
                    builder.build_responder()?
                },
            }
        };

        let handshake = Handshake::new(socket, handshake_state, self.recv_timeout);
        let socket = handshake
            .perform_handshake()
            .await
            .map_err(NoiseError::HandshakeFailed)?;

        Ok(socket)
    }
}

#[cfg(test)]
mod test {
    use futures::{future, FutureExt};
    use snow::params::{BaseChoice, CipherChoice, DHChoice, HandshakePattern, HashChoice};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::{memsocket::MemorySocket, peer_manager::PeerFeatures, test_utils::node_identity::build_node_identity};

    fn check_noise_params(config: &NoiseConfig) {
        assert_eq!(config.parameters.hash, HashChoice::Blake2b);
        assert_eq!(config.parameters.name, NOISE_PARAMETERS);
        assert_eq!(config.parameters.cipher, CipherChoice::ChaChaPoly);
        assert_eq!(config.parameters.base, BaseChoice::Noise);
        assert_eq!(config.parameters.dh, DHChoice::Curve25519);
        assert_eq!(config.parameters.handshake.pattern, HandshakePattern::XX);
    }

    #[test]
    fn new() {
        let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let config = NoiseConfig::new(node_identity.clone());
        check_noise_params(&config);
        assert_eq!(config.node_identity.public_key(), node_identity.public_key());
    }

    #[tokio::test]
    async fn upgrade_socket() {
        let node_identity1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let config1 = NoiseConfig::new(node_identity1.clone());

        let node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let config2 = NoiseConfig::new(node_identity2.clone());

        let (in_socket, out_socket) = MemorySocket::new_pair();
        let (mut socket_in, mut socket_out) = future::join(
            config1.upgrade_socket(in_socket, ConnectionDirection::Inbound),
            config2.upgrade_socket(out_socket, ConnectionDirection::Outbound),
        )
        .map(|(s1, s2)| (s1.unwrap(), s2.unwrap()))
        .await;

        let in_pubkey = socket_in.get_remote_public_key().unwrap();
        let out_pubkey = socket_out.get_remote_public_key().unwrap();

        assert_eq!(&in_pubkey, node_identity2.public_key());
        assert_eq!(&out_pubkey, node_identity1.public_key());

        let sample = b"Children of time";
        socket_in.write_all(sample).await.unwrap();
        socket_in.flush().await.unwrap();
        socket_in.shutdown().await.unwrap();

        let mut read_buf = Vec::with_capacity(16);
        socket_out.read_to_end(&mut read_buf).await.unwrap();
        assert_eq!(read_buf, sample);
    }
}
