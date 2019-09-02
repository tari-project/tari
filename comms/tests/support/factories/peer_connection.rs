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

use super::{peer_connection_context::PeerConnectionContextFactory, TestFactory, TestFactoryError};
use std::thread::JoinHandle;
use tari_comms::connection::{ConnectionError, PeerConnection};

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
}

impl<'c> TestFactory for PeerConnectionFactory<'c> {
    type Object = (PeerConnection, JoinHandle<Result<(), ConnectionError>>);

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        let peer_conn_context = self
            .peer_connection_context_factory
            .build()
            .map_err(TestFactoryError::build_failed())?;

        let mut conn = PeerConnection::new();
        let handle = conn
            .start(peer_conn_context)
            .map_err(TestFactoryError::build_failed())?;

        Ok((conn, handle))
    }
}
