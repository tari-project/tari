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

use crate::{
    actor::DhtRequester,
    config::DhtConfig,
    crypt,
    envelope::{Destination, DhtMessageFlags, DhtMessageHeader, DhtMessageOrigin, NodeDestination},
    inbound::{DecryptedDhtMessage, DhtInboundMessage},
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::{
        envelope::DhtMessageType,
        store_forward::{StoredMessage, StoredMessagesRequest, StoredMessagesResponse},
    },
    store_forward::{error::StoreAndForwardError, SafStorage},
    utils::hoist_nested_result,
    PipelineError,
};
use digest::Digest;
use futures::{future, stream, Future, FutureExt, StreamExt};
use log::*;
use prost::Message;
use std::{convert::TryInto, sync::Arc};
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, Peer, PeerManager, PeerManagerError},
    types::Challenge,
    utils::signature,
};
use tari_crypto::tari_utilities::ByteArray;
use tokio::{runtime, task};
use tower::{Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::store_forward";

pub struct MessageHandlerTask<S> {
    config: DhtConfig,
    next_service: S,
    dht_requester: DhtRequester,
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    message: Option<DecryptedDhtMessage>,
    store: Arc<SafStorage>,
}

impl<S> MessageHandlerTask<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DhtConfig,
        next_service: S,
        store: Arc<SafStorage>,
        dht_requester: DhtRequester,
        peer_manager: Arc<PeerManager>,
        outbound_service: OutboundMessageRequester,
        node_identity: Arc<NodeIdentity>,
        message: DecryptedDhtMessage,
    ) -> Self
    {
        Self {
            config,
            store,
            dht_requester,
            next_service,
            peer_manager,
            outbound_service,
            node_identity,
            message: Some(message),
        }
    }

    pub async fn run(mut self) -> Result<(), PipelineError> {
        let message = self
            .message
            .take()
            .expect("DhtInboundMessageTask initialized without message");

        if message.dht_header.message_type.is_dht_message() && message.decryption_failed() {
            debug!(
                target: LOG_TARGET,
                "Received SAFRetrieveMessages message which could not decrypt from NodeId={}. Discarding message.",
                message.source_peer.node_id
            );
            return Ok(());
        }

        match message.dht_header.message_type {
            DhtMessageType::SafRequestMessages => self.handle_stored_messages_request(message).await?,

            DhtMessageType::SafStoredMessages => self.handle_stored_messages(message).await?,
            // Not a SAF message, call downstream middleware
            _ => {
                trace!(target: LOG_TARGET, "Passing message onto next service");
                self.next_service.oneshot(message).await?
            },
        }

        Ok(())
    }

    async fn handle_stored_messages_request(
        &mut self,
        message: DecryptedDhtMessage,
    ) -> Result<(), StoreAndForwardError>
    {
        trace!(
            target: LOG_TARGET,
            "Received request for stored message from {}",
            message.source_peer.public_key
        );
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let retrieve_msgs = msg
            .decode_part::<StoredMessagesRequest>(0)?
            .ok_or_else(|| StoreAndForwardError::InvalidEnvelopeBody)?;

        if !self.peer_manager.in_network_region(
            &message.source_peer.node_id,
            self.node_identity.node_id(),
            self.config.saf_num_closest_nodes,
        )? {
            debug!(
                target: LOG_TARGET,
                "Received store and forward message requests from node outside of this nodes network region"
            );
            return Ok(());
        }

        // Compile a set of stored messages for the requesting peer
        let messages = self.store.with_lock(|mut store| {
            store
                .iter()
                // All messages within start_time (if specified)
                .filter(|(_, msg)| {
                    retrieve_msgs.since.as_ref().map(|since| msg.stored_at.as_ref().map(|s| since.seconds <= s.seconds).unwrap_or( false)).unwrap_or( true)
                })
                .filter(|(_, msg)|{
                    if msg.dht_header.is_none() {
                        warn!(target: LOG_TARGET, "Message was stored without a header. This should never happen!");
                        return false;
                    }
                    let dht_header = msg.dht_header.as_ref().expect("previously checked");

                    match &dht_header.destination {
                        None=> false,
                        // The stored message was sent with an undisclosed recipient. Perhaps this node
                        // is interested in it
                        Some(Destination::Unknown(_)) => true,
                        // Was the stored message sent for the requesting node public key?
                        Some(Destination::PublicKey(pk)) => pk.as_slice() == message.source_peer.public_key.as_bytes(),
                        // Was the stored message sent for the requesting node node id?
                        Some( Destination::NodeId(node_id)) => node_id.as_slice() == message.source_peer.node_id.as_bytes(),
                    }
                })
                .take(self.config.saf_max_returned_messages)
                .map(|(_, msg)| msg)
                .cloned()
                .collect::<Vec<_>>()
        });

        let stored_messages: StoredMessagesResponse = messages.into();

        trace!(
            target: LOG_TARGET,
            "Responding to received message retrieval request with {} message(s)",
            stored_messages.messages().len()
        );
        self.outbound_service
            .send_message_no_header(
                SendMessageParams::new()
                    .direct_public_key(message.source_peer.public_key.clone())
                    .with_dht_message_type(DhtMessageType::SafStoredMessages)
                    .finish(),
                stored_messages,
            )
            .await?;

        Ok(())
    }

    async fn handle_stored_messages(self, message: DecryptedDhtMessage) -> Result<(), StoreAndForwardError> {
        trace!(
            target: LOG_TARGET,
            "Received stored messages from {}",
            message.source_peer.public_key
        );
        // TODO: Should check that stored messages were requested before accepting them
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");
        let response = msg
            .decode_part::<StoredMessagesResponse>(0)?
            .ok_or_else(|| StoreAndForwardError::InvalidEnvelopeBody)?;
        let source_peer = Arc::new(message.source_peer);

        debug!(
            target: LOG_TARGET,
            "Received {} stored messages from peer",
            response.messages().len()
        );

        let tasks = response
            .messages
            .into_iter()
            // Map to futures which process the stored message
            .map(|msg| self.process_incoming_stored_message(Arc::clone(&source_peer), msg));

        let successful_msgs_iter = future::join_all(tasks)
            .await
            .into_iter()
            .map(|result| {
                match &result {
                    // Failed decryption is acceptable, the message wasn't for this node so we
                    // simply discard the message.
                    // TODO: Should we add this message to our SAF store?
                    Err(err @ StoreAndForwardError::DecryptionFailed) => {
                        debug!(
                            target: LOG_TARGET,
                            "Unable to decrypt stored message sent by {}: {}",
                            source_peer.node_id.short_str(),
                            err
                        );
                    },
                    // The peer that originally sent this message is not known to us.
                    // TODO: Should we try to discover this peer?
                    Err(StoreAndForwardError::PeerManagerError(PeerManagerError::PeerNotFoundError)) => {
                        debug!(target: LOG_TARGET, "Origin peer not found. Discarding stored message.");
                    },

                    // Failed to send request to Dht Actor, something has gone very wrong
                    Err(StoreAndForwardError::DhtActorError(err)) => {
                        error!(
                            target: LOG_TARGET,
                            "DhtActor returned an error. {}. This could indicate a system malfunction.", err
                        );
                    },
                    // Duplicate message detected, no problem it happens.
                    Err(StoreAndForwardError::DuplicateMessage) => {
                        debug!(
                            target: LOG_TARGET,
                            "Store and forward received a duplicate message. Message discarded."
                        );
                    },

                    // Every other error shouldn't happen if the sending node is behaving
                    Err(err) => {
                        // TODO: #banheuristics
                        warn!(
                            target: LOG_TARGET,
                            "SECURITY: invalid store and forward message was discarded from NodeId={}. Reason: {}. \
                             These messages should never have been forwarded. This is a sign of a badly behaving node.",
                            source_peer.node_id.short_str(),
                            err
                        );
                    },
                    _ => {},
                }

                result
            })
            .filter(Result::is_ok)
            .map(Result::unwrap);

        self.next_service
            .call_all(stream::iter(successful_msgs_iter))
            .unordered()
            .for_each(|service_result| {
                if let Err(err) = service_result {
                    error!(target: LOG_TARGET, "Error when calling next service: {}", err);
                }
                future::ready(())
            })
            .await;

        Ok(())
    }

    fn process_incoming_stored_message(
        &self,
        source_peer: Arc<Peer>,
        message: StoredMessage,
    ) -> impl Future<Output = Result<DecryptedDhtMessage, StoreAndForwardError>>
    {
        let node_identity = Arc::clone(&self.node_identity);
        let peer_manager = Arc::clone(&self.peer_manager);
        let config = self.config.clone();
        let mut dht_requester = self.dht_requester.clone();
        task::spawn_blocking(move || {
            if message.dht_header.is_none() {
                return Err(StoreAndForwardError::DhtHeaderNotProvided);
            }

            let dht_header: DhtMessageHeader = message
                .dht_header
                .expect("previously checked")
                .try_into()
                .map_err(StoreAndForwardError::DhtMessageError)?;

            let dht_flags = dht_header.flags;

            let origin = dht_header
                .origin
                .as_ref()
                .ok_or_else(|| StoreAndForwardError::MessageOriginRequired)?;

            // Check that the destination is either undisclosed
            Self::check_destination(&config, &peer_manager, &node_identity, &dht_header)?;
            // Verify the signature
            Self::check_signature(origin, &message.encrypted_body)?;
            // Check that the message has not already been received.
            // The current thread runtime is used because calls to the DHT actor are async
            let mut rt = runtime::Builder::new().basic_scheduler().build()?;
            rt.block_on(Self::check_duplicate(&mut dht_requester, &message.encrypted_body))?;

            // Attempt to decrypt the message (if applicable), and deserialize it
            let decrypted_body =
                Self::maybe_decrypt_and_deserialize(&node_identity, origin, dht_flags, &message.encrypted_body)?;

            let inbound_msg = DhtInboundMessage::new(dht_header, Arc::clone(&source_peer), message.encrypted_body);

            Ok(DecryptedDhtMessage::succeeded(decrypted_body, inbound_msg))
        })
        .map(hoist_nested_result)
    }

    async fn check_duplicate(dht_requester: &mut DhtRequester, body: &[u8]) -> Result<(), StoreAndForwardError> {
        let msg_hash = Challenge::new().chain(body).result().to_vec();
        if dht_requester.insert_message_hash(msg_hash).await? {
            Err(StoreAndForwardError::DuplicateMessage)
        } else {
            Ok(())
        }
    }

    fn check_destination(
        config: &DhtConfig,
        peer_manager: &PeerManager,
        node_identity: &NodeIdentity,
        dht_header: &DhtMessageHeader,
    ) -> Result<(), StoreAndForwardError>
    {
        Some(&dht_header.destination)
            .filter(|destination| match destination {
                NodeDestination::Unknown => true,
                NodeDestination::PublicKey(pk) => node_identity.public_key() == pk,
                NodeDestination::NodeId(node_id) => {
                    // Pass this check if the node id equals ours or is in this node's region
                    if node_identity.node_id() == node_id {
                        return true;
                    }

                    peer_manager
                        .in_network_region(node_identity.node_id(), node_id, config.num_neighbouring_nodes)
                        .unwrap_or(false)
                },
            })
            .map(|_| ())
            .ok_or_else(|| StoreAndForwardError::InvalidDestination)
    }

    fn check_signature(origin: &DhtMessageOrigin, body: &[u8]) -> Result<(), StoreAndForwardError> {
        signature::verify(&origin.public_key, &origin.signature, body)
            .map_err(|_| StoreAndForwardError::InvalidSignature)
            .and_then(|is_valid| {
                if is_valid {
                    Ok(())
                } else {
                    Err(StoreAndForwardError::InvalidSignature)
                }
            })
    }

    fn maybe_decrypt_and_deserialize(
        node_identity: &NodeIdentity,
        origin: &DhtMessageOrigin,
        flags: DhtMessageFlags,
        body: &[u8],
    ) -> Result<EnvelopeBody, StoreAndForwardError>
    {
        if flags.contains(DhtMessageFlags::ENCRYPTED) {
            let shared_secret = crypt::generate_ecdh_secret(node_identity.secret_key(), &origin.public_key);
            let decrypted_bytes = crypt::decrypt(&shared_secret, body)?;
            EnvelopeBody::decode(decrypted_bytes.as_slice()).map_err(|_| StoreAndForwardError::DecryptionFailed)
        } else {
            // Malformed cleartext messages should never have been forwarded by the peer
            EnvelopeBody::decode(body).map_err(|_| StoreAndForwardError::MalformedMessage)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        store_forward::message::datetime_to_timestamp,
        test_utils::{
            create_dht_actor_mock,
            make_dht_inbound_message,
            make_node_identity,
            make_peer_manager,
            service_spy,
            DhtMockState,
        },
        PipelineError,
    };
    use chrono::Utc;
    use futures::channel::mpsc;
    use prost::Message;
    use std::time::Duration;
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tokio::runtime::Handle;

    // TODO: unit tests for static functions (check_signature, etc)

    #[tokio_macros::test_basic]
    async fn request_stored_messages() {
        let rt_handle = Handle::current();
        let spy = service_spy();
        let storage = Arc::new(SafStorage::new(10));

        let peer_manager = make_peer_manager();
        let (oms_tx, mut oms_rx) = mpsc::channel(1);

        let node_identity = make_node_identity();

        // Recent message
        let inbound_msg = make_dht_inbound_message(&node_identity, vec![], DhtMessageFlags::empty());
        storage.insert(
            vec![0],
            StoredMessage::new(0, inbound_msg.dht_header, b"A".to_vec()),
            Duration::from_secs(60),
        );

        // Expired message
        let inbound_msg = make_dht_inbound_message(&node_identity, vec![], DhtMessageFlags::empty());
        storage.insert(
            vec![1],
            StoredMessage::new(0, inbound_msg.dht_header, vec![]),
            Duration::from_secs(0),
        );

        // Out of time range
        let inbound_msg = make_dht_inbound_message(&node_identity, vec![], DhtMessageFlags::empty());
        let mut msg = StoredMessage::new(0, inbound_msg.dht_header, vec![]);
        msg.stored_at = Some(datetime_to_timestamp(
            Utc::now().checked_sub_signed(chrono::Duration::days(1)).unwrap(),
        ));

        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesRequest::since(
                Utc::now().checked_sub_signed(chrono::Duration::seconds(60)).unwrap()
            ))
            .unwrap(),
            make_dht_inbound_message(&node_identity, vec![], DhtMessageFlags::ENCRYPTED),
        );
        message.dht_header.message_type = DhtMessageType::SafRequestMessages;

        let (tx, _) = mpsc::channel(1);
        let dht_requester = DhtRequester::new(tx);

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            storage,
            dht_requester,
            peer_manager,
            OutboundMessageRequester::new(oms_tx),
            node_identity,
            message,
        );

        rt_handle.spawn(task.run());

        let (_, body) = unwrap_oms_send_msg!(oms_rx.next().await.unwrap());
        let body = EnvelopeBody::decode(body.as_slice()).unwrap();
        let msg = body.decode_part::<StoredMessagesResponse>(0).unwrap().unwrap();
        assert_eq!(msg.messages().len(), 1);
        assert_eq!(msg.messages()[0].encrypted_body, b"A");
        assert!(!spy.is_called());
    }

    #[tokio_macros::test_basic]
    async fn receive_stored_messages() {
        let rt_handle = Handle::current();
        let spy = service_spy();
        let storage = Arc::new(SafStorage::new(10));

        let peer_manager = make_peer_manager();
        let (oms_tx, _) = mpsc::channel(1);

        let node_identity = make_node_identity();

        let shared_key = crypt::generate_ecdh_secret(node_identity.secret_key(), node_identity.public_key());
        let msg_a = crypt::encrypt(
            &shared_key,
            &wrap_in_envelope_body!(&b"A".to_vec())
                .unwrap()
                .to_encoded_bytes()
                .unwrap(),
        )
        .unwrap();

        let inbound_msg_a = make_dht_inbound_message(&node_identity, msg_a.clone(), DhtMessageFlags::ENCRYPTED);
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_a.source_peer))
            .unwrap();
        let msg_b = crypt::encrypt(
            &shared_key,
            &wrap_in_envelope_body!(b"B".to_vec())
                .unwrap()
                .to_encoded_bytes()
                .unwrap(),
        )
        .unwrap();

        let inbound_msg_b = make_dht_inbound_message(&node_identity, msg_b.clone(), DhtMessageFlags::ENCRYPTED);
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_b.source_peer))
            .unwrap();

        let msg1 = StoredMessage::new(0, inbound_msg_a.dht_header.clone(), msg_a);
        let msg2 = StoredMessage::new(0, inbound_msg_b.dht_header, msg_b);
        // Cleartext message
        let clear_msg = wrap_in_envelope_body!(b"Clear".to_vec())
            .unwrap()
            .to_encoded_bytes()
            .unwrap();
        let clear_header =
            make_dht_inbound_message(&node_identity, clear_msg.clone(), DhtMessageFlags::empty()).dht_header;
        let msg_clear = StoredMessage::new(0, clear_header, clear_msg);
        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesResponse {
                messages: vec![msg1.clone(), msg2, msg_clear],
            })
            .unwrap(),
            make_dht_inbound_message(&node_identity, vec![], DhtMessageFlags::ENCRYPTED),
        );
        message.dht_header.message_type = DhtMessageType::SafStoredMessages;

        let (dht_requester, mut mock) = create_dht_actor_mock(1);
        let mock_state = DhtMockState::new();
        mock.set_shared_state(mock_state.clone());
        rt_handle.spawn(mock.run());

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            storage,
            dht_requester,
            peer_manager,
            OutboundMessageRequester::new(oms_tx),
            node_identity,
            message,
        );

        task.run().await.unwrap();
        assert_eq!(spy.call_count(), 3);
        let requests = spy.take_requests();
        assert_eq!(requests.len(), 3);
        // Deserialize each request into the message (a vec of a single byte in this case)
        let msgs = requests
            .into_iter()
            .map(|req| req.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap())
            .collect::<Vec<Vec<u8>>>();
        assert!(msgs.contains(&b"A".to_vec()));
        assert!(msgs.contains(&b"B".to_vec()));
        assert!(msgs.contains(&b"Clear".to_vec()));
        assert_eq!(mock_state.call_count(), msgs.len());
    }
}
