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
use futures::{future, Future, StreamExt};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::{Frame, MessageEnvelope, MessageFlags},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy},
};
use tari_service_framework::reply_channel::Receiver;

const LOG_TARGET: &'static str = "tari_p2p::services::comms_outbound";

/// Convenience type alias for the RequestStream
type CommsOutboundRequestRx = Receiver<CommsOutboundRequest, Result<CommsOutboundResponse, CommsOutboundServiceError>>;

/// Service responsible for sending messages to the comms OMS
pub struct CommsOutboundService {
    request_rx: CommsOutboundRequestRx,
    oms: Arc<OutboundMessageService>,
}

impl CommsOutboundService {
    pub fn new(request_rx: CommsOutboundRequestRx, oms: Arc<OutboundMessageService>) -> Self {
        Self { request_rx, oms }
    }

    pub async fn run(mut self) {
        loop {
            futures::select! {
                request_context = self.request_rx.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await).or_else(log_and_discard);
                }
                complete => break,
            }
        }
    }

    pub async fn handle_request(
        &self,
        request: CommsOutboundRequest,
    ) -> Result<CommsOutboundResponse, CommsOutboundServiceError>
    {
        match request {
            CommsOutboundRequest::SendMsg {
                broadcast_strategy,
                flags,
                body,
            } => self.send_msg(broadcast_strategy, flags, *body).await,

            CommsOutboundRequest::Forward {
                broadcast_strategy,
                message_envelope,
            } => self.forward_message(broadcast_strategy, *message_envelope).await,
        }
    }

    fn send_msg(
        &self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        body: Frame,
    ) -> impl Future<Output = Result<(), CommsOutboundServiceError>>
    {
        future::ready(self.oms.send_raw(broadcast_strategy, flags, body).map_err(Into::into))
    }

    fn forward_message(
        &self,
        broadcast_strategy: BroadcastStrategy,
        envelope: MessageEnvelope,
    ) -> impl Future<Output = Result<(), CommsOutboundServiceError>>
    {
        future::ready(
            self.oms
                .forward_message(broadcast_strategy, envelope)
                .map_err(Into::into),
        )
    }
}

fn log_and_discard<T, E>(_: Result<T, E>) -> Result<(), E> {
    error!(target: LOG_TARGET, "Failed to send reply");
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crossbeam_channel as channel;
    use futures::{
        executor::{block_on, LocalPool},
        pin_mut,
        task::SpawnExt,
        FutureExt,
    };
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
    use tari_service_framework::reply_channel;
    use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
    use tari_utilities::message_format::MessageFormat;
    use tempdir::TempDir;
    use tower_service::Service;

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
    fn dead_request_stream() {
        let (oms, _) = setup_oms();
        let (sender, receiver) = reply_channel::unbounded();
        let service = CommsOutboundService::new(receiver, oms);
        // Drop the sender, the service should stop running
        drop(sender);

        let fut = service.run().fuse();
        pin_mut!(fut);

        // Test that the run future immediately completes because the receiver stream is closed
        block_on(async {
            futures::select! {
                _ = fut => {},
                complete => {panic!("run() future was not ready immediately")},
            }
        });
    }

    #[test]
    fn call_send_message() {
        let (oms, oms_rx) = setup_oms();
        let (mut sender, receiver) = reply_channel::unbounded();
        let service = CommsOutboundService::new(receiver, oms);

        let mut pool = LocalPool::new();
        pool.spawner().spawn(service.run()).unwrap();

        let res = pool.run_until(sender.call(CommsOutboundRequest::SendMsg {
            broadcast_strategy: BroadcastStrategy::Flood,
            flags: MessageFlags::empty(),
            body: Box::new(Vec::new()),
        }));

        assert!(res.is_ok());

        // We only care that OMS got called (i.e the Receiver received something)
        assert!(!oms_rx.is_empty());
    }

    #[test]
    fn call_forward() {
        let (oms, oms_rx) = setup_oms();
        let (mut sender, receiver) = reply_channel::unbounded();
        let service = CommsOutboundService::new(receiver, oms);

        let mut pool = LocalPool::new();
        // Run the service
        pool.spawner().spawn(service.run()).unwrap();

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

        let res = pool.run_until(sender.call(CommsOutboundRequest::Forward {
            broadcast_strategy: BroadcastStrategy::Flood,
            message_envelope: Box::new(MessageEnvelope::new(vec![0], header.to_binary().unwrap(), vec![])),
        }));

        assert!(res.is_ok());

        // We only care that OMS got called (i.e the Receiver received something)
        assert!(!oms_rx.is_empty());
    }
}
