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
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures::{
    future,
    future::BoxFuture,
    stream::{self, StreamExt},
    task::Context,
};
use log::*;
use tari_comms::{
    Bytes,
    BytesMut,
    message::{MessageExt, MessageTag},
    peer_manager::{NodeId, NodeIdentity, Peer},
    pipeline::PipelineError,
    types::{CommsDHKE, CommsPublicKey},
};
use tari_utilities::{ByteArray, epoch_time::EpochTime, hex::Hex};
use tokio::{sync::oneshot, time::Instant};
use tower::{Service, ServiceExt, layer::Layer};

use super::{error::DhtOutboundError, message::DhtOutboundRequest};
use crate::{
    DhtConfig,
    actor::DhtRequester,
    broadcast_strategy::BroadcastStrategy,
    config::{FORWARD_BUDGET_FACTOR, FORWARD_SOURCE_BUDGET_FACTOR},
    crypt,
    dedup,
    discovery::DhtDiscoveryRequester,
    envelope::{DhtMessageFlags, DhtMessageHeader, NodeDestination, datetime_to_epochtime},
    inbound::decode_envelope_body,
    message_signature::MessageSignature,
    outbound::{
        SendMessageResponse,
        message::{DhtOutboundMessage, OutboundEncryption, SendFailure},
        message_params::FinalSendMessageParams,
        message_send_state::MessageSendState,
    },
    proto::envelope::DhtMessageType,
    version::DhtProtocolVersion,
};

const LOG_TARGET: &str = "comms::dht::outbound::broadcast_middleware";

/// How long a message this node sends stays valid (its `expires`).
pub(crate) const MESSAGE_VALIDITY_WINDOW: Duration = Duration::from_secs(3 * 60 * 60);

/// Caps how many messages of other peers this node forwards (sends under their original header), so that relaying for
/// others never takes it over a neighbour's forwarded-traffic flood limit, and so that a few sources cannot use up the
/// forwarding that other sources need. Per `flood_ban_timespan`:
/// - each source (the peer that delivered the messages to this node) gets `FORWARD_SOURCE_BUDGET_FACTOR` x
///   `flood_ban_max_msg_count` forwarded messages, each counted once however many peers it goes to;
/// - each peer gets at most `FORWARD_BUDGET_FACTOR` x `flood_ban_max_msg_count` forwarded messages. Until it has had
///   `CONTENDED_PERCENT` of that, any source may use it. From then on it is shared fairly: a source is let through only
///   while it has sent the peer fewer than the budget / (active sources + 1). The heaviest sources stop first, and the
///   extra share stays free for a source that has sent little or nothing, so low-rate sources still get through while a
///   few others flood the same peer.
///
/// Messages this node originates are not capped.
pub(crate) struct ForwardBudget {
    max_count: usize,
    window: Duration,
    state: Mutex<ForwardBudgetState>,
}

/// The share of a peer's forwarding budget after which it is split fairly between sources
const CONTENDED_PERCENT: usize = 80;

#[derive(Default)]
struct ForwardBudgetState {
    /// When forwarded messages were sent to each peer within the window, oldest first, in total and by source
    to_peer: HashMap<NodeId, PeerForwards>,
    /// When each source's forwarded messages were sent within the window, oldest first
    from_source: HashMap<NodeId, VecDeque<Instant>>,
    pruned_at: Option<Instant>,
    /// Forwarded messages dropped since the last log line, by source
    dropped: HashMap<NodeId, usize>,
    logged_at: Option<Instant>,
}

#[derive(Default)]
struct PeerForwards {
    sent: VecDeque<Instant>,
    by_source: HashMap<NodeId, VecDeque<Instant>>,
}

/// Drops the times before `since`
fn trim(sent: &mut VecDeque<Instant>, since: Instant) {
    while sent.front().is_some_and(|t| *t < since) {
        sent.pop_front();
    }
}

impl ForwardBudget {
    pub(crate) fn new(config: &DhtConfig) -> Self {
        Self {
            max_count: config.flood_ban_max_msg_count,
            window: config.flood_ban_timespan,
            state: Mutex::default(),
        }
    }

    /// Keeps the peers that a message from `source` can still be forwarded to in this window, and uses up its budget
    fn take(&self, source: &NodeId, peers: &mut Vec<NodeId>) {
        let max_per_peer = self.max_count.saturating_mul(FORWARD_BUDGET_FACTOR);
        let contended_at = max_per_peer.saturating_mul(CONTENDED_PERCENT) / 100;
        let now = Instant::now();
        let since = now.checked_sub(self.window).unwrap_or(now);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let state = &mut *state;
        if state
            .pruned_at
            .is_none_or(|at| now.saturating_duration_since(at) >= self.window)
        {
            state.to_peer.retain(|_, forwards| {
                forwards.by_source.retain(|_, sent| {
                    trim(sent, since);
                    !sent.is_empty()
                });
                trim(&mut forwards.sent, since);
                !forwards.sent.is_empty()
            });
            state.from_source.retain(|_, sent| {
                trim(sent, since);
                !sent.is_empty()
            });
            state.pruned_at = Some(now);
        }
        let num_before = peers.len();
        let from_source = state.from_source.entry(source.clone()).or_default();
        trim(from_source, since);
        if from_source.len() >= self.max_count.saturating_mul(FORWARD_SOURCE_BUDGET_FACTOR) {
            peers.clear();
        } else {
            peers.retain(|peer| {
                let forwards = state.to_peer.entry(peer.clone()).or_default();
                trim(&mut forwards.sent, since);
                if forwards.sent.len() >= max_per_peer {
                    return false;
                }
                if forwards.sent.len() >= contended_at {
                    forwards.by_source.retain(|_, sent| {
                        trim(sent, since);
                        !sent.is_empty()
                    });
                    let num_active = forwards
                        .by_source
                        .len()
                        .saturating_add(usize::from(!forwards.by_source.contains_key(source)));
                    // At least 1, so that a small budget split many ways still lets each source through
                    let fair_share = max_per_peer
                        .checked_div(num_active.saturating_add(1))
                        .unwrap_or(0)
                        .max(1);
                    if forwards.by_source.get(source).map_or(0, VecDeque::len) >= fair_share {
                        return false;
                    }
                }
                forwards.sent.push_back(now);
                forwards.by_source.entry(source.clone()).or_default().push_back(now);
                true
            });
            if !peers.is_empty() {
                state.from_source.entry(source.clone()).or_default().push_back(now);
            }
        }
        let num_dropped = num_before.saturating_sub(peers.len());
        if num_dropped > 0 {
            let dropped = state.dropped.entry(source.clone()).or_default();
            *dropped = dropped.saturating_add(num_dropped);
        }
        if !state.dropped.is_empty() &&
            state
                .logged_at
                .is_none_or(|at| now.saturating_duration_since(at) >= self.window)
        {
            let total = state.dropped.values().fold(0usize, |acc, n| acc.saturating_add(*n));
            if let Some((top_source, top_dropped)) = state.dropped.iter().max_by_key(|(_, n)| **n) {
                warn!(
                    target: LOG_TARGET,
                    "Dropped {} forwarded message(s) over the forwarding budget in the last {:.0?}, {} of them from \
                     source `{}`",
                    total,
                    self.window,
                    top_dropped,
                    top_source
                );
            }
            state.dropped.clear();
            state.logged_at = Some(now);
        }
    }
}

pub struct BroadcastLayer {
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    node_identity: Arc<NodeIdentity>,
    message_validity_window: chrono::Duration,
    protocol_version: DhtProtocolVersion,
    forward_budget: Arc<ForwardBudget>,
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
            message_validity_window: chrono::Duration::from_std(MESSAGE_VALIDITY_WINDOW)
                .expect("message_validity_window is too large"),
            protocol_version: config.protocol_version,
            forward_budget: Arc::new(ForwardBudget::new(config)),
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
            self.forward_budget.clone(),
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
    forward_budget: Arc<ForwardBudget>,
}

impl<S> BroadcastMiddleware<S> {
    pub(crate) fn new(
        service: S,
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        message_validity_window: chrono::Duration,
        protocol_version: DhtProtocolVersion,
        forward_budget: Arc<ForwardBudget>,
    ) -> Self {
        Self {
            next_service: service,
            dht_requester,
            dht_discovery_requester,
            node_identity,
            message_validity_window,
            protocol_version,
            forward_budget,
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
                self.forward_budget.clone(),
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
    forward_budget: Arc<ForwardBudget>,
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
        forward_budget: Arc<ForwardBudget>,
    ) -> Self {
        Self {
            service,
            node_identity,
            dht_requester,
            dht_discovery_requester,
            request: Some(request),
            message_validity_window,
            protocol_version,
            forward_budget,
        }
    }

    pub async fn handle(mut self) -> Result<(), PipelineError> {
        let request = self.request.take().expect("request cannot be None");
        trace!(target: LOG_TARGET, "Processing outbound request {request}");
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
                warn!(target: LOG_TARGET, "Error when sending broadcast messages: {err}");
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
        body: BytesMut,
        reply_tx: oneshot::Sender<SendMessageResponse>,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError> {
        trace!(target: LOG_TARGET, "Send params: {params:?}");
        if params
            .broadcast_strategy
            .direct_public_key()
            .filter(|pk| *pk == self.node_identity.public_key())
            .is_some()
        {
            warn!(target: LOG_TARGET, "Attempt to send a message to ourselves");
            let _result = reply_tx.send(SendMessageResponse::Failed(SendFailure::SendToOurselves));
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
            forwarded_from,
            debug_info: _,
            tag,
        } = params;

        match self.select_peers(broadcast_strategy.clone()).await {
            Ok(mut peers) => {
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

                    let _result = reply_tx
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
                            let _result =
                                discovery_reply_tx.send(SendMessageResponse::Failed(SendFailure::DiscoveryFailed));
                            return Err(err);
                        },
                        Err(err) => {
                            let _result = discovery_reply_tx
                                .send(SendMessageResponse::Failed(SendFailure::General(err.to_string())));
                            return Err(err);
                        },
                    }
                }

                let expires = Utc::now()
                    .checked_add_signed(self.message_validity_window)
                    .unwrap_or(chrono::DateTime::<Utc>::MAX_UTC);

                match self
                    .generate_send_messages(
                        peers,
                        destination,
                        dht_message_type,
                        encryption,
                        dht_header,
                        forwarded_from,
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
                        let _result = reply_tx
                            .take()
                            .expect("cannot fail")
                            .send(SendMessageResponse::Queued(send_states.into()));

                        Ok(msgs)
                    },
                    Err(err) => {
                        let _result = reply_tx.take().expect("cannot fail").send(SendMessageResponse::Failed(
                            SendFailure::FailedToGenerateMessages(err.to_string()),
                        ));
                        Err(err)
                    },
                }
            },
            Err(err) => {
                let _result = reply_tx.send(SendMessageResponse::Failed(SendFailure::General(err.to_string())));
                Err(err)
            },
        }
    }

    async fn select_peers(&mut self, broadcast_strategy: BroadcastStrategy) -> Result<Vec<NodeId>, DhtOutboundError> {
        self.dht_requester
            .select_peers(broadcast_strategy)
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "{err}");
                DhtOutboundError::PeerSelectionFailed
            })
    }

    async fn initiate_peer_discovery(
        &mut self,
        dest_public_key: Box<CommsPublicKey>,
    ) -> Result<Peer, DhtOutboundError> {
        trace!(
            target: LOG_TARGET,
            "Initiating peer discovery for public key '{dest_public_key}'"

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
                debug!(target: LOG_TARGET, "Peer discovery failed because '{err}'.");
                Err(DhtOutboundError::DiscoveryFailed)
            },
        }
    }

    async fn generate_send_messages(
        &mut self,
        mut selected_peers: Vec<NodeId>,
        destination: NodeDestination,
        dht_message_type: DhtMessageType,
        encryption: OutboundEncryption,
        custom_header: Option<DhtMessageHeader>,
        forwarded_from: Option<NodeId>,
        extra_flags: DhtMessageFlags,
        force_origin: bool,
        is_broadcast: bool,
        body: BytesMut,
        expires: Option<DateTime<Utc>>,
        tag: Option<MessageTag>,
    ) -> Result<(Vec<DhtOutboundMessage>, Vec<MessageSendState>), DhtOutboundError> {
        let dht_flags = encryption.flags() | extra_flags;
        let expires_epochtime = expires.map(datetime_to_epochtime);

        let (ephemeral_public_key, message_signature, body) = self.process_encryption(
            &encryption,
            force_origin,
            &destination,
            dht_message_type,
            dht_flags,
            expires_epochtime,
            body,
        )?;

        // Another peer's message being forwarded: only peers with forwarding budget left for its source get it
        if let Some(source) = &forwarded_from {
            self.forward_budget.take(source, &mut selected_peers);
        }

        // A message sent with an existing header is being forwarded: the inbound path already recorded its key, and
        // the signature that key needs is in that header, not in `message_signature`.
        if is_broadcast && custom_header.is_none() {
            let sig_canonical = message_signature.as_deref().unwrap_or(&[]);
            let is_signed = !sig_canonical.is_empty();
            // Key an unsigned cleartext body the way the inbound path will key its echo
            let decoded_body = if !is_signed && !dht_flags.is_encrypted() {
                decode_envelope_body(&body).ok()
            } else {
                None
            };
            let hash = dedup::message_key(sig_canonical, dht_flags, &body, decoded_body.as_ref());
            self.add_to_dedup_cache(hash, expires_epochtime, is_signed).await?;
        }

        // Construct a DhtOutboundMessage for each recipient
        let messages = selected_peers.into_iter().map(|node_id| {
            let (reply_tx, reply_rx) = oneshot::channel();
            let tag = tag.unwrap_or_default();
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
                    message_signature: message_signature.clone(),
                    is_broadcast,
                    expires: expires_epochtime.map(EpochTime::as_u64),
                },
                send_state,
            )
        });

        Ok(messages.unzip())
    }

    async fn add_to_dedup_cache(
        &mut self,
        hash: [u8; 32],
        expires: Option<EpochTime>,
        is_signed: bool,
    ) -> Result<(), DhtOutboundError> {
        trace!(
            target: LOG_TARGET,
            "Dedup added message hash {} to cache for message",
            hash.to_hex(),
        );

        // Do not count messages we've broadcast towards the total hit count
        let hit_count = self
            .dht_requester
            .get_message_cache_hit_count(hash.to_vec())
            .await
            .map_err(|err| DhtOutboundError::FailedToInsertMessageHash(err.to_string()))?;
        if hit_count == 0 {
            self.dht_requester
                .add_message_to_dedup_cache(
                    hash.to_vec(),
                    self.node_identity.public_key().clone(),
                    expires,
                    is_signed,
                )
                .await
                .map_err(|err| DhtOutboundError::FailedToInsertMessageHash(err.to_string()))?;
        }
        Ok(())
    }

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn process_encryption(
        &self,
        encryption: &OutboundEncryption,
        include_origin: bool,
        destination: &NodeDestination,
        message_type: DhtMessageType,
        flags: DhtMessageFlags,
        expires: Option<EpochTime>,
        mut body: BytesMut,
    ) -> Result<FinalMessageParts, DhtOutboundError> {
        match encryption {
            // Encrypt the message, protecting the sender identity
            OutboundEncryption::EncryptFor(recipient_public_key) => {
                trace!(target: LOG_TARGET, "Encrypting message for {recipient_public_key}");
                let ristretto_recipient_public_key = recipient_public_key
                    .to_public_key()
                    .map_err(|e| DhtOutboundError::MessageFormatError(e.to_string()))?;

                // Perform an ephemeral ECDH exchange against the recipient public key
                let (ephemeral_secret_key, ephemeral_public_key) = CommsPublicKey::random_keypair(&mut rand::rng());
                let shared_ephemeral_secret = CommsDHKE::new(&ephemeral_secret_key, &ristretto_recipient_public_key);

                // Produce a masked sender public key using an offset mask derived from the ECDH exchange
                let mask = crypt::generate_key_mask(&shared_ephemeral_secret)
                    .map_err(|e| DhtOutboundError::CipherError(e.to_string()))?;
                let masked_sender_public_key = &mask *
                    self.node_identity
                        .public_key()
                        .to_public_key()
                        .map_err(|e| DhtOutboundError::MessageFormatError(e.to_string()))?;

                // Pad and encrypt the message using the masked sender public key
                let key_message = crypt::generate_key_message(&shared_ephemeral_secret);
                crypt::encrypt_message(&key_message, &mut body, masked_sender_public_key.as_bytes())?;
                let encrypted_body = body.freeze();

                // Produce a hash that binds the message and metadata
                let binding_hash = crypt::create_message_domain_separated_hash_parts(
                    self.protocol_version,
                    destination,
                    message_type,
                    flags,
                    expires,
                    Some(&ephemeral_public_key),
                    &encrypted_body,
                );

                // Sign the encrypted message using the masked sender key
                let masked_sender_secret_key = mask * self.node_identity.secret_key();
                let signature = MessageSignature::new_signed(masked_sender_secret_key, &binding_hash).to_proto();

                Ok((
                    Some(Arc::new(ephemeral_public_key)),
                    Some(signature.to_encoded_bytes().into()), // this includes the masked signer public key
                    encrypted_body,
                ))
            },
            // Keep the message unencrypted
            OutboundEncryption::ClearText => {
                trace!(target: LOG_TARGET, "Encryption not requested for message");

                // We may or may not sign it
                if include_origin {
                    let binding_hash = crypt::create_message_domain_separated_hash_parts(
                        self.protocol_version,
                        destination,
                        message_type,
                        flags,
                        expires,
                        None,
                        &body,
                    );
                    let signature =
                        MessageSignature::new_signed(self.node_identity.secret_key().clone(), &binding_hash).to_proto();
                    Ok((None, Some(signature.to_encoded_bytes().into()), body.freeze()))
                // this includes the signer
                // public key
                } else {
                    Ok((None, None, body.freeze()))
                }
            },
        }
    }
}

#[cfg(test)]
mod test {
    use std::{iter::repeat_with, time::Duration};

    use tari_comms::{
        multiaddr::Multiaddr,
        net_address::{MultiaddressesWithStats, PeerAddressSource},
        peer_manager::{PeerFeatures, PeerFlags},
    };
    use tari_test_utils::unpack_enum;
    use tokio::task;

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

    #[tokio::test]
    async fn test_send_message_flood() {
        let pk = CommsPublicKey::default();
        let example_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk),
            MultiaddressesWithStats::from_addresses_with_source(
                vec!["/ip4/127.0.0.1/tcp/9999".parse::<Multiaddr>().unwrap()],
                &PeerAddressSource::Config,
            ),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            Default::default(),
        );

        let other_peer = {
            let mut p = example_peer.clone();
            let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
            p.node_id = NodeId::from_key(&pk);
            p.public_key = pk;
            p
        };

        let node_identity = Arc::new(NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        ));

        let (dht_requester, dht_mock) = create_dht_actor_mock();
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
            Arc::new(ForwardBudget::new(&DhtConfig::default())),
        );
        assert_send_static_service(&service);
        let (reply_tx, _reply_rx) = oneshot::channel();

        service
            .call(DhtOutboundRequest::SendMessage(
                Box::new(SendMessageParams::new().flood(vec![]).finish()),
                b"custom_msg".as_slice().into(),
                reply_tx,
            ))
            .await
            .unwrap();

        assert_eq!(spy.call_count(), 2);
        let requests = spy.take_requests();
        assert!(
            requests
                .iter()
                .any(|msg| msg.destination_node_id == example_peer.node_id)
        );
        assert!(requests.iter().any(|msg| msg.destination_node_id == other_peer.node_id));
    }

    fn budget(max_count: usize) -> (DhtConfig, ForwardBudget) {
        let config = DhtConfig {
            flood_ban_max_msg_count: max_count,
            ..Default::default()
        };
        let budget = ForwardBudget::new(&config);
        (config, budget)
    }

    fn take(budget: &ForwardBudget, source: &NodeId, peers: &[&NodeId]) -> Vec<NodeId> {
        let mut peers = peers.iter().map(|p| (*p).clone()).collect();
        budget.take(source, &mut peers);
        peers
    }

    /// A source's budget is charged once per forwarded message, however many peers it goes to
    #[tokio::test(start_paused = true)]
    async fn forwarding_is_charged_once_per_message() {
        let (config, budget) = budget(1);
        let peers = repeat_with(|| make_peer().node_id).take(6).collect::<Vec<_>>();
        let peer_refs = peers.iter().collect::<Vec<_>>();
        let source = make_peer().node_id;

        assert_eq!(take(&budget, &source, &peer_refs), peers);
        // The source's one message for this window is used up
        assert!(take(&budget, &source, &peer_refs).is_empty());

        tokio::time::advance(config.flood_ban_timespan + Duration::from_secs(1)).await;
        assert_eq!(take(&budget, &source, &peer_refs), peers);
    }

    /// While a peer has budget to spare, no source is held to a share of it
    #[tokio::test(start_paused = true)]
    async fn forwarding_is_not_throttled_without_contention() {
        let max_count = 10;
        let (_, budget) = budget(max_count);
        let peer = make_peer().node_id;
        for source in repeat_with(|| make_peer().node_id).take(9) {
            assert_eq!(take(&budget, &source, &[&peer]), vec![peer.clone()]);
        }
        // Far more than a fair share of the 10 sources, but the peer's budget is not contended
        let heavy = make_peer().node_id;
        for _ in 0..max_count {
            assert_eq!(take(&budget, &heavy, &[&peer]), vec![peer.clone()]);
        }
    }

    /// Sources flooding one peer up to their limits are held to a fair share once its budget is contended, leaving
    /// room for a low-rate source, and the peer's total is never exceeded
    #[tokio::test(start_paused = true)]
    async fn heavy_sources_do_not_starve_a_low_rate_source() {
        let max_count = 10;
        let (_, budget) = budget(max_count);
        let peer = make_peer().node_id;
        let heavy = repeat_with(|| make_peer().node_id)
            .take(FORWARD_BUDGET_FACTOR)
            .collect::<Vec<_>>();
        let mut sent = 0usize;
        for _ in 0..max_count {
            for source in &heavy {
                sent = sent.saturating_add(take(&budget, source, &[&peer]).len());
            }
        }
        assert!(sent < max_count * FORWARD_BUDGET_FACTOR);

        assert_eq!(take(&budget, &make_peer().node_id, &[&peer]), vec![peer]);
    }

    #[tokio::test]
    async fn the_forwarding_budget_applies_only_to_forwarded_messages() {
        let node_identity = Arc::new(NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        ));
        let peer = make_peer();
        let (dht_requester, dht_mock) = create_dht_actor_mock();
        let (dht_discover_requester, _) = create_dht_discovery_mock(Duration::from_secs(10));
        dht_mock
            .get_shared_state()
            .set_select_peers_response(vec![peer.clone()]);
        task::spawn(dht_mock.run());
        let spy = service_spy();
        let config = DhtConfig {
            flood_ban_max_msg_count: 1,
            ..Default::default()
        };
        let mut service = BroadcastMiddleware::new(
            spy.to_service::<PipelineError>(),
            node_identity.clone(),
            dht_requester,
            dht_discover_requester,
            chrono::Duration::seconds(10800),
            DhtProtocolVersion::latest(),
            Arc::new(ForwardBudget::new(&config)),
        );
        let source = make_peer().node_id;
        let header = DhtMessageHeader {
            version: DhtProtocolVersion::latest(),
            destination: NodeDestination::Unknown,
            message_signature: Vec::new(),
            ephemeral_public_key: None,
            message_type: DhtMessageType::None,
            flags: DhtMessageFlags::NONE,
            message_tag: MessageTag::new(),
            expires: None,
        };

        for _ in 0..=FORWARD_BUDGET_FACTOR {
            let (reply_tx, _reply_rx) = oneshot::channel();
            service
                .call(DhtOutboundRequest::SendMessage(
                    Box::new(
                        SendMessageParams::new()
                            .propagate(NodeDestination::Unknown, vec![])
                            .with_dht_header(header.clone())
                            .forwarded_from(source.clone())
                            .finish(),
                    ),
                    b"forwarded".as_slice().into(),
                    reply_tx,
                ))
                .await
                .unwrap();
        }
        // One forwarded message from one source to one peer per window
        assert_eq!(spy.call_count(), 1);

        for _ in 0..=FORWARD_BUDGET_FACTOR {
            let (reply_tx, _reply_rx) = oneshot::channel();
            service
                .call(DhtOutboundRequest::SendMessage(
                    Box::new(
                        SendMessageParams::new()
                            .propagate(NodeDestination::Unknown, vec![])
                            .finish(),
                    ),
                    b"own".as_slice().into(),
                    reply_tx,
                ))
                .await
                .unwrap();
        }
        assert_eq!(spy.call_count(), FORWARD_BUDGET_FACTOR + 2);
    }

    #[tokio::test]
    async fn test_send_message_direct_not_found() {
        // Test for issue https://github.com/tari-project/tari/issues/959

        let pk = CommsPublicKey::default();
        let node_identity = NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        let (dht_requester, dht_mock) = create_dht_actor_mock();
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
            Arc::new(ForwardBudget::new(&DhtConfig::default())),
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
                BytesMut::from(b"custom_msg".as_slice()),
                reply_tx,
            ))
            .await
            .unwrap();

        let send_message_response = reply_rx.await.unwrap();
        unpack_enum!(SendMessageResponse::Queued(tags) = send_message_response);
        assert_eq!(tags.len(), 0);
        assert_eq!(spy.call_count(), 0);
    }

    #[tokio::test]
    async fn test_send_message_direct_dht_discovery() {
        let node_identity = NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        let (dht_requester, dht_mock) = create_dht_actor_mock();
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
            Arc::new(ForwardBudget::new(&DhtConfig::default())),
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
                b"custom_msg".as_slice().into(),
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
