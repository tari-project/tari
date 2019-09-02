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

use rand::{OsRng, Rng};
use tari_comms::connection::{
    peer_connection::{ConnectionId, PeerConnectionContext},
    types::Linger,
    CurveEncryption,
    CurvePublicKey,
    CurveSecretKey,
    Direction,
    InprocAddress,
    NetAddress,
    PeerConnectionContextBuilder,
    ZmqContext,
};

pub fn create<'c>() -> PeerConnectionContextFactory<'c> {
    PeerConnectionContextFactory::default()
}

#[derive(Default)]
pub struct PeerConnectionContextFactory<'c> {
    direction: Option<Direction>,
    context: Option<&'c ZmqContext>,
    connection_id: Option<ConnectionId>,
    message_sink_address: Option<InprocAddress>,
    server_public_key: Option<CurvePublicKey>,
    curve_keypair: Option<(CurveSecretKey, CurvePublicKey)>,
    address: Option<NetAddress>,
    linger: Option<Linger>,
}

fn random_connection_id() -> ConnectionId {
    let rng = &mut OsRng::new().unwrap();
    (0..8).map(|_| rng.gen::<u8>()).collect::<Vec<u8>>().into()
}

impl<'c> PeerConnectionContextFactory<'c> {
    factory_setter!(with_direction, direction, Option<Direction>);

    factory_setter!(with_connection_id, connection_id, Option<ConnectionId>);

    factory_setter!(with_message_sink_address, message_sink_address, Option<InprocAddress>);

    factory_setter!(with_server_public_key, server_public_key, Option<CurvePublicKey>);

    factory_setter!(
        with_curve_keypair,
        curve_keypair,
        Option<(CurveSecretKey, CurvePublicKey)>
    );

    factory_setter!(with_address, address, Option<NetAddress>);

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

        let address = self.address.or(Some("127.0.0.1:0".parse().unwrap())).unwrap();

        let mut builder = PeerConnectionContextBuilder::new()
            .set_id(self.connection_id.clone().or(Some(random_connection_id())).unwrap())
            .set_linger(self.linger.or(Some(Linger::Indefinitely)).unwrap())
            .set_direction(direction.clone())
            .set_context(context)
            .set_address(address)
            .set_message_sink_address(self.message_sink_address.or(Some(InprocAddress::random())).unwrap());

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

        let peer_context = builder.build().map_err(TestFactoryError::build_failed())?;

        Ok(peer_context)
    }
}
