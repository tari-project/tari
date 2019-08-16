// Copyright 2019 The Tari Project
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

use crate::services::comms_outbound::{
    error::CommsOutboundServiceError,
    messages::{CommsOutboundRequest, CommsOutboundResponse},
};
use futures::{
    future::{self, Either},
    Future,
    Poll,
};
use std::sync::Arc;
use tari_comms::{
    message::{Frame, MessageEnvelope, MessageFlags},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy},
};
use tower_service::Service;

/// Service responsible for sending messages to the comms OMS
pub struct CommsOutboundService {
    oms: Arc<OutboundMessageService>,
}

impl CommsOutboundService {
    pub fn new(oms: Arc<OutboundMessageService>) -> Self {
        Self { oms }
    }

    fn send_msg(
        &self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        body: Frame,
    ) -> impl Future<Item = Result<(), CommsOutboundServiceError>, Error = CommsOutboundServiceError>
    {
        // TODO(sdbondi): Change required when oms is async
        future::ok(
            self.oms
                .send_raw(broadcast_strategy, flags, body)
                .map_err(CommsOutboundServiceError::OutboundError),
        )
    }

    fn forward_message(
        &self,
        broadcast_strategy: BroadcastStrategy,
        envelope: MessageEnvelope,
    ) -> impl Future<Item = Result<(), CommsOutboundServiceError>, Error = CommsOutboundServiceError>
    {
        // TODO(sdbondi): Change required when oms is async
        future::ok(
            self.oms
                .forward_message(broadcast_strategy, envelope)
                .map_err(CommsOutboundServiceError::OutboundError),
        )
    }
}

impl Service<CommsOutboundRequest> for CommsOutboundService {
    type Error = CommsOutboundServiceError;
    type Response = Result<CommsOutboundResponse, CommsOutboundServiceError>;

    existential type Future: Future<Item = Self::Response, Error = Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, req: CommsOutboundRequest) -> Self::Future {
        match req {
            // Send a ping synchronously for now until comms is async
            CommsOutboundRequest::SendMsg {
                broadcast_strategy,
                flags,
                body,
            } => Either::A(self.send_msg(broadcast_strategy, flags, *body)),
            CommsOutboundRequest::Forward {
                broadcast_strategy,
                message_envelope,
            } => Either::B(self.forward_message(broadcast_strategy, *message_envelope)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crossbeam_channel as channel;
    use futures::Async;
    use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
    use std::iter;
    use tari_comms::{
        connection::NetAddress,
        message::{MessageEnvelopeHeader, NodeDestination},
        outbound_message_service::OutboundMessage,
        peer_manager::{NodeId, NodeIdentity, Peer, PeerFlags, PeerManager},
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tari_storage::{key_val_store::lmdb_database::LMDBWrapper, lmdb_store::LMDBBuilder};
    use tari_utilities::message_format::MessageFormat;
    use tempdir::TempDir;

    pub fn random_string(len: usize) -> String {
        let mut rng = OsRng::new().unwrap();
        iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
    }

    // TODO: Thankfully, this won't be needed in 'Future' :P - Remove this once the OMS is a Sink.
    fn setup_oms() -> (Arc<OutboundMessageService>, channel::Receiver<OutboundMessage>) {
        let tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
        let mut rng = OsRng::new().unwrap();
        let node_identity = NodeIdentity::random(&mut rng, "127.0.0.1:9000".parse().unwrap())
            .map(Arc::new)
            .unwrap();

        let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = "127.0.0.1:55445".parse::<NetAddress>().unwrap().into();
        let dest_peer = Peer::new(pk, node_id, net_addresses, PeerFlags::default());
        let database_name = random_string(8);
        let datastore = LMDBBuilder::new()
            .set_path(tmpdir.path().to_str().unwrap())
            .set_environment_size(10)
            .set_max_number_of_databases(1)
            .add_database(&database_name, lmdb_zero::db::CREATE)
            .build()
            .unwrap();

        let peer_database = datastore.get_handle(&database_name).unwrap();
        let peer_database = LMDBWrapper::new(Arc::new(peer_database));

        // Add a peer so that something will be sent
        let peer_manager = PeerManager::new(peer_database).map(Arc::new).unwrap();
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let (message_sender, message_receiver) = channel::unbounded();
        (
            OutboundMessageService::new(node_identity.clone(), message_sender, peer_manager)
                .map(Arc::new)
                .unwrap(),
            message_receiver,
        )
    }

    #[test]
    fn poll_ready() {
        let (oms, _) = setup_oms();
        let mut service = CommsOutboundService::new(oms);

        // Always ready
        assert!(service.poll_ready().unwrap().is_ready());
    }

    #[test]
    fn call_send_message() {
        let (oms, oms_rx) = setup_oms();
        let mut service = CommsOutboundService::new(oms);

        let mut fut = service.call(CommsOutboundRequest::SendMsg {
            broadcast_strategy: BroadcastStrategy::Flood,
            flags: MessageFlags::empty(),
            body: Box::new(Vec::new()),
        });

        match fut.poll().unwrap() {
            Async::Ready(Ok(_)) => {},
            Async::Ready(Err(err)) => panic!("unexpected failed result for send_message: {:?}", err),
            _ => panic!("future is not ready"),
        }

        // We only care that OMS got called (i.e the Receiver received something)
        assert!(!oms_rx.is_empty());
    }

    #[test]
    fn call_forward() {
        let (oms, oms_rx) = setup_oms();
        let mut service = CommsOutboundService::new(oms);
        let mut rng = OsRng::new().unwrap();
        let header = MessageEnvelopeHeader {
            version: 0,
            origin_source: CommsPublicKey::random_keypair(&mut rng).1,
            peer_source: CommsPublicKey::random_keypair(&mut rng).1,
            dest: NodeDestination::Unknown,
            origin_signature: vec![],
            peer_signature: vec![],
            flags: MessageFlags::empty(),
        };

        let mut fut = service.call(CommsOutboundRequest::Forward {
            broadcast_strategy: BroadcastStrategy::Flood,
            message_envelope: Box::new(MessageEnvelope::new(vec![0], header.to_binary().unwrap(), vec![])),
        });

        match fut.poll().unwrap() {
            Async::Ready(Ok(_)) => {},
            Async::Ready(Err(err)) => panic!("unexpected failed result for forward: {:?}", err),
            _ => panic!("future is not ready"),
        }

        // We only care that OMS got called (i.e the Receiver received something)
        assert!(!oms_rx.is_empty());
    }
}
