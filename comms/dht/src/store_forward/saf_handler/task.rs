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

use std::{
    cmp,
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use futures::{future, stream, StreamExt};
use log::*;
use prost::Message;
use tari_comms::{
    message::{EnvelopeBody, MessageTag},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerManagerError},
    pipeline::PipelineError,
    types::{CommsDHKE, CommsPublicKey},
    BytesMut,
};
use tari_utilities::{epoch_time::EpochTime, ByteArray};
use tokio::sync::mpsc;
use tower::{Service, ServiceExt};

use crate::{
    actor::{DhtRequester, OffenceSeverity},
    crypt,
    dedup,
    envelope::{epochtime_to_datetime, DhtMessageError, DhtMessageHeader, NodeDestination},
    inbound::{DecryptedDhtMessage, DhtInboundMessage},
    message_signature::{MessageSignature, MessageSignatureError, ProtoMessageSignature},
    outbound::{OutboundMessageRequester, SendMessageParams},
    proto::{
        envelope::DhtMessageType,
        store_forward::{
            stored_messages_response::SafResponseType,
            StoredMessage as ProtoStoredMessage,
            StoredMessagesRequest,
            StoredMessagesResponse,
        },
    },
    storage::DhtMetadataKey,
    store_forward::{
        error::StoreAndForwardError,
        service::FetchStoredMessageQuery,
        SafConfig,
        StoreAndForwardRequester,
    },
};

const LOG_TARGET: &str = "comms::dht::storeforward::handler";

pub struct MessageHandlerTask<S> {
    config: SafConfig,
    next_service: S,
    dht_requester: DhtRequester,
    outbound_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    message: Option<DecryptedDhtMessage>,
    saf_requester: StoreAndForwardRequester,
    saf_response_signal_sender: mpsc::Sender<()>,
}

impl<S> MessageHandlerTask<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    pub fn new(
        config: SafConfig,
        next_service: S,
        saf_requester: StoreAndForwardRequester,
        dht_requester: DhtRequester,
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
        debug!(
            target: LOG_TARGET,
            "Received request for stored message {} from {} (Trace: {})",
            message.tag,
            message.source_peer.node_id,
            message.dht_header.message_tag
        );
        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");

        let retrieve_msgs = msg
            .decode_part::<StoredMessagesRequest>(0)?
            .ok_or(StoreAndForwardError::EnvelopeBodyMissingMessagePart)?;

        let source_pubkey = Box::new(message.source_peer.public_key.clone());
        let source_node_id = Box::new(message.source_peer.node_id.clone());

        // Compile a set of stored messages for the requesting peer
        let mut query = FetchStoredMessageQuery::new(source_pubkey, source_node_id.clone());

        let max = u32::try_from(self.config.max_returned_messages).unwrap_or_else(|_| {
            warn!(target: LOG_TARGET,"Your node is configured with an extremely high number for max_returned_messages. This will likely be disregarded by peers.");
            u32::MAX
         });
        // limit of 0 means no hard limit, though we still limit to our configured limit
        if retrieve_msgs.limit == 0 {
            query.with_limit(max);
        } else {
            // Return up to the limit. The limit cannot exceed our locally configured max_returned_messages setting.
            // Returning less than requested is completely expected.
            query.with_limit(cmp::min(retrieve_msgs.limit, max));
        }

        let since = match retrieve_msgs.since {
            0 => None,
            since => {
                let since = epochtime_to_datetime(EpochTime::from_secs_since_epoch(since));
                debug!(
                    target: LOG_TARGET,
                    "Peer '{}' requested all messages since '{}'",
                    source_node_id.short_str(),
                    since
                );
                query.with_messages_since(since);
                Some(since)
            },
        };

        let response_types = vec![SafResponseType::ForMe];

        for resp_type in response_types {
            query.with_response_type(resp_type);
            let messages = self.saf_requester.fetch_messages(query.clone()).await?;

            let stored_messages = StoredMessagesResponse {
                messages: messages.into_iter().map(TryInto::try_into).collect::<Result<_, _>>()?,
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
                .send_message_no_header_no_wait(
                    SendMessageParams::new()
                        .direct_public_key(message.source_peer.public_key.clone())
                        .with_dht_message_type(DhtMessageType::SafStoredMessages)
                        .finish(),
                    stored_messages,
                )
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

        if let Err(err) = self.check_saf_messages_were_requested(&source_node_id).await {
            // Peer sent SAF messages we didn't request, it was cancelled locally or sent it more than 4 to 10 minutes
            // late?? #banheuristics
            warn!(target: LOG_TARGET, "SAF response check failed: {}", err);
            return Ok(());
        }

        let msg = message
            .success()
            .expect("already checked that this message decrypted successfully");
        let response = msg
            .decode_part::<StoredMessagesResponse>(0)?
            .ok_or(StoreAndForwardError::EnvelopeBodyMissingMessagePart)?;

        if response.messages.len() > self.config.max_returned_messages {
            warn!(
                target: LOG_TARGET,
                "Peer '{}' sent {} stored messages which is more than the maximum allowed of {}. Discarding \
                 messages.",
                source_node_id.short_str(),
                response.messages.len(),
                self.config.max_returned_messages
            );
            self.dht_requester
                .ban_peer(
                    message.source_peer.public_key.clone(),
                    OffenceSeverity::High,
                    format!(
                        "Peer sent too many stored messages ({} of {})",
                        response.messages.len(),
                        self.config.max_returned_messages
                    ),
                )
                .await;
            return Ok(());
        }

        let source_peer = message.source_peer.clone();

        debug!(
            target: LOG_TARGET,
            "Received {} stored messages of type {} from peer `{}` (Trace: {})",
            response.messages().len(),
            SafResponseType::try_from(response.response_type).ok()
                .as_ref()
                .map(|t| format!("{:?}", t))
                .unwrap_or_else(|| "<Invalid>".to_string()),
            source_node_id,
            message_tag
        );

        let successful_messages = self
            .process_incoming_stored_messages(source_peer.clone(), response.messages)
            .await?;

        // Let the SAF Service know we got a SAF response.
        let _ = self
            .saf_response_signal_sender
            .send(())
            .await
            .map_err(|e| warn!(target: LOG_TARGET, "Error sending SAF response signal; {:?}", e));

        self.next_service
            .call_all(stream::iter(successful_messages))
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

    async fn process_incoming_stored_messages(
        &mut self,
        source_peer: Arc<Peer>,
        messages: Vec<ProtoStoredMessage>,
    ) -> Result<Vec<DecryptedDhtMessage>, StoreAndForwardError> {
        let mut last_saf_received = self
            .dht_requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::LastSafMessageReceived)
            .await?;

        // Allocations: the number of messages has already been bounds checked to be <=
        // self.config.max_returned_messages
        let mut results = Vec::with_capacity(messages.len());
        for msg in messages {
            let result = self
                .validate_and_decrypt_incoming_stored_message(Arc::clone(&source_peer), msg)
                .await;

            let Some(result) = self
                .process_saf_message_validation_result(&source_peer.public_key, result)
                .await
            else {
                // Logging of problems and banning are done inside process_saf_message. We can simply continue
                continue;
            };

            // If the messages should no longer be processed because we banned the peer, we exit here on Err
            let (msg, stored_at) = result?;

            if last_saf_received.as_ref().map(|dt| stored_at > *dt).unwrap_or(true) {
                last_saf_received = Some(stored_at);
            }

            results.push(msg);
        }

        if let Some(last_saf_received) = last_saf_received {
            self.dht_requester
                .set_metadata(DhtMetadataKey::LastSafMessageReceived, last_saf_received)
                .await?;
        }

        Ok(results)
    }

    async fn validate_and_decrypt_incoming_stored_message(
        &mut self,
        source_peer: Arc<Peer>,
        message: ProtoStoredMessage,
    ) -> Result<(DecryptedDhtMessage, DateTime<Utc>), StoreAndForwardError> {
        let node_identity = &self.node_identity;
        let Some(dht_header) = message.dht_header else {
            return Err(StoreAndForwardError::DhtMessageError(DhtMessageError::HeaderOmitted));
        };

        if message.body.is_empty() {
            return Err(StoreAndForwardError::DhtMessageError(DhtMessageError::BodyEmpty));
        }

        let stored_at = epochtime_to_datetime(EpochTime::from_secs_since_epoch(message.stored_at));
        if stored_at > Utc::now() {
            return Err(StoreAndForwardError::StoredAtWasInFuture);
        }

        let msg_hash = dedup::create_message_hash(&dht_header.message_signature, &message.body);

        let dht_header: DhtMessageHeader = dht_header.try_into().map_err(StoreAndForwardError::DhtMessageError)?;

        if !dht_header.is_semantically_valid() {
            return Err(StoreAndForwardError::BadDhtHeaderSemanticallyInvalid);
        }
        let message_type = dht_header.message_type;

        if message_type.is_dht_message() {
            debug!(
                target: LOG_TARGET,
                "Discarding {} message from peer '{}'",
                message_type,
                source_peer.node_id.short_str()
            );
            return Err(StoreAndForwardError::PeerSentDhtMessageViaSaf);
        }

        if message_type.is_saf_message() {
            debug!(
                target: LOG_TARGET,
                "Discarding {} message from peer '{}'",
                message_type,
                source_peer.node_id.short_str()
            );
            return Err(StoreAndForwardError::PeerSentSafMessageViaSaf);
        }

        // Check that the destination is either undisclosed, for us or for our network region
        Self::check_destination_for(node_identity.public_key(), &dht_header).await?;

        // Attempt to decrypt the message (if applicable), and deserialize it
        let (authenticated_pk, decrypted_body) =
            Self::authenticate_and_decrypt_if_required(node_identity, &dht_header, &message.body)?;

        // Check that the message has not already been received.
        Self::check_duplicate(
            &mut self.dht_requester,
            msg_hash.to_vec(),
            source_peer.public_key.clone(),
        )
        .await?;

        let mut inbound_msg =
            DhtInboundMessage::new(MessageTag::new(), dht_header, Arc::clone(&source_peer), message.body);
        inbound_msg.is_saf_message = true;

        Ok((
            DecryptedDhtMessage::succeeded(decrypted_body, authenticated_pk, inbound_msg),
            stored_at,
        ))
    }

    async fn check_duplicate(
        dht_requester: &mut DhtRequester,
        msg_hash: Vec<u8>,
        public_key: CommsPublicKey,
    ) -> Result<(), StoreAndForwardError> {
        let hit_count = dht_requester.add_message_to_dedup_cache(msg_hash, public_key).await?;
        if hit_count > 1 {
            Err(StoreAndForwardError::DuplicateMessage)
        } else {
            Ok(())
        }
    }

    async fn check_destination_for(
        public_key: &CommsPublicKey,
        dht_header: &DhtMessageHeader,
    ) -> Result<(), StoreAndForwardError> {
        let is_valid_destination = match &dht_header.destination {
            NodeDestination::Unknown => true,
            NodeDestination::PublicKey(pk) => *public_key == **pk,
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
        if header.flags.is_encrypted() {
            let ephemeral_public_key = header.ephemeral_public_key.as_ref().expect(
                "[store and forward] DHT header is invalid after validity check because it did not contain an \
                 ephemeral_public_key",
            );

            trace!(
                target: LOG_TARGET,
                "Attempting to decrypt message signature ({} byte(s))",
                header.message_signature.len()
            );
            let masked_sender_public_key = Self::authenticate_message(&header.message_signature, header, body)?;

            trace!(
                target: LOG_TARGET,
                "Attempting to decrypt message body ({} byte(s))",
                body.len()
            );

            let shared_ephemeral_secret = CommsDHKE::new(node_identity.secret_key(), ephemeral_public_key);
            let key_message = crypt::generate_key_message(&shared_ephemeral_secret);
            let mut decrypted_bytes = BytesMut::from(body);
            crypt::decrypt_message(&key_message, &mut decrypted_bytes, masked_sender_public_key.as_bytes())?;
            let envelope_body =
                EnvelopeBody::decode(decrypted_bytes.freeze()).map_err(|_| StoreAndForwardError::DecryptionFailed)?;
            if envelope_body.is_empty() {
                return Err(StoreAndForwardError::EnvelopeBodyMissingMessagePart);
            }

            // Unmask the sender public key
            let mask = crypt::generate_key_mask(&shared_ephemeral_secret)
                .map_err(|e| StoreAndForwardError::InvariantError(e.to_string()))?;
            let mask_inverse = mask.invert().ok_or(StoreAndForwardError::DecryptionFailed)?;
            Ok((Some(mask_inverse * masked_sender_public_key), envelope_body))
        } else {
            let authenticated_pk = if header.message_signature.is_empty() {
                None
            } else {
                Some(Self::authenticate_message(&header.message_signature, header, body)?)
            };
            let envelope_body = EnvelopeBody::decode(body).map_err(StoreAndForwardError::MalformedEnvelopeBody)?;
            Ok((authenticated_pk, envelope_body))
        }
    }

    fn authenticate_message(
        cleartext_message_signature_body: &[u8],
        header: &DhtMessageHeader,
        body: &[u8],
    ) -> Result<CommsPublicKey, StoreAndForwardError> {
        let message_signature = ProtoMessageSignature::decode(cleartext_message_signature_body)?;
        let message_signature = MessageSignature::try_from(message_signature)?;

        let binding_message_representation = crypt::create_message_domain_separated_hash(header, body);

        if message_signature.verify(&binding_message_representation) {
            Ok(message_signature.into_signer_public_key())
        } else {
            Err(StoreAndForwardError::InvalidMessageSignature(
                MessageSignatureError::VerificationFailed,
            ))
        }
    }

    async fn check_saf_messages_were_requested(&mut self, peer: &NodeId) -> Result<(), StoreAndForwardError> {
        match self.saf_requester.mark_saf_response_received(peer.clone()).await? {
            Some(age) if age <= self.config.max_inflight_request_age => Ok(()),
            Some(age) => Err(StoreAndForwardError::SafMessagesReceivedAfterDeadline {
                peer: peer.clone(),
                message_age: age,
            }),
            None => Err(StoreAndForwardError::ReceivedUnrequestedSafMessages),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn process_saf_message_validation_result<T>(
        &mut self,
        source_peer: &CommsPublicKey,
        result: Result<T, StoreAndForwardError>,
    ) -> Option<Result<T, StoreAndForwardError>> {
        match result {
            Ok(t) => Some(Ok(t)),
            // Failed decryption is acceptable, the message wasn't for this node so we
            // simply discard the message.
            Err(err @ StoreAndForwardError::DhtEncryptError(_)) | Err(err @ StoreAndForwardError::DecryptionFailed) => {
                debug!(
                    target: LOG_TARGET,
                    "Unable to decrypt stored message sent by {}: {}",
                    source_peer,
                    err
                );
                None
            },
            // The peer that originally sent this message is not known to us.
            Err(StoreAndForwardError::PeerManagerError(PeerManagerError::PeerNotFoundError)) => {
                debug!(target: LOG_TARGET, "Origin peer not found. Discarding stored message.");
                None
            },
            Err(StoreAndForwardError::PeerManagerError(PeerManagerError::BannedPeer)) => {
                debug!(target: LOG_TARGET, "Origin peer was banned. Discarding stored message.");
                None
            },

            // These aren't be possible in this function if the code is correct.
            Err(err @ StoreAndForwardError::InvariantError(_)) |
            Err(err @ StoreAndForwardError::SafMessagesReceivedAfterDeadline { .. }) |
            Err(err @ StoreAndForwardError::ReceivedUnrequestedSafMessages) => {
                error!(target: LOG_TARGET, "BUG: unreachable error reached! {}", err);
                None
            },

            // Internal errors
            Err(err @ StoreAndForwardError::RequestCancelled) |
            Err(err @ StoreAndForwardError::RequesterChannelClosed) |
            Err(err @ StoreAndForwardError::DhtOutboundError(_)) |
            Err(err @ StoreAndForwardError::StorageError(_)) |
            Err(err @ StoreAndForwardError::PeerManagerError(_)) |
            Err(err @ StoreAndForwardError::ConnectivityError(_)) => {
                error!(target: LOG_TARGET, "Internal error: {}", err);
                None
            },

            // Failed to send request to Dht Actor, something has gone very wrong
            Err(StoreAndForwardError::DhtActorError(err)) => {
                error!(
                    target: LOG_TARGET,
                    "DhtActor returned an error. {}. This could indicate a system malfunction.", err
                );
                None
            },
            // Duplicate message detected, no problem it happens.
            Err(StoreAndForwardError::DuplicateMessage) => {
                debug!(
                    target: LOG_TARGET,
                    "Store and forward received a duplicate message. Message discarded."
                );
                None
            },

            // The decrypted message did not contain a required message part. The sender has no way to know this
            // so we can just ignore the message
            Err(StoreAndForwardError::EnvelopeBodyMissingMessagePart) => {
                debug!(
                    target: LOG_TARGET,
                    "Received stored message from peer `{}` that is missing a required message part. Message \
                     discarded.",
                    source_peer
                );
                None
            },

            // Peer sent an invalid SAF reply
            Err(err @ StoreAndForwardError::StoredAtWasInFuture) |
            Err(err @ StoreAndForwardError::InvalidSafResponseMessage { .. }) => {
                warn!(
                    target: LOG_TARGET,
                    "SECURITY: invalid store and forward message was discarded from NodeId={}. Reason: {}. \
                     This is a sign of a badly behaving node.",
                    source_peer,
                    err
                );
                self.dht_requester
                    .ban_peer(source_peer.clone(), OffenceSeverity::High, &err)
                    .await;
                Some(Err(err))
            },

            // Ban - peer sent us a message containing an invalid DhtHeader or encoded signature. They should
            // have discarded this message.
            Err(err @ StoreAndForwardError::DecodeError(_)) |
            Err(err @ StoreAndForwardError::MessageError(_)) |
            Err(err @ StoreAndForwardError::MalformedEnvelopeBody(_)) |
            Err(err @ StoreAndForwardError::DhtMessageError(_)) => {
                warn!(
                    target: LOG_TARGET,
                    "SECURITY: invalid store and forward message was discarded from NodeId={}. Reason: {}. \
                     These messages should never have been forwarded. This is a sign of a badly behaving node.",
                    source_peer,
                    err
                );
                self.dht_requester
                    .ban_peer(source_peer.clone(), OffenceSeverity::Medium, &err)
                    .await;
                Some(Err(err))
            },

            Err(err @ StoreAndForwardError::BadDhtHeaderSemanticallyInvalid) |
            Err(err @ StoreAndForwardError::InvalidMessageSignature(_)) => {
                warn!(
                    target: LOG_TARGET,
                    "SECURITY: invalid store and forward message was discarded from NodeId={}. Reason: {}. \
                     These messages should never have been forwarded. This is a sign of a badly behaving node.",
                    source_peer,
                    err
                );
                self.dht_requester
                    .ban_peer(source_peer.clone(), OffenceSeverity::High, &err)
                    .await;
                Some(Err(err))
            },

            // The destination for this message is not this node, so the sender should not have sent it
            Err(err @ StoreAndForwardError::InvalidDestination) => {
                warn!(
                    target: LOG_TARGET,
                    "SECURITY: invalid store and forward message was discarded from NodeId={}. Reason: {}. \
                     These messages should never have been forwarded. This is a sign of a badly behaving node.",
                    source_peer,
                    err
                );
                self.dht_requester
                    .ban_peer(source_peer.clone(), OffenceSeverity::High, &err)
                    .await;
                Some(Err(err))
            },
            Err(err @ StoreAndForwardError::PeerSentDhtMessageViaSaf) |
            Err(err @ StoreAndForwardError::PeerSentSafMessageViaSaf) => {
                warn!(
                    target: LOG_TARGET,
                    "SECURITY: invalid store and forward message was discarded from NodeId={}. Reason: {}. \
                     These messages should never have been forwarded. This is a sign of a badly behaving node.",
                    source_peer,
                    err
                );
                self.dht_requester
                    .ban_peer(source_peer.clone(), OffenceSeverity::High, &err)
                    .await;
                Some(Err(err))
            },
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use chrono::Timelike;
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tari_test_utils::collect_recv;
    use tari_utilities::{hex, hex::Hex};
    use tokio::{task, time::sleep};

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

    fn make_stored_message(
        message: String,
        node_identity: &NodeIdentity,
        dht_header: DhtMessageHeader,
        stored_at: chrono::NaiveDateTime,
    ) -> StoredMessage {
        let msg_hash = hex::to_hex(&dedup::create_message_hash(
            &dht_header.message_signature,
            message.as_bytes(),
        ));
        let body = message.into_bytes();
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
            body_hash: msg_hash,
        }
    }

    #[tokio::test]
    #[allow(clippy::similar_names)]
    #[allow(clippy::too_many_lines)]
    async fn request_stored_messages() {
        let spy = service_spy();
        let (requester, mock_state) = create_store_and_forward_mock();

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
            false,
        )
        .unwrap();

        let since = Utc::now()
            .checked_sub_signed(chrono::Duration::seconds(60))
            .map(|d| d.with_nanosecond(0).unwrap())
            .unwrap();
        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesRequest::since(since)),
            None,
            make_dht_inbound_message(
                &node_identity,
                &b"Keep this for others please".to_vec(),
                DhtMessageFlags::ENCRYPTED,
                true,
                false,
            )
            .unwrap(),
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
            outbound_requester.clone(),
            node_identity.clone(),
            message.clone(),
            saf_response_signal_sender.clone(),
        );

        task::spawn(task.run());

        oms_mock_state
            .wait_call_count(1, Duration::from_secs(10))
            .await
            .unwrap();

        let (_, body) = oms_mock_state.pop_call().await.unwrap();
        let body = EnvelopeBody::decode(body.as_ref()).unwrap();
        let msg = body.decode_part::<StoredMessagesResponse>(0).unwrap().unwrap();
        assert_eq!(msg.messages().len(), 0);
        assert!(!spy.is_called());

        // assert_eq!(mock_state.call_count(), 2);
        let calls = mock_state.take_calls().await;
        let fetch_call = calls.iter().find(|c| c.contains("FetchMessages")).unwrap();
        assert!(fetch_call.contains(node_identity.public_key().to_hex().as_str()));
        assert!(fetch_call.contains(format!("{:?}", since).as_str()));

        let msg1_time = Utc::now()
            .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(120)).unwrap())
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
            .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(30)).unwrap())
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
            outbound_requester.clone(),
            node_identity.clone(),
            message,
            saf_response_signal_sender,
        );

        task::spawn(task.run());

        for _ in 0..6 {
            if oms_mock_state.call_count().await >= 1 {
                break;
            }
            sleep(Duration::from_secs(5)).await;
        }
        assert_eq!(oms_mock_state.call_count().await, 1);
        let (_, body) = oms_mock_state.pop_call().await.unwrap();

        let body = EnvelopeBody::decode(body).unwrap();
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

    #[tokio::test]
    #[allow(clippy::similar_names, clippy::too_many_lines)]
    async fn receive_stored_messages() {
        let spy = service_spy();
        let (saf_requester, saf_mock_state) = create_store_and_forward_mock();

        let peer_manager = build_peer_manager();
        let (oms_tx, _) = mpsc::channel(1);

        let node_identity = make_node_identity();

        let msg_a = wrap_in_envelope_body!(&b"A".to_vec());

        let inbound_msg_a =
            make_dht_inbound_message(&node_identity, &msg_a, DhtMessageFlags::ENCRYPTED, true, true).unwrap();
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_a.source_peer))
            .await
            .unwrap();

        let msg_b = wrap_in_envelope_body!(b"B".to_vec());
        let inbound_msg_b =
            make_dht_inbound_message(&node_identity, &msg_b, DhtMessageFlags::ENCRYPTED, true, true).unwrap();
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_b.source_peer))
            .await
            .unwrap();

        let msg1_time = Utc::now()
            .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(60)).unwrap())
            .unwrap();
        let msg1 = ProtoStoredMessage::new(0, inbound_msg_a.dht_header.clone(), inbound_msg_a.body, msg1_time);
        let msg2_time = Utc::now()
            .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(30)).unwrap())
            .unwrap();
        let msg2 = ProtoStoredMessage::new(0, inbound_msg_b.dht_header, inbound_msg_b.body, msg2_time);

        // Cleartext message
        let clear_msg = wrap_in_envelope_body!(b"Clear".to_vec());
        let clear_header = make_dht_inbound_message(&node_identity, &clear_msg, DhtMessageFlags::empty(), false, false)
            .unwrap()
            .dht_header;
        let msg_clear_time = Utc::now()
            .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(120)).unwrap())
            .unwrap();
        let msg_clear = ProtoStoredMessage::new(0, clear_header, clear_msg.to_encoded_bytes(), msg_clear_time);
        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesResponse {
                messages: vec![msg1.clone(), msg2, msg_clear],
                request_id: 123,
                response_type: 0
            }),
            None,
            make_dht_inbound_message(
                &node_identity,
                &b"Stored message".to_vec(),
                DhtMessageFlags::ENCRYPTED,
                true,
                false,
            )
            .unwrap(),
        );
        message.dht_header.message_type = DhtMessageType::SafStoredMessages;

        let (mut dht_requester, mock) = create_dht_actor_mock(1);
        task::spawn(mock.run());
        let (saf_response_signal_sender, mut saf_response_signal_receiver) = mpsc::channel(20);

        assert!(dht_requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::LastSafMessageReceived)
            .await
            .unwrap()
            .is_none());

        // Allow request inflight check to pass
        saf_mock_state.set_request_inflight(Some(Duration::from_secs(10))).await;

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            saf_requester,
            dht_requester.clone(),
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
        let signals = collect_recv!(
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

        assert_eq!(last_saf_received.second(), msg2_time.second());
    }

    #[tokio::test]
    #[allow(clippy::similar_names, clippy::too_many_lines)]
    async fn rejected_with_bad_message_semantics() {
        let spy = service_spy();
        let (saf_requester, saf_mock_state) = create_store_and_forward_mock();

        let peer_manager = build_peer_manager();
        let (oms_tx, _) = mpsc::channel(1);

        let node_identity = make_node_identity();

        let msg_a = wrap_in_envelope_body!(&b"A".to_vec());

        let inbound_msg_a =
            make_dht_inbound_message(&node_identity, &msg_a, DhtMessageFlags::ENCRYPTED, true, false).unwrap();
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_a.source_peer))
            .await
            .unwrap();

        let msg_b = wrap_in_envelope_body!(b"B".to_vec());
        let inbound_msg_b =
            make_dht_inbound_message(&node_identity, &msg_b, DhtMessageFlags::ENCRYPTED, false, true).unwrap();
        // Need to know the peer to process a stored message
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_b.source_peer))
            .await
            .unwrap();

        let msg1_time = Utc::now()
            .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(60)).unwrap())
            .unwrap();
        let msg1 = ProtoStoredMessage::new(0, inbound_msg_a.dht_header.clone(), inbound_msg_a.body, msg1_time);
        let msg2_time = Utc::now()
            .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(30)).unwrap())
            .unwrap();
        let msg2 = ProtoStoredMessage::new(0, inbound_msg_b.dht_header, inbound_msg_b.body, msg2_time);

        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesResponse {
                messages: vec![msg1.clone(), msg2],
                request_id: 123,
                response_type: 0
            }),
            None,
            make_dht_inbound_message(
                &node_identity,
                &b"Stored message".to_vec(),
                DhtMessageFlags::ENCRYPTED,
                false,
                false,
            )
            .unwrap(),
        );
        message.dht_header.message_type = DhtMessageType::SafStoredMessages;

        let (mut dht_requester, mock) = create_dht_actor_mock(1);
        task::spawn(mock.run());
        let (saf_response_signal_sender, _) = mpsc::channel(20);

        assert!(dht_requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::LastSafMessageReceived)
            .await
            .unwrap()
            .is_none());

        // Allow request inflight check to pass
        saf_mock_state.set_request_inflight(Some(Duration::from_secs(10))).await;

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            saf_requester,
            dht_requester.clone(),
            OutboundMessageRequester::new(oms_tx),
            node_identity,
            message,
            saf_response_signal_sender,
        );

        let err = task.run().await.unwrap_err();
        matches!(
            err.downcast_ref::<StoreAndForwardError>().unwrap(),
            StoreAndForwardError::BadDhtHeaderSemanticallyInvalid
        );

        assert_eq!(spy.call_count(), 0);
        let requests = spy.take_requests();
        assert_eq!(requests.len(), 0);
    }

    #[tokio::test]
    async fn stored_at_in_future() {
        let spy = service_spy();
        let (requester, _) = create_store_and_forward_mock();

        let peer_manager = build_peer_manager();
        let (oms_tx, _) = mpsc::channel(1);

        let node_identity = make_node_identity();

        let msg_a = wrap_in_envelope_body!(&b"A".to_vec());
        let inbound_msg_a =
            make_dht_inbound_message(&node_identity, &msg_a, DhtMessageFlags::ENCRYPTED, true, false).unwrap();
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_a.source_peer))
            .await
            .unwrap();

        let msg1 = ProtoStoredMessage::new(
            0,
            inbound_msg_a.dht_header.clone(),
            inbound_msg_a.body,
            Utc::now() + chrono::Duration::days(1),
        );
        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesResponse {
                messages: vec![msg1.clone()],
                request_id: 123,
                response_type: 0
            }),
            None,
            make_dht_inbound_message(
                &node_identity,
                &b"Stored message".to_vec(),
                DhtMessageFlags::ENCRYPTED,
                true,
                false,
            )
            .unwrap(),
        );
        message.dht_header.message_type = DhtMessageType::SafStoredMessages;

        let (mut dht_requester, mock) = create_dht_actor_mock(1);
        task::spawn(mock.run());

        let (saf_response_signal_sender, _) = mpsc::channel(1);

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            requester,
            dht_requester.clone(),
            OutboundMessageRequester::new(oms_tx),
            node_identity,
            message,
            saf_response_signal_sender,
        );

        task.run().await.unwrap();
        let requests = spy.take_requests();
        // Message was discarded
        assert_eq!(spy.call_count(), 0);
        assert_eq!(requests.len(), 0);

        let last_saf_received = dht_requester
            .get_metadata::<DateTime<Utc>>(DhtMetadataKey::LastSafMessageReceived)
            .await
            .unwrap();

        // LastSafMessageReceived was not set at all
        assert!(last_saf_received.is_none());
    }

    #[tokio::test]
    async fn saf_message_was_requested() {
        let spy = service_spy();
        let (saf_requester, saf_mock_state) = create_store_and_forward_mock();

        let peer_manager = build_peer_manager();
        let (oms_tx, _) = mpsc::channel(1);

        let node_identity = make_node_identity();

        let msg_a = wrap_in_envelope_body!(&b"A".to_vec());
        let inbound_msg_a =
            make_dht_inbound_message(&node_identity, &msg_a, DhtMessageFlags::ENCRYPTED, true, true).unwrap();
        peer_manager
            .add_peer(Clone::clone(&*inbound_msg_a.source_peer))
            .await
            .unwrap();

        let msg1 = ProtoStoredMessage::new(
            0,
            inbound_msg_a.dht_header.clone(),
            inbound_msg_a.body,
            Utc::now() - chrono::Duration::days(1),
        );
        let mut message = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(StoredMessagesResponse {
                messages: vec![msg1.clone()],
                request_id: 123,
                response_type: 0
            }),
            None,
            make_dht_inbound_message(
                &node_identity,
                &b"Stored message".to_vec(),
                DhtMessageFlags::ENCRYPTED,
                true,
                false,
            )
            .unwrap(),
        );
        message.dht_header.message_type = DhtMessageType::SafStoredMessages;

        let (dht_requester, mock) = create_dht_actor_mock(1);
        task::spawn(mock.run());

        let (saf_response_signal_sender, _) = mpsc::channel(1);

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            saf_requester.clone(),
            dht_requester.clone(),
            OutboundMessageRequester::new(oms_tx.clone()),
            node_identity.clone(),
            message.clone(),
            saf_response_signal_sender.clone(),
        );

        task.run().await.unwrap();
        let requests = spy.take_requests();
        // Message was discarded
        assert_eq!(spy.call_count(), 0);
        assert_eq!(requests.len(), 0);

        // The SAF request was made
        saf_mock_state.set_request_inflight(Some(Duration::from_secs(0))).await;

        let task = MessageHandlerTask::new(
            Default::default(),
            spy.to_service::<PipelineError>(),
            saf_requester,
            dht_requester,
            OutboundMessageRequester::new(oms_tx),
            node_identity,
            message,
            saf_response_signal_sender,
        );

        task.run().await.unwrap();
        let requests = spy.take_requests();
        // Message was discarded
        assert_eq!(spy.call_count(), 1);
        assert_eq!(requests.len(), 1);
    }
}
