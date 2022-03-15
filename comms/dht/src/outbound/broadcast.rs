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

use std::{sync::Arc, task::Poll};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use digest::Digest;
use futures::{
    future,
    future::BoxFuture,
    stream::{self, StreamExt},
    task::Context,
};
use log::*;
use rand::rngs::OsRng;
use tari_comms::{
    message::{MessageExt, MessageTag},
    peer_manager::{NodeId, NodeIdentity, Peer},
    pipeline::PipelineError,
    types::{Challenge, CommsPublicKey},
    utils::signature,
};
use tari_crypto::{
    keys::PublicKey,
    tari_utilities::{epoch_time::EpochTime, message_format::MessageFormat, ByteArray},
};
use tari_utilities::hex::Hex;
use tokio::sync::oneshot;
use tower::{layer::Layer, Service, ServiceExt};

use super::{error::DhtOutboundError, message::DhtOutboundRequest};
use crate::{
    actor::DhtRequester,
    broadcast_strategy::BroadcastStrategy,
    crypt,
    discovery::DhtDiscoveryRequester,
    envelope::{datetime_to_epochtime, datetime_to_timestamp, DhtMessageFlags, DhtMessageHeader, NodeDestination},
    outbound::{
        message::{DhtOutboundMessage, OutboundEncryption, SendFailure},
        message_params::FinalSendMessageParams,
        message_send_state::MessageSendState,
        SendMessageResponse,
    },
    proto::envelope::{DhtMessageType, OriginMac},
    version::DhtProtocolVersion,
    DhtConfig,
};

const LOG_TARGET: &str = "comms::dht::outbound::broadcast_middleware";

pub struct BroadcastLayer {
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    node_identity: Arc<NodeIdentity>,
    message_validity_window: chrono::Duration,
    protocol_version: DhtProtocolVersion,
}

impl BroadcastLayer {
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        config: &DhtConfig,
    ) -> Self {
        BroadcastLayer {
            dht_requester,
            dht_discovery_requester,
            node_identity,
            message_validity_window: chrono::Duration::from_std(config.saf_config.msg_validity)
                .expect("message_validity_window is too large"),
            protocol_version: config.protocol_version,
        }
    }
}

impl<S> Layer<S> for BroadcastLayer {
    type Service = BroadcastMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        BroadcastMiddleware::new(
            service,
            Arc::clone(&self.node_identity),
            self.dht_requester.clone(),
            self.dht_discovery_requester.clone(),
            self.message_validity_window,
            self.protocol_version,
        )
    }
}

/// Responsible for constructing messages using a broadcast strategy and passing them on to
/// the worker task.
#[derive(Clone)]
pub struct BroadcastMiddleware<S> {
    next_service: S,
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    node_identity: Arc<NodeIdentity>,
    message_validity_window: chrono::Duration,
    protocol_version: DhtProtocolVersion,
}

impl<S> BroadcastMiddleware<S> {
    pub fn new(
        service: S,
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        message_validity_window: chrono::Duration,
        protocol_version: DhtProtocolVersion,
    ) -> Self {
        Self {
            next_service: service,
            dht_requester,
            dht_discovery_requester,
            node_identity,
            message_validity_window,
            protocol_version,
        }
    }
}

impl<S> Service<DhtOutboundRequest> for BroadcastMiddleware<S>
where
    S: Service<DhtOutboundMessage, Response = (), Error = PipelineError> + Clone + Send + 'static,
    S::Future: Send,
{
    type Error = PipelineError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtOutboundRequest) -> Self::Future {
        Box::pin(
            BroadcastTask::new(
                self.next_service.clone(),
                Arc::clone(&self.node_identity),
                self.dht_requester.clone(),
                self.dht_discovery_requester.clone(),
                msg,
                self.message_validity_window,
                self.protocol_version,
            )
            .handle(),
        )
    }
}

struct BroadcastTask<S> {
    service: S,
    node_identity: Arc<NodeIdentity>,
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    request: Option<DhtOutboundRequest>,
    message_validity_window: chrono::Duration,
    protocol_version: DhtProtocolVersion,
}
type FinalMessageParts = (Option<Arc<CommsPublicKey>>, Option<Bytes>, Bytes);

impl<S> BroadcastTask<S>
where S: Service<DhtOutboundMessage, Response = (), Error = PipelineError>
{
    pub fn new(
        service: S,
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        request: DhtOutboundRequest,
        message_validity_window: chrono::Duration,
        protocol_version: DhtProtocolVersion,
    ) -> Self {
        Self {
            service,
            node_identity,
            dht_requester,
            dht_discovery_requester,
            request: Some(request),
            message_validity_window,
            protocol_version,
        }
    }

    pub async fn handle(mut self) -> Result<(), PipelineError> {
        let request = self.request.take().expect("request cannot be None");
        debug!(target: LOG_TARGET, "Processing outbound request {}", request);
        let messages = self.generate_outbound_messages(request).await?;
        trace!(
            target: LOG_TARGET,
            "Passing {} message(s) to next_service",
            messages.len()
        );

        self.service
            .call_all(stream::iter(messages))
            .unordered()
            .filter_map(|result| future::ready(result.err()))
            .for_each(|err| {
                warn!(target: LOG_TARGET, "Error when sending broadcast messages: {}", err);
                future::ready(())
            })
            .await;

        Ok(())
    }

    pub async fn generate_outbound_messages(
        &mut self,
        msg: DhtOutboundRequest,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError> {
        match msg {
            DhtOutboundRequest::SendMessage(params, body, reply_tx) => {
                self.handle_send_message(*params, body, reply_tx).await
            },
        }
    }

    async fn handle_send_message(
        &mut self,
        params: FinalSendMessageParams,
        body: Bytes,
        reply_tx: oneshot::Sender<SendMessageResponse>,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError> {
        trace!(target: LOG_TARGET, "Send params: {:?}", params);
        if params
            .broadcast_strategy
            .direct_public_key()
            .filter(|pk| *pk == self.node_identity.public_key())
            .is_some()
        {
            warn!(target: LOG_TARGET, "Attempt to send a message to ourselves");
            let _ = reply_tx.send(SendMessageResponse::Failed(SendFailure::SendToOurselves));
            return Err(DhtOutboundError::SendToOurselves);
        }

        let FinalSendMessageParams {
            broadcast_strategy,
            destination,
            dht_message_type,
            dht_message_flags,
            encryption,
            is_discovery_enabled,
            force_origin,
            dht_header,
            tag,
        } = params;

        match self.select_peers(broadcast_strategy.clone()).await {
            Ok(mut peers) => {
                if reply_tx.is_closed() {
                    return Err(DhtOutboundError::ReplyChannelCanceled);
                }

                let mut reply_tx = Some(reply_tx);

                trace!(
                    target: LOG_TARGET,
                    "Number of peers selected = {}, is_discovery_enabled = {}",
                    peers.len(),
                    is_discovery_enabled,
                );

                let is_broadcast = broadcast_strategy.is_multi_message(&peers);

                // Discovery is required if:
                //  - Discovery is enabled for this request
                //  - There where no peers returned
                //  - A direct public key broadcast strategy is used
                if is_discovery_enabled && peers.is_empty() && broadcast_strategy.direct_public_key().is_some() {
                    let (discovery_reply_tx, discovery_reply_rx) = oneshot::channel();
                    let target_public_key = broadcast_strategy.into_direct_public_key().expect("already checked");

                    let _ = reply_tx
                        .take()
                        .expect("cannot fail")
                        .send(SendMessageResponse::PendingDiscovery(discovery_reply_rx));

                    match self.initiate_peer_discovery(target_public_key).await {
                        Ok(peer) => {
                            // Set the reply_tx so that it can be used later
                            reply_tx = Some(discovery_reply_tx);
                            peers = vec![peer.node_id];
                        },
                        Err(err @ DhtOutboundError::DiscoveryFailed) => {
                            let _ = discovery_reply_tx.send(SendMessageResponse::Failed(SendFailure::DiscoveryFailed));
                            return Err(err);
                        },
                        Err(err) => {
                            let _ = discovery_reply_tx
                                .send(SendMessageResponse::Failed(SendFailure::General(err.to_string())));
                            return Err(err);
                        },
                    }
                }

                let expires = Utc::now() + self.message_validity_window;

                match self
                    .generate_send_messages(
                        peers,
                        destination,
                        dht_message_type,
                        encryption,
                        dht_header,
                        dht_message_flags,
                        force_origin,
                        is_broadcast,
                        body,
                        Some(expires),
                        tag,
                    )
                    .await
                {
                    Ok((msgs, send_states)) => {
                        // Reply with the `MessageTag`s for each message
                        let _ = reply_tx
                            .take()
                            .expect("cannot fail")
                            .send(SendMessageResponse::Queued(send_states.into()));

                        Ok(msgs)
                    },
                    Err(err) => {
                        let _ = reply_tx.take().expect("cannot fail").send(SendMessageResponse::Failed(
                            SendFailure::FailedToGenerateMessages(err.to_string()),
                        ));
                        Err(err)
                    },
                }
            },
            Err(err) => {
                let _ = reply_tx.send(SendMessageResponse::Failed(SendFailure::General(err.to_string())));
                Err(err)
            },
        }
    }

    async fn select_peers(&mut self, broadcast_strategy: BroadcastStrategy) -> Result<Vec<NodeId>, DhtOutboundError> {
        self.dht_requester
            .select_peers(broadcast_strategy)
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                DhtOutboundError::PeerSelectionFailed
            })
    }

    async fn initiate_peer_discovery(
        &mut self,
        dest_public_key: Box<CommsPublicKey>,
    ) -> Result<Peer, DhtOutboundError> {
        trace!(
            target: LOG_TARGET,
            "Initiating peer discovery for public key '{}'",
            dest_public_key
        );

        // Peer not found, let's try and discover it
        match self
            .dht_discovery_requester
            .discover_peer(*dest_public_key.clone(), NodeDestination::PublicKey(dest_public_key))
            .await
        {
            // Peer found!
            Ok(peer) => {
                debug!(
                    target: LOG_TARGET,
                    "Peer discovery succeeded for public key '{}'.", peer.public_key
                );
                Ok(peer)
            },
            // Error during discovery
            Err(err) => {
                debug!(target: LOG_TARGET, "Peer discovery failed because '{}'.", err);
                Err(DhtOutboundError::DiscoveryFailed)
            },
        }
    }

    async fn generate_send_messages(
        &mut self,
        selected_peers: Vec<NodeId>,
        destination: NodeDestination,
        dht_message_type: DhtMessageType,
        encryption: OutboundEncryption,
        custom_header: Option<DhtMessageHeader>,
        extra_flags: DhtMessageFlags,
        force_origin: bool,
        is_broadcast: bool,
        body: Bytes,
        expires: Option<DateTime<Utc>>,
        tag: Option<MessageTag>,
    ) -> Result<(Vec<DhtOutboundMessage>, Vec<MessageSendState>), DhtOutboundError> {
        let dht_flags = encryption.flags() | extra_flags;
        let expires_epochtime = expires.map(datetime_to_epochtime);

        let (ephemeral_public_key, origin_mac, body) = self.process_encryption(
            &encryption,
            force_origin,
            &destination,
            &dht_message_type,
            dht_flags,
            expires_epochtime,
            body,
        )?;

        if is_broadcast {
            self.add_to_dedup_cache(&body, self.node_identity.public_key().clone())
                .await?;
        }

        // Construct a DhtOutboundMessage for each recipient
        let messages = selected_peers.into_iter().map(|node_id| {
            let (reply_tx, reply_rx) = oneshot::channel();
            let tag = tag.unwrap_or_else(MessageTag::new);
            let send_state = MessageSendState::new(tag, reply_rx);
            (
                DhtOutboundMessage {
                    protocol_version: self.protocol_version,
                    tag,
                    destination_node_id: node_id,
                    destination: destination.clone(),
                    dht_message_type,
                    dht_flags,
                    custom_header: custom_header.clone(),
                    body: body.clone(),
                    reply: reply_tx.into(),
                    ephemeral_public_key: ephemeral_public_key.clone(),
                    origin_mac: origin_mac.clone(),
                    is_broadcast,
                    expires: expires.map(datetime_to_timestamp),
                },
                send_state,
            )
        });

        Ok(messages.unzip())
    }

    async fn add_to_dedup_cache(&mut self, body: &[u8], public_key: CommsPublicKey) -> Result<(), DhtOutboundError> {
        let hash = Challenge::new().chain(&body).finalize().to_vec();
        trace!(
            target: LOG_TARGET,
            "Dedup added message hash {} to cache for message",
            hash.to_hex(),
        );

        // Do not count messages we've broadcast towards the total hit count
        let hit_count = self
            .dht_requester
            .get_message_cache_hit_count(hash.clone())
            .await
            .map_err(|err| DhtOutboundError::FailedToInsertMessageHash(err.to_string()))?;
        if hit_count == 0 {
            self.dht_requester
                .add_message_to_dedup_cache(hash, public_key)
                .await
                .map_err(|err| DhtOutboundError::FailedToInsertMessageHash(err.to_string()))?;
        }
        Ok(())
    }

    fn process_encryption(
        &self,
        encryption: &OutboundEncryption,
        include_origin: bool,
        destination: &NodeDestination,
        message_type: &DhtMessageType,
        flags: DhtMessageFlags,
        expires: Option<EpochTime>,
        body: Bytes,
    ) -> Result<FinalMessageParts, DhtOutboundError> {
        match encryption {
            OutboundEncryption::EncryptFor(public_key) => {
                trace!(target: LOG_TARGET, "Encrypting message for {}", public_key);
                // Generate ephemeral public/private key pair and ECDH shared secret
                let (e_sk, e_pk) = CommsPublicKey::random_keypair(&mut OsRng);
                let shared_ephemeral_secret = crypt::generate_ecdh_secret(&e_sk, &**public_key);
                // Encrypt the message with the body
                let encrypted_body = crypt::encrypt(&shared_ephemeral_secret, &body)?;

                let mac_challenge = crypt::create_origin_mac_challenge_parts(
                    self.protocol_version,
                    destination,
                    message_type,
                    flags,
                    expires,
                    Some(&e_pk),
                    &encrypted_body,
                );
                // Sign the encrypted message
                let origin_mac = create_origin_mac(&self.node_identity, mac_challenge)?;
                // Encrypt and set the origin field
                let encrypted_origin_mac = crypt::encrypt(&shared_ephemeral_secret, &origin_mac)?;
                Ok((
                    Some(Arc::new(e_pk)),
                    Some(encrypted_origin_mac.into()),
                    encrypted_body.into(),
                ))
            },
            OutboundEncryption::ClearText => {
                trace!(target: LOG_TARGET, "Encryption not requested for message");

                if include_origin {
                    let mac_challenge = crypt::create_origin_mac_challenge_parts(
                        self.protocol_version,
                        destination,
                        message_type,
                        flags,
                        expires,
                        None,
                        &body,
                    );
                    let origin_mac = create_origin_mac(&self.node_identity, mac_challenge)?;
                    Ok((None, Some(origin_mac.into()), body))
                } else {
                    Ok((None, None, body))
                }
            },
        }
    }
}

fn create_origin_mac(node_identity: &NodeIdentity, mac_challenge: Challenge) -> Result<Vec<u8>, DhtOutboundError> {
    let signature = signature::sign_challenge(&mut OsRng, node_identity.secret_key().clone(), mac_challenge)?;
    let mac = OriginMac {
        public_key: node_identity.public_key().to_vec(),
        signature: signature.to_binary()?,
    };
    Ok(mac.to_encoded_bytes())
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use rand::rngs::OsRng;
    use tari_comms::{
        multiaddr::Multiaddr,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        runtime,
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tari_test_utils::unpack_enum;
    use tokio::{sync::oneshot, task};

    use super::*;
    use crate::{
        outbound::SendMessageParams,
        test_utils::{
            assert_send_static_service,
            create_dht_actor_mock,
            create_dht_discovery_mock,
            make_peer,
            service_spy,
        },
    };

    #[runtime::test]
    async fn send_message_flood() {
        let pk = CommsPublicKey::default();
        let example_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk),
            vec!["/ip4/127.0.0.1/tcp/9999".parse::<Multiaddr>().unwrap()].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            Default::default(),
        );

        let other_peer = {
            let mut p = example_peer.clone();
            let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
            p.node_id = NodeId::from_key(&pk);
            p.public_key = pk;
            p
        };

        let node_identity = Arc::new(NodeIdentity::random(
            &mut OsRng,
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        ));

        let (dht_requester, dht_mock) = create_dht_actor_mock(10);
        let (dht_discover_requester, _) = create_dht_discovery_mock(Duration::from_secs(10));

        let mock_state = dht_mock.get_shared_state();
        mock_state.set_select_peers_response(vec![example_peer.clone(), other_peer.clone()]);

        task::spawn(dht_mock.run());

        let spy = service_spy();

        let mut service = BroadcastMiddleware::new(
            spy.to_service::<PipelineError>(),
            node_identity,
            dht_requester,
            dht_discover_requester,
            chrono::Duration::seconds(10800),
            DhtProtocolVersion::latest(),
        );
        assert_send_static_service(&service);
        let (reply_tx, _reply_rx) = oneshot::channel();

        service
            .call(DhtOutboundRequest::SendMessage(
                Box::new(SendMessageParams::new().flood(vec![]).finish()),
                b"custom_msg".to_vec().into(),
                reply_tx,
            ))
            .await
            .unwrap();

        assert_eq!(spy.call_count(), 2);
        let requests = spy.take_requests();
        assert!(requests
            .iter()
            .any(|msg| msg.destination_node_id == example_peer.node_id));
        assert!(requests.iter().any(|msg| msg.destination_node_id == other_peer.node_id));
    }

    #[runtime::test]
    async fn send_message_direct_not_found() {
        // Test for issue https://github.com/tari-project/tari/issues/959

        let pk = CommsPublicKey::default();
        let node_identity = NodeIdentity::random(
            &mut OsRng,
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        let (dht_requester, dht_mock) = create_dht_actor_mock(10);
        task::spawn(dht_mock.run());
        let (dht_discover_requester, _) = create_dht_discovery_mock(Duration::from_secs(10));
        let spy = service_spy();

        let mut service = BroadcastMiddleware::new(
            spy.to_service::<PipelineError>(),
            Arc::new(node_identity),
            dht_requester,
            dht_discover_requester,
            chrono::Duration::seconds(10800),
            DhtProtocolVersion::latest(),
        );
        let (reply_tx, reply_rx) = oneshot::channel();

        service
            .call(DhtOutboundRequest::SendMessage(
                Box::new(
                    SendMessageParams::new()
                        .direct_public_key(pk)
                        .with_discovery(false)
                        .finish(),
                ),
                Bytes::from_static(b"custom_msg"),
                reply_tx,
            ))
            .await
            .unwrap();

        let send_message_response = reply_rx.await.unwrap();
        unpack_enum!(SendMessageResponse::Queued(tags) = send_message_response);
        assert_eq!(tags.len(), 0);
        assert_eq!(spy.call_count(), 0);
    }

    #[runtime::test]
    async fn send_message_direct_dht_discovery() {
        let node_identity = NodeIdentity::random(
            &mut OsRng,
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        let (dht_requester, dht_mock) = create_dht_actor_mock(10);
        task::spawn(dht_mock.run());
        let (dht_discover_requester, discovery_mock) = create_dht_discovery_mock(Duration::from_secs(10));
        let dht_discovery_state = discovery_mock.get_shared_state();
        discovery_mock.spawn();

        let peer_to_discover = make_peer();
        dht_discovery_state.set_discover_peer_response(peer_to_discover.clone());

        let spy = service_spy();

        let mut service = BroadcastMiddleware::new(
            spy.to_service::<PipelineError>(),
            Arc::new(node_identity),
            dht_requester,
            dht_discover_requester,
            chrono::Duration::seconds(10800),
            DhtProtocolVersion::latest(),
        );
        let (reply_tx, reply_rx) = oneshot::channel();

        service
            .call(DhtOutboundRequest::SendMessage(
                Box::new(
                    SendMessageParams::new()
                        .direct_public_key(peer_to_discover.public_key.clone())
                        .with_discovery(true)
                        .finish(),
                ),
                b"custom_msg".to_vec().into(),
                reply_tx,
            ))
            .await
            .unwrap();

        let send_message_response = reply_rx.await.unwrap();

        unpack_enum!(SendMessageResponse::PendingDiscovery(await_discovery) = send_message_response);
        let discovery_reply = await_discovery.await.unwrap();
        assert_eq!(dht_discovery_state.call_count(), 1);
        unpack_enum!(SendMessageResponse::Queued(tags) = discovery_reply);
        assert_eq!(tags.len(), 1);
        assert_eq!(spy.call_count(), 1);
    }
}
