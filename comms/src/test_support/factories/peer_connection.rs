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

use super::{peer::PeersFactory, peer_connection_context::PeerConnectionContextFactory, Factory, FactoryError};

use crate::{
    connection::{
        peer_connection::{ConnectionId, PeerConnectionContext},
        Context,
        CurveEncryption,
        CurvePublicKey,
        CurveSecretKey,
        Direction,
        InprocAddress,
        PeerConnection,
        PeerConnectionContextBuilder,
    },
    peer_manager::{Peer, PeerManager},
    types::{CommsDataStore, CommsPublicKey},
};
use rand::{OsRng, Rng};

pub fn create<'c>() -> PeerConnectionFactory<'c> {
    PeerConnectionFactory::default()
}

#[derive(Default)]
pub struct PeerConnectionFactory<'c> {
    peer_connection_context_factory: PeerConnectionContextFactory<'c>,
}

impl<'c> PeerConnectionFactory<'c> {
    pub fn with_peer_connection_context_factory(mut self, context_factory: PeerConnectionContextFactory<'c>) -> Self {
        self.peer_connection_context_factory = context_factory;
        self
    }

    fn random_connection_id() -> ConnectionId {
        let rng = &mut OsRng::new().unwrap();
        (0..8).map(|_| rng.gen::<u8>()).collect()
    }
}

impl<'c> Factory for PeerConnectionFactory<'c> {
    type Object = (PeerConnection, CurveSecretKey, CurvePublicKey);

    fn build(self) -> Result<Self::Object, FactoryError> {
        let (peer_conn_context, secret_key, public_key) = self
            .peer_connection_context_factory
            .build()
            .map_err(FactoryError::build_failed())?;

        let conn = PeerConnection::new();
        conn.start(peer_conn_context).map_err(FactoryError::build_failed());

        Ok((conn, secret_key, public_key))
    }
}
