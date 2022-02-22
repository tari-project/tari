// Copyright 2019. The Tari Project
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

use std::sync::Arc;

use chrono::Utc;
use futures::{pin_mut, StreamExt};
use log::*;
use tari_comms::connectivity::{ConnectivityEvent, ConnectivityRequester};
use tari_p2p::services::liveness::{LivenessEvent, LivenessHandle, MetadataKey, PingPongEvent};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;
use tokio::sync::broadcast;

use crate::contacts_service::{
    error::ContactsServiceError,
    handle::{ContactsLivenessData, ContactsLivenessEvent, ContactsServiceRequest, ContactsServiceResponse},
    storage::database::{Contact, ContactsBackend, ContactsDatabase},
};

const LOG_TARGET: &str = "wallet:contacts_service";
const NUM_ROUNDS_NETWORK_SILENCE: u16 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactMessageType {
    Ping,
    Pong,
}

pub struct ContactsService<T>
where T: ContactsBackend + 'static
{
    db: ContactsDatabase<T>,
    request_stream:
        Option<reply_channel::Receiver<ContactsServiceRequest, Result<ContactsServiceResponse, ContactsServiceError>>>,
    shutdown_signal: Option<ShutdownSignal>,
    liveness: LivenessHandle,
    liveness_data: Vec<ContactsLivenessData>,
    connectivity: ConnectivityRequester,
    event_publisher: broadcast::Sender<Arc<ContactsLivenessEvent>>,
    number_of_rounds_no_pings: u16,
}

impl<T> ContactsService<T>
where T: ContactsBackend + 'static
{
    pub fn new(
        db: ContactsDatabase<T>,
        request_stream: reply_channel::Receiver<
            ContactsServiceRequest,
            Result<ContactsServiceResponse, ContactsServiceError>,
        >,
        shutdown_signal: ShutdownSignal,
        liveness: LivenessHandle,
        connectivity: ConnectivityRequester,
        event_publisher: broadcast::Sender<Arc<ContactsLivenessEvent>>,
    ) -> Self {
        Self {
            db,
            request_stream: Some(request_stream),
            shutdown_signal: Some(shutdown_signal),
            liveness,
            liveness_data: Vec::new(),
            connectivity,
            event_publisher,
            number_of_rounds_no_pings: 0,
        }
    }

    pub async fn start(mut self) -> Result<(), ContactsServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Contacts Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let liveness_event_stream = self.liveness.get_event_stream();
        pin_mut!(liveness_event_stream);

        let connectivity_events = self.connectivity.get_event_subscription();
        pin_mut!(connectivity_events);

        let shutdown = self
            .shutdown_signal
            .take()
            .expect("Output Manager Service initialized without shutdown signal");
        pin_mut!(shutdown);

        // Add all contacts as monitored peers to the liveness service
        let result = self.db.get_contacts().await;
        if let Ok(ref contacts) = result {
            self.add_contacts_to_liveness_service(contacts).await?;
        }
        self.set_liveness_metadata(b"Watching you!".to_vec()).await?;
        debug!(target: LOG_TARGET, "Contacts Service started");
        loop {
            tokio::select! {
                Some(request_context) = request_stream.next() => {
                    let (request, reply_tx) = request_context.split();
                    let response = self.handle_request(request).await.map_err(|e| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", e);
                        e
                    });
                    let _ = reply_tx.send(response).map_err(|e| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        e
                    });
                },

                Ok(event) = liveness_event_stream.recv() => {
                    let _ = self.handle_liveness_event(&*event).await.map_err(|e| {
                        error!(target: LOG_TARGET, "Failed to handle contact status liveness event: {:?}", e);
                        e
                    });
                },

                Ok(event) = connectivity_events.recv() => {
                    self.handle_connectivity_event(event);
                }

                _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "Contacts service shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
        info!(target: LOG_TARGET, "Contacts Service ended");
        Ok(())
    }

    async fn handle_request(
        &mut self,
        request: ContactsServiceRequest,
    ) -> Result<ContactsServiceResponse, ContactsServiceError> {
        match request {
            ContactsServiceRequest::GetContact(pk) => {
                let result = self.db.get_contact(pk.clone()).await;
                if let Ok(ref contact) = result {
                    self.liveness.check_add_monitored_peer(contact.node_id.clone()).await?;
                };
                Ok(result.map(ContactsServiceResponse::Contact)?)
            },
            ContactsServiceRequest::UpsertContact(c) => {
                self.db.upsert_contact(c.clone()).await?;
                self.liveness.check_add_monitored_peer(c.node_id).await?;
                info!(
                    target: LOG_TARGET,
                    "Contact Saved: \nAlias: {}\nPubKey: {} ", c.alias, c.public_key
                );
                Ok(ContactsServiceResponse::ContactSaved)
            },
            ContactsServiceRequest::RemoveContact(pk) => {
                let result = self.db.remove_contact(pk.clone()).await?;
                self.liveness
                    .check_remove_monitored_peer(result.node_id.clone())
                    .await?;
                info!(
                    target: LOG_TARGET,
                    "Contact Removed: \nAlias: {}\nPubKey: {} ", result.alias, result.public_key
                );
                Ok(ContactsServiceResponse::ContactRemoved(result))
            },
            ContactsServiceRequest::GetContacts => {
                let result = self.db.get_contacts().await;
                if let Ok(ref contacts) = result {
                    self.add_contacts_to_liveness_service(contacts).await?;
                }
                Ok(result.map(ContactsServiceResponse::Contacts)?)
            },
        }
    }

    async fn add_contacts_to_liveness_service(&mut self, contacts: &[Contact]) -> Result<(), ContactsServiceError> {
        for contact in contacts {
            self.liveness.check_add_monitored_peer(contact.node_id.clone()).await?;
        }
        Ok(())
    }

    /// Tack this node's metadata on to ping/pongs sent by the liveness service
    async fn set_liveness_metadata(&mut self, message: Vec<u8>) -> Result<(), ContactsServiceError> {
        self.liveness
            .set_metadata_entry(MetadataKey::ContactsLiveness, message)
            .await?;
        Ok(())
    }

    async fn handle_liveness_event(&mut self, event: &LivenessEvent) -> Result<(), ContactsServiceError> {
        match event {
            // Received a ping, check if it contains ContactsLiveness
            LivenessEvent::ReceivedPing(event) => {
                trace!(
                    target: LOG_TARGET,
                    "Received contact liveness ping from neighbouring node '{}'.",
                    event.node_id
                );
                self.update_with_ping_pong(event, ContactMessageType::Ping).await?;
            },
            // Received a pong, check if our neighbour sent it and it contains ContactsLiveness
            LivenessEvent::ReceivedPong(event) => {
                trace!(
                    target: LOG_TARGET,
                    "Received contact liveness pong from neighbouring node '{}'.",
                    event.node_id
                );
                self.update_with_ping_pong(event, ContactMessageType::Pong).await?;
            },
            // New ping round has begun
            LivenessEvent::PingRoundBroadcast(num_peers) => {
                debug!(
                    target: LOG_TARGET,
                    "New contact liveness round sent to {} peer(s)", num_peers
                );
                // If there were no pings for a while, we are probably alone.
                if *num_peers == 0 {
                    self.number_of_rounds_no_pings += 1;
                    if self.number_of_rounds_no_pings >= NUM_ROUNDS_NETWORK_SILENCE {
                        self.send_network_silence().await?;
                        self.number_of_rounds_no_pings = 0;
                    }
                }
                self.resize_contacts_liveness_data_buffer(*num_peers);
            },
        }

        Ok(())
    }

    async fn update_with_ping_pong(
        &mut self,
        event: &PingPongEvent,
        message_type: ContactMessageType,
    ) -> Result<(), ContactsServiceError> {
        self.number_of_rounds_no_pings = 0;
        if event.metadata.has(MetadataKey::ContactsLiveness) {
            let mut latency: Option<u32> = None;
            if let Some(pos) = self
                .liveness_data
                .iter()
                .position(|peer_status| *peer_status.node_id() == event.node_id)
            {
                latency = self.liveness_data[pos].latency();
                self.liveness_data.remove(pos);
            }

            let last_seen = Utc::now();
            // Do not overwrite measured latency with value 'None' if this is a ping from a neighbouring node
            if event.latency.is_some() {
                latency = event.latency.map(|val| val.as_millis() as u32);
            }
            let this_public_key = self
                .db
                .update_contact_last_seen(&event.node_id, last_seen.naive_utc(), latency)
                .await?;

            let data = ContactsLivenessData::new(
                this_public_key,
                event.node_id.clone(),
                latency,
                last_seen,
                message_type.clone(),
            );
            self.liveness_data.push(data);

            // send only fails if there are no subscribers.
            let _ = self.event_publisher.send(Arc::new(ContactsLivenessEvent::StatusUpdated(
                self.liveness_data.clone(),
            )));
            trace!(
                target: LOG_TARGET,
                "{:?} from {} last seen at {} with latency {:?} ms",
                message_type,
                event.node_id,
                last_seen,
                latency
            );
        } else {
            trace!(
                target: LOG_TARGET,
                "Ping-pong metadata key from {} not recognized",
                event.node_id
            );
        }
        Ok(())
    }

    async fn send_network_silence(&mut self) -> Result<(), ContactsServiceError> {
        let _ = self
            .event_publisher
            .send(Arc::new(ContactsLivenessEvent::NetworkSilence));
        Ok(())
    }

    // Ensure that we're waiting for the correct amount of peers to respond to
    // and have allocated space for their replies
    fn resize_contacts_liveness_data_buffer(&mut self, n: usize) {
        match self.liveness_data.capacity() {
            cap if n > cap => {
                let additional = n - self.liveness_data.len();
                self.liveness_data.reserve_exact(additional);
            },
            cap if n < cap => {
                self.liveness_data.shrink_to(cap);
            },
            _ => {},
        }
    }

    fn handle_connectivity_event(&mut self, event: ConnectivityEvent) {
        use ConnectivityEvent::*;
        match event {
            PeerDisconnected(node_id) | PeerBanned(node_id) => {
                if let Some(pos) = self.liveness_data.iter().position(|p| *p.node_id() == node_id) {
                    debug!(
                        target: LOG_TARGET,
                        "Removing disconnected/banned peer `{}` from contacts status list ", node_id
                    );
                    self.liveness_data.remove(pos);
                }
            },
            _ => {},
        }
    }
}
