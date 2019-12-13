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

use crate::{
    connection::ConnectionDirection,
    noise::{
        crypto_resolver::TariCryptoResolver,
        error::NoiseError,
        socket::{Handshake, NoiseSocket},
    },
    peer_manager::NodeIdentity,
    types::CommsPublicKey,
};
use futures::{AsyncRead, AsyncWrite};
use snow::{self, params::NoiseParams};
use std::sync::Arc;
use tari_utilities::ByteArray;

pub(super) const NOISE_IX_PARAMETER: &str = "Noise_IX_25519_ChaChaPoly_BLAKE2b";

/// The Noise protocol configuration to be used to perform a protocol upgrade on an underlying
/// socket.
#[derive(Clone, Debug)]
pub struct NoiseConfig {
    node_identity: Arc<NodeIdentity>,
    parameters: NoiseParams,
}

impl NoiseConfig {
    /// Create a new NoiseConfig with the provided keypair
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        let parameters: NoiseParams = NOISE_IX_PARAMETER.parse().expect("Invalid noise parameters");
        Self {
            node_identity,
            parameters,
        }
    }

    /// Upgrades the given socket to using the noise protocol. The upgraded socket and the peer's static key
    /// is returned.
    pub async fn upgrade_socket<TSocket>(
        &self,
        socket: TSocket,
        direction: ConnectionDirection,
    ) -> Result<(CommsPublicKey, NoiseSocket<TSocket>), NoiseError>
    where
        TSocket: AsyncWrite + AsyncRead + Unpin,
    {
        let builder = snow::Builder::with_resolver(self.parameters.clone(), Box::new(TariCryptoResolver::default()))
            .local_private_key(self.node_identity.secret_key().as_bytes());

        let handshake_state = match direction {
            ConnectionDirection::Outbound => builder.build_initiator()?,
            ConnectionDirection::Inbound => builder.build_responder()?,
        };

        let handshake = Handshake::new(socket, handshake_state);
        let socket = handshake.handshake_1rt().await.map_err(NoiseError::HandshakeFailed)?;
        let static_key = socket
            .get_remote_static()
            .ok_or(NoiseError::PeerPublicStaticKeyUnknown)?;
        let comms_pk = CommsPublicKey::from_bytes(static_key).map_err(NoiseError::InvalidCommsPublicKey)?;

        Ok((comms_pk, socket))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        peer_manager::PeerFeatures,
        test_utils::{node_identity::build_node_identity, tcp::build_connected_tcp_socket_pair},
    };
    use futures::{future, AsyncReadExt, AsyncWriteExt};
    use snow::params::{BaseChoice, CipherChoice, DHChoice, HandshakePattern, HashChoice};
    use tokio::runtime::Runtime;

    fn check_noise_params(config: &NoiseConfig) {
        assert_eq!(config.parameters.hash, HashChoice::Blake2b);
        assert_eq!(config.parameters.name, NOISE_IX_PARAMETER);
        assert_eq!(config.parameters.cipher, CipherChoice::ChaChaPoly);
        assert_eq!(config.parameters.base, BaseChoice::Noise);
        assert_eq!(config.parameters.dh, DHChoice::Curve25519);
        assert_eq!(config.parameters.handshake.pattern, HandshakePattern::IX);
    }

    #[test]
    fn new() {
        let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let config = NoiseConfig::new(node_identity.clone());
        check_noise_params(&config);
        assert_eq!(config.node_identity.public_key(), node_identity.public_key());
    }

    #[test]
    fn upgrade_socket() {
        let rt = Runtime::new().unwrap();

        let node_identity1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let config1 = NoiseConfig::new(node_identity1.clone());

        let node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let config2 = NoiseConfig::new(node_identity2.clone());

        rt.block_on(async move {
            let (in_socket, out_socket) = build_connected_tcp_socket_pair().await;
            let (upgraded_in, upgraded_out) = future::join(
                config1.upgrade_socket(in_socket, ConnectionDirection::Inbound),
                config2.upgrade_socket(out_socket, ConnectionDirection::Outbound),
            )
            .await;

            let (in_pubkey, mut socket_in) = upgraded_in.unwrap();
            let (out_pubkey, mut socket_out) = upgraded_out.unwrap();

            assert_eq!(&in_pubkey, node_identity2.public_key());
            assert_eq!(&out_pubkey, node_identity1.public_key());

            let sample = b"Children of time";
            socket_in.write_all(sample).await.unwrap();
            socket_in.flush().await.unwrap();
            socket_in.close().await.unwrap();

            let mut read_buf = Vec::with_capacity(16);
            socket_out.read_to_end(&mut read_buf).await.unwrap();
            assert_eq!(read_buf, sample);
        });
    }
}
