//  Copyright 2019 The Tari Project
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

use super::{TestFactory, TestFactoryError};

use futures::channel::mpsc::Sender;
use multiaddr::Multiaddr;
use tari_comms::{
    connection::{
        peer_connection::PeerConnectionContext,
        types::Linger,
        zmq::ZmqIdentity,
        CurveEncryption,
        CurvePublicKey,
        CurveSecretKey,
        Direction,
        PeerConnectionContextBuilder,
        ZmqContext,
    },
    message::FrameSet,
};

pub fn create<'c>() -> PeerConnectionContextFactory<'c> {
    PeerConnectionContextFactory::default()
}

#[derive(Default)]
pub struct PeerConnectionContextFactory<'c> {
    direction: Option<Direction>,
    context: Option<&'c ZmqContext>,
    connection_identity: Option<ZmqIdentity>,
    peer_identity: Option<ZmqIdentity>,
    message_sink_channel: Option<Sender<FrameSet>>,
    server_public_key: Option<CurvePublicKey>,
    curve_keypair: Option<(CurveSecretKey, CurvePublicKey)>,
    address: Option<Multiaddr>,
    linger: Option<Linger>,
}

impl<'c> PeerConnectionContextFactory<'c> {
    factory_setter!(with_direction, direction, Option<Direction>);

    factory_setter!(with_connection_identity, connection_identity, Option<ZmqIdentity>);

    factory_setter!(with_peer_identity, peer_identity, Option<ZmqIdentity>);

    factory_setter!(
        with_message_sink_channel,
        message_sink_channel,
        Option<Sender<FrameSet>>
    );

    factory_setter!(with_server_public_key, server_public_key, Option<CurvePublicKey>);

    factory_setter!(
        with_curve_keypair,
        curve_keypair,
        Option<(CurveSecretKey, CurvePublicKey)>
    );

    factory_setter!(with_address, address, Option<Multiaddr>);

    factory_setter!(with_linger, linger, Option<Linger>);

    pub fn with_context(mut self, context: &'c ZmqContext) -> Self {
        self.context = Some(context);
        self
    }
}

impl<'c> TestFactory for PeerConnectionContextFactory<'c> {
    type Object = PeerConnectionContext;

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        let context = self.context.ok_or(TestFactoryError::BuildFailed(
            "Context must be set for PeerConnectionContextFactory".into(),
        ))?;

        let direction = self.direction.ok_or(TestFactoryError::BuildFailed(
            "Must set direction on PeerConnectionContextFactory".into(),
        ))?;

        let address = self.address.or(Some("/ip4/127.0.0.1/tcp/0".parse().unwrap())).unwrap();

        let mut builder = PeerConnectionContextBuilder::new()
            .set_linger(self.linger.or(Some(Linger::Indefinitely)).unwrap())
            .set_direction(direction.clone())
            .set_context(context)
            .set_address(address)
            .set_message_sink_channel(self.message_sink_channel.unwrap());

        if let Some(connection_identity) = self.connection_identity {
            builder = builder.set_connection_identity(connection_identity)
        }
        if let Some(peer_identity) = self.peer_identity {
            builder = builder.set_peer_identity(peer_identity);
        }

        let (secret_key, public_key) = self
            .curve_keypair
            .unwrap_or(CurveEncryption::generate_keypair().map_err(TestFactoryError::build_failed())?);

        match direction {
            Direction::Inbound => {
                builder = builder.set_curve_encryption(CurveEncryption::Server {
                    secret_key: secret_key.clone(),
                });
            },
            Direction::Outbound => {
                let server_public_key = self.server_public_key.or(Some(public_key.clone())).unwrap();
                builder = builder.set_curve_encryption(CurveEncryption::Client {
                    secret_key: secret_key.clone(),
                    public_key: public_key.clone(),
                    server_public_key,
                });
            },
        }

        let peer_context = builder.finish().map_err(TestFactoryError::build_failed())?;

        Ok(peer_context)
    }
}
