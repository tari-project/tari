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
    envelope::{timestamp_to_datetime, DhtMessageFlags, DhtMessageHeader, NodeDestination},
    inbound::{DecryptedDhtMessage, DhtInboundMessage},
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::{
        envelope::{DhtMessageType, OriginMac},
        store_forward::{
            stored_messages_response::SafResponseType,
            StoredMessage as ProtoStoredMessage,
            StoredMessagesRequest,
            StoredMessagesResponse,
        },
    },
    storage::DhtMetadataKey,
    store_forward::{error::StoreAndForwardError, service::FetchStoredMessageQuery, StoreAndForwardRequester},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use digest::Digest;
use futures::{channel::mpsc, future, stream, SinkExt, StreamExt};
use log::*;
use prost::Message;
use std::{convert::TryInto, sync::Arc};
use tari_comms::{
    message::{EnvelopeBody, MessageTag},
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerManager, PeerManagerError},
    pipeline::PipelineError,
    types::{Challenge, CommsPublicKey},
    utils::signature,
};
use tari_utilities::{convert::try_convert_all, ByteArray};
use tower::{Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::storeforward::handler";

pub struct MessageHandlerTask<S> {
    config: DhtConfig,
    next_service: S,
    dht_requester: DhtRequester,
    peer_manager: Arc<PeerManager>,
    outbound_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    message: Option<DecryptedDhtMessage>,
    saf_requester: StoreAndForwardRequester,
    saf_response_signal_sender: mpsc::Sender<()>,
}

impl<S> MessageHandlerTask<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DhtConfig,
        next_service: S,
        saf_requester: StoreAndForwardRequester,
        dht_requester: DhtRequester,
        peer_manager: Arc<PeerManager>,
        outbound_service: OutboundMessageRequester,
        node_identity: Arc<NodeIdentity>,
        message: DecryptedDhtMessage,
        saf_response_signal_sender: mpsc::Sender<()>,
    ) -> Self {
        Self {
            config,
            saf_requester,
            dht_requester,
            next_service,
            peer_manager,
            outbound_service,
            node_identity,
            message: Some(message),
            saf_response_signal_sender,
        }
    }

    pub async fn run(mut self) -> Result<(), PipelineError> {
        let message = self
            .message
            .take()
            .expect("DhtInboundMessageTask initialized without message");

        if message.is_duplicate() {
            debug!(
                target: LOG_TARGET,
                "Received message ({}) that has already been received {} time(s). Last sent by peer '{}', passing on \
                 (Trace: {})",
                message.tag,
                message.dedup_hit_count,
                message.source_peer.node_id.short_str(),
                message.dht_header.message_tag,
            );
            self.next_service.oneshot(message).await?;
            return Ok(());
        }

        if message.dht_header.message_type.is_saf_message() && message.decryption_failed() {
            debug!(
                target: LOG_TARGET,
                "Received store and forward message {} which could not decrypt from NodeId={}. Discarding message. \
                 (Trace: {})",
                message.tag,
                message.source_peer.node_id,
                message.dht_header.message_tag
            );
            return Ok(());
        }

        match message.dht_header.message_type {
            DhtMessageType::SafRequestMessages => {
                if self.node_identity.has_peer_features(PeerFeatures::DHT_STORE_FORWARD) {
                    self.handle_stored_messages_request(message).await?
                } else {
                    // TODO: #banheuristics - requester should not have requested store and forward messages from this
                    //       node
                    debug!(
                        target: LOG_TARGET,
                        "Received store and forward request {} from peer '{}' however, this node is not a store and \
                         forward node. Request ignored. (Trace: {})",
                        message.tag,
                        message.source_peer.node_id.short_str(),
                        message.dht_header.message_tag
                    );
                }
            },

            DhtMessageType::SafStoredMessages => self.handle_stored_messages(message).await?,
            // Not a SAF message, call downstream middleware
            _ => {
                trace!(
                    target: LOG_TARGET,
                    "Passing message {} onto next service (Trace: {})",
                    message.tag,
                    message.dht_header.message_tag
                );
                self.next_service.oneshot(message).await?;
            },
        }

        Ok(())
    }

    async fn handle_stored_messages_request(
        &mut self,
        message: DecryptedDhtMessage,
    ) -> Result<(), StoreAndForwardError> {
        trace!(
            target: LOG_TARGET,
            "Received request for stored message {} from {} (Trace: {})",
            message.tag,
            message.source_peer.public_key,
            message.dht_header.message_tag
        );
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let retrieve_msgs = msg
            .decode_part::<StoredMessagesRequest>(0)?
            .ok_or(StoreAndForwardError::InvalidEnvelopeBody)?;

        let source_pubkey = Box::new(message.source_peer.public_key.clone());
        let source_node_id = Box::new(message.source_peer.node_id.clone());

        // Compile a set of stored messages for the requesting peer
        let mut query = FetchStoredMessageQuery::new(source_pubkey, source_node_id.clone());

        let since: Option<DateTime<Utc>> = match retrieve_msgs.since.map(timestamp_to_datetime) {
            Some(since) => {
                debug!(
                    target: LOG_TARGET,
                    "Peer '{}' requested all messages since '{}'",
                    source_node_id.short_str(),
                    since
                );
                query.with_messages_since(since);
                Some(since)
            },
            None => None,
        };

        let response_types = vec![SafResponseType::ForMe];

        for resp_type in response_types {
            query.with_response_type(resp_type);
            let messages = self.saf_requester.fetch_messages(query.clone()).await?;

            let stored_messages = StoredMessagesResponse {
                messages: try_convert_all(messages)?,
                request_id: retrieve_msgs.request_id,
                response_type: resp_type as i32,
            };

            debug!(
                target: LOG_TARGET,
                "Responding to received message retrieval request with {} {:?} message(s)",
                stored_messages.messages().len(),
                resp_type
            );

            match self
                .outbound_service
                .send_message_no_header(
                    SendMessageParams::new()
                        .direct_public_key(message.source_peer.public_key.clone())
                        .with_dht_message_type(DhtMessageType::SafStoredMessages)
                        .finish(),
                    stored_messages,
                )
                .await?
                .resolve()
                .await
            {
                Ok(_) => {
                    if let Some(threshold) = since {
                        debug!(
                            target: LOG_TARGET,
                            "Removing stored message(s) from before {} for peer '{}'",
                            threshold,
                            message.source_peer.node_id.short_str()
                        );
                        self.saf_requester.remove_messages_older_than(threshold).await?;
                    }
                },
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send stored messages to peer '{}': {}",
                        message.source_peer.node_id.short_str(),
                        err
                    );
                },
            }
        }

        Ok(())
    }

    async fn handle_stored_messages(mut self, message: DecryptedDhtMessage) -> Result<(), StoreAndForwardError> {
        trace!(
            target: LOG_TARGET,
            "Received stored messages from {} (Trace: {})",
            message.source_peer.public_key,
            message.dht_header.message_tag
        );
        let source_node_id = message.source_peer.node_id.clone();
        let message_tag = message.dht_header.message_tag;
        // TODO: Should check that stored messages were requested before accepting them
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");
        let response = msg
            .decode_part::<StoredMessagesResponse>(0)?
            .ok_or(StoreAndForwardError::InvalidEnvelopeBody)?;
        let source_peer = Arc::new(message.source_peer);

        debug!(
            target: LOG_TARGET,
            "Received {} stored messages of type {} from peer `{}` (Trace: {})",
            response.messages().len(),
            SafResponseType::from_i32(response.response_type)
                .as_ref()
                .map(|t| format!("{:?}", t))
                .unwrap_or_else(|| "<Invalid>".to_string()),
            source_node_id,
            message_tag
        );

        let mut results = Vec::with_capacity(response.messages.len());
        for msg in response.messages {
            let result = self
                .process_incoming_stored_message(Arc::clone(&source_peer), msg)
                .await;
            results.push(result);
        }

        let successful_msgs_iter = results
            .into_iter()
            .map(|result| {
                match &result {
                    Ok(msg) => {
                        trace!(target: LOG_TARGET, "Recv SAF message: {}", msg);
                    },
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
                }

                result
            })
            .filter(Result::is_ok)
            .map(Result::unwrap);

        // Let the SAF Service know we got a SAF response.
        let _ = self
            .saf_response_signal_sender
            .send(())
            .await
            .map_err(|e| warn!(target: LOG_TARGET, "Error sending SAF response signal; {:?}", e));

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

    async fn process_incoming_stored_message(
        &mut self,
        source_peer: Arc<Peer>,
        message: ProtoStoredMessage,
    ) -> Result<DecryptedDhtMessage, StoreAndForwardError> {
        let node_identity = &self.node_identity;
        let peer_manager = &self.peer_manager;
        let config = &self.config;

        if message.dht_header.is_none() {
            return Err(StoreAndForwardError::DhtHeaderNotProvided);
        }

        let stored_at = match message.stored_at {
            None => chrono::MIN_DATETIME,
            Some(t) => DateTime::from_utc(
                NaiveDateTime::from_timestamp(t.seconds, t.nanos.try_into().unwrap_or(0)),
                Utc,
            ),
        };

        let dht_header: DhtMessageHeader = message
            .dht_header
            .expect("previously checked")
            .try_into()
            .map_err(StoreAndForwardError::DhtMessageError)?;

        if !dht_header.is_valid() {
            return Err(StoreAndForwardError::InvalidDhtHeader);
        }
        let message_type = dht_header.message_type;

        if message_type.is_dht_message() {
            if !message_type.is_dht_discovery() {
                debug!(
                    target: LOG_TARGET,
                    "Discarding {} message from peer '{}'",
                    message_type,
                    source_peer.node_id.short_str()
                );
                return Err(StoreAndForwardError::InvalidDhtMessageType);
            }
            if dht_header.destination.is_unknown() {
                debug!(
                    target: LOG_TARGET,
                    "Discarding anonymous discovery message from peer '{}'",
                    source_peer.node_id.short_str()
                );
                return Err(StoreAndForwardError::InvalidDhtMessageType);
            }
        }

        // Check that the destination is either undisclosed, for us or for our network region
        Self::check_destination(&config, &peer_manager, &node_identity, &dht_header).await?;
        // Check that the message has not already been received.
        Self::check_duplicate(&mut self.dht_requester, &message.body, source_peer.public_key.clone()).await?;

        // Attempt to decrypt the message (if applicable), and deserialize it
        let (authenticated_pk, decrypted_body) =
            Self::authenticate_and_decrypt_if_required(&node_identity, &dht_header, &message.body)?;

        let mut inbound_msg =
            DhtInboundMessage::new(MessageTag::new(), dht_header, Arc::clone(&source_peer), message.body);
        inbound_msg.is_saf_message = true;

        let last_saf_received = self
            .dht_requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::LastSafMessageReceived)
            .await
            .ok()
            .flatten()
            .unwrap_or(chrono::MIN_DATETIME);

        if stored_at > last_saf_received {
            if let Err(err) = self
                .dht_requester
                .set_metadata(DhtMetadataKey::LastSafMessageReceived, stored_at)
                .await
            {
                warn!(
                    target: LOG_TARGET,
                    "Failed to set last SAF message received timestamp: {:?}", err
                );
            }
        }

        Ok(DecryptedDhtMessage::succeeded(
            decrypted_body,
            authenticated_pk,
            inbound_msg,
        ))
    }

    async fn check_duplicate(
        dht_requester: &mut DhtRequester,
        body: &[u8],
        public_key: CommsPublicKey,
    ) -> Result<(), StoreAndForwardError> {
        let msg_hash = Challenge::new().chain(body).finalize().to_vec();
        let hit_count = dht_requester.add_message_to_dedup_cache(msg_hash, public_key).await?;
        if hit_count > 1 {
            Err(StoreAndForwardError::DuplicateMessage)
        } else {
            Ok(())
        }
    }

    async fn check_destination(
        config: &DhtConfig,
        peer_manager: &PeerManager,
        node_identity: &NodeIdentity,
        dht_header: &DhtMessageHeader,
    ) -> Result<(), StoreAndForwardError> {
        let is_valid_destination = match &dht_header.destination {
            NodeDestination::Unknown => true,
            NodeDestination::PublicKey(pk) => node_identity.public_key() == &**pk,
            // Pass this check if the node id equals ours or is in this node's region
            NodeDestination::NodeId(node_id) if node_identity.node_id() == &**node_id => true,
            NodeDestination::NodeId(node_id) => peer_manager
                .in_network_region(node_identity.node_id(), node_id, config.num_neighbouring_nodes)
                .await
                .unwrap_or(false),
        };

        if is_valid_destination {
            Ok(())
        } else {
            Err(StoreAndForwardError::InvalidDestination)
        }
    }

    fn authenticate_and_decrypt_if_required(
        node_identity: &NodeIdentity,
        header: &DhtMessageHeader,
        body: &[u8],
    ) -> Result<(Option<CommsPublicKey>, EnvelopeBody), StoreAndForwardError> {
        if header.flags.contains(DhtMessageFlags::ENCRYPTED) {
            let ephemeral_public_key = header.ephemeral_public_key.as_ref().expect(
                "[store and forward] DHT header is invalid after validity check because it did not contain an \
                 ephemeral_public_key",
            );

            trace!(
                target: LOG_TARGET,
                "Attempting to decrypt origin mac ({} byte(s))",
                header.origin_mac.len()
            );
            let shared_secret = crypt::generate_ecdh_secret(node_identity.secret_key(), ephemeral_public_key);
            let decrypted = crypt::decrypt(&shared_secret, &header.origin_mac)?;
            let authenticated_pk = Self::authenticate_message(&decrypted, body)?;

            trace!(
                target: LOG_TARGET,
                "Attempting to decrypt message body ({} byte(s))",
                body.len()
            );
            let decrypted_bytes = crypt::decrypt(&shared_secret, body)?;
            let envelope_body =
                EnvelopeBody::decode(decrypted_bytes.as_slice()).map_err(|_| StoreAndForwardError::DecryptionFailed)?;
            if envelope_body.is_empty() {
                return Err(StoreAndForwardError::InvalidEnvelopeBody);
            }
            Ok((Some(authenticated_pk), envelope_body))
        } else {
            let authenticated_pk = if !header.origin_mac.is_empty() {
                Some(Self::authenticate_message(&header.origin_mac, body)?)
            } else {
                None
            };
            let envelope_body = EnvelopeBody::decode(body).map_err(|_| StoreAndForwardError::MalformedMessage)?;
            Ok((authenticated_pk, envelope_body))
        }
    }

    fn authenticate_message(origin_mac_body: &[u8], body: &[u8]) -> Result<CommsPublicKey, StoreAndForwardError> {
        let origin_mac = OriginMac::decode(origin_mac_body)?;
        let public_key =
            CommsPublicKey::from_bytes(&origin_mac.public_key).map_err(|_| StoreAndForwardError::InvalidOriginMac)?;

        if signature::verify(&public_key, &origin_mac.signature, body) {
            Ok(public_key)
        } else {
            Err(StoreAndForwardError::InvalidOriginMac)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        outbound::mock::create_outbound_service_mock,
        proto::envelope::DhtHeader,
        store_forward::{message::StoredMessagePriority, StoredMessage},
        test_utils::{
            build_peer_manager,
            create_dht_actor_mock,
            create_store_and_forward_mock,
            make_dht_header,
            make_dht_inbound_message,
            make_keypair,
            make_node_identity,
            service_spy,
        },
    };
    use chrono::{Duration as OldDuration, Utc};
    use futures::channel::mpsc;
    use prost::Message;
    use std::time::Duration;
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tari_crypto::tari_utilities::hex;
    use tari_test_utils::collect_stream;
    use tari_utilities::hex::Hex;
    use tokio::{runtime::Handle, task, time::delay_for};

    // TODO: unit tests for static functions (check_signature, etc)

    fn make_stored_message(
        message: String,
        node_identity: &NodeIdentity,
        dht_header: DhtMessageHeader,
        stored_at: NaiveDateTime,
    ) -> StoredMessage {
        let body = message.as_bytes().to_vec();
        let body_hash = hex::to_hex(&Challenge::new().chain(body.clone()).finalize());
        StoredMessage {
            id: 1,
            version: 0,
            origin_pubkey: Some(node_identity.public_key().to_hex()),
            message_type: DhtMessageType::None as i32,
            destination_pubkey: None,
            destination_node_id: None,
            header: DhtHeader::from(dht_header).to_encoded_bytes(),
            body,
            is_encrypted: false,
            priority: StoredMessagePriority::High as i32,
            stored_at,
            body_hash,
        }
    }

    #[tokio_macros::test]
    async fn request_stored_messages() {
        let spy = service_spy();
        let (requester, mock_state) = create_store_and_forward_mock();

        let peer_manager = build_peer_manager();
        let (outbound_requester, outbound_mock) = create_outbound_service_mock(10);
        let oms_mock_state = outbound_mock.get_state();
        task::spawn(outbound_mock.run());

        let node_identity = make_node_identity();

        // Recent message
        let (e_sk, e_pk) = make_keypair();
        let dht_header = make_dht_header(
            &node_identity,
            &e_pk,
            &e_sk,
            &[],
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
        );

        let since = Utc::now().checked_sub_signed(chrono::Duration::seconds(60)).unwrap();
        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesRequest::since(since)),
            None,
            make_dht_inbound_message(
                &node_identity,
                b"Keep this for others please".to_vec(),
                DhtMessageFlags::ENCRYPTED,
                true,
            ),
        );
        message.dht_header.message_type = DhtMessageType::SafRequestMessages;

        let (tx, _) = mpsc::channel(1);
        let dht_requester = DhtRequester::new(tx);
        let (saf_response_signal_sender, _saf_response_signal_receiver) = mpsc::channel(20);

        // First test that the task will respond if there are no messages to send.
        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            requester.clone(),
            dht_requester.clone(),
            peer_manager.clone(),
            outbound_requester.clone(),
            node_identity.clone(),
            message.clone(),
            saf_response_signal_sender.clone(),
        );

        task::spawn(task.run());

        for _ in 0..6 {
            if oms_mock_state.call_count() >= 1 {
                break;
            }
            delay_for(Duration::from_secs(5)).await;
        }
        assert_eq!(oms_mock_state.call_count(), 1);

        let call = oms_mock_state.pop_call().unwrap();
        let body = call.1.to_vec();
        let body = EnvelopeBody::decode(body.as_slice()).unwrap();
        let msg = body.decode_part::<StoredMessagesResponse>(0).unwrap().unwrap();
        assert_eq!(msg.messages().len(), 0);
        assert!(!spy.is_called());

        // assert_eq!(mock_state.call_count(), 2);
        let calls = mock_state.take_calls().await;
        let fetch_call = calls.iter().find(|c| c.contains("FetchMessages")).unwrap();
        assert!(fetch_call.contains(node_identity.public_key().to_hex().as_str()));
        assert!(fetch_call.contains(format!("{:?}", since).as_str()));

        let msg1_time = Utc::now()
            .checked_sub_signed(OldDuration::from_std(Duration::from_secs(120)).unwrap())
            .unwrap();
        let msg1 = "one".to_string();
        mock_state
            .add_message(make_stored_message(
                msg1.clone(),
                &node_identity,
                dht_header.clone(),
                msg1_time.naive_utc(),
            ))
            .await;

        let msg2_time = Utc::now()
            .checked_sub_signed(OldDuration::from_std(Duration::from_secs(30)).unwrap())
            .unwrap();
        let msg2 = "two".to_string();
        mock_state
            .add_message(make_stored_message(
                msg2.clone(),
                &node_identity,
                dht_header,
                msg2_time.naive_utc(),
            ))
            .await;

        // Now lets test its response where there are messages to return.
        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            requester,
            dht_requester,
            peer_manager,
            outbound_requester.clone(),
            node_identity.clone(),
            message,
            saf_response_signal_sender,
        );

        task::spawn(task.run());

        for _ in 0..6 {
            if oms_mock_state.call_count() >= 1 {
                break;
            }
            delay_for(Duration::from_secs(5)).await;
        }
        assert_eq!(oms_mock_state.call_count(), 1);
        let call = oms_mock_state.pop_call().unwrap();

        let body = call.1.to_vec();
        let body = EnvelopeBody::decode(body.as_slice()).unwrap();
        let msg = body.decode_part::<StoredMessagesResponse>(0).unwrap().unwrap();

        assert_eq!(msg.messages().len(), 1);
        assert_eq!(msg.messages()[0].body, "two".as_bytes());
        assert!(!spy.is_called());

        assert_eq!(mock_state.call_count(), 2);
        let calls = mock_state.take_calls().await;

        let fetch_call = calls.iter().find(|c| c.contains("FetchMessages")).unwrap();
        assert!(fetch_call.contains(node_identity.public_key().to_hex().as_str()));
        assert!(fetch_call.contains(format!("{:?}", since).as_str()));

        let stored_messages = mock_state.get_messages().await;

        assert!(!stored_messages.iter().any(|s| s.body == msg1.as_bytes()));
        assert!(stored_messages.iter().any(|s| s.body == msg2.as_bytes()));
    }

    #[tokio_macros::test_basic]
    async fn receive_stored_messages() {
        let rt_handle = Handle::current();
        let spy = service_spy();
        let (requester, _) = create_store_and_forward_mock();

        let peer_manager = build_peer_manager();
        let (oms_tx, _) = mpsc::channel(1);

        let node_identity = make_node_identity();

        let msg_a = wrap_in_envelope_body!(&b"A".to_vec()).to_encoded_bytes();

        let inbound_msg_a = make_dht_inbound_message(&node_identity, msg_a.clone(), DhtMessageFlags::ENCRYPTED, true);
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_a.source_peer))
            .await
            .unwrap();

        let msg_b = &wrap_in_envelope_body!(b"B".to_vec()).to_encoded_bytes();
        let inbound_msg_b = make_dht_inbound_message(&node_identity, msg_b.clone(), DhtMessageFlags::ENCRYPTED, true);
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_b.source_peer))
            .await
            .unwrap();

        let msg1_time = Utc::now()
            .checked_sub_signed(OldDuration::from_std(Duration::from_secs(60)).unwrap())
            .unwrap();
        let msg1 = ProtoStoredMessage::new(0, inbound_msg_a.dht_header.clone(), inbound_msg_a.body, msg1_time);
        let msg2_time = Utc::now()
            .checked_sub_signed(OldDuration::from_std(Duration::from_secs(30)).unwrap())
            .unwrap();
        let msg2 = ProtoStoredMessage::new(0, inbound_msg_b.dht_header, inbound_msg_b.body, msg2_time);

        // Cleartext message
        let clear_msg = wrap_in_envelope_body!(b"Clear".to_vec()).to_encoded_bytes();
        let clear_header =
            make_dht_inbound_message(&node_identity, clear_msg.clone(), DhtMessageFlags::empty(), false).dht_header;
        let msg_clear_time = Utc::now()
            .checked_sub_signed(OldDuration::from_std(Duration::from_secs(120)).unwrap())
            .unwrap();
        let msg_clear = ProtoStoredMessage::new(0, clear_header, clear_msg, msg_clear_time);
        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesResponse {
                messages: vec![msg1.clone(), msg2, msg_clear],
                request_id: 123,
                response_type: 0
            }),
            None,
            make_dht_inbound_message(
                &node_identity,
                b"Stored message".to_vec(),
                DhtMessageFlags::ENCRYPTED,
                true,
            ),
        );
        message.dht_header.message_type = DhtMessageType::SafStoredMessages;

        let (mut dht_requester, mock) = create_dht_actor_mock(1);
        rt_handle.spawn(mock.run());
        let (saf_response_signal_sender, mut saf_response_signal_receiver) = mpsc::channel(20);

        assert!(dht_requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::LastSafMessageReceived)
            .await
            .unwrap()
            .is_none());

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            requester,
            dht_requester.clone(),
            peer_manager,
            OutboundMessageRequester::new(oms_tx),
            node_identity,
            message,
            saf_response_signal_sender,
        );

        task.run().await.unwrap();
        assert_eq!(spy.call_count(), 3);
        let requests = spy.take_requests();
        assert_eq!(requests.len(), 3);
        // Deserialize each request into the message (a vec of a single byte in this case)
        let msgs = requests
            .into_iter()
            .map(|req| req.success().unwrap().decode_part::<Vec<_>>(0).unwrap().unwrap())
            .collect::<Vec<Vec<u8>>>();
        assert!(msgs.contains(&b"A".to_vec()));
        assert!(msgs.contains(&b"B".to_vec()));
        assert!(msgs.contains(&b"Clear".to_vec()));
        let signals = collect_stream!(
            saf_response_signal_receiver,
            take = 1,
            timeout = Duration::from_secs(20)
        );
        assert_eq!(signals.len(), 1);

        let last_saf_received = dht_requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::LastSafMessageReceived)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(last_saf_received, msg2_time);
    }
}
