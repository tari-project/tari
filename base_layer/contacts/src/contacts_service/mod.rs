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

pub mod error;
pub mod handle;
pub mod proto;
pub mod service;
pub mod storage;
pub mod types;

use std::{sync::Arc, time::Duration};

use futures::future;
use log::*;
use tari_comms::connectivity::ConnectivityRequester;
use tari_comms_dht::Dht;
use tari_p2p::{comms_connector::SubscriptionFactory, services::liveness::LivenessHandle};
use tari_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::broadcast;

use crate::contacts_service::{
    handle::ContactsServiceHandle,
    service::ContactsService,
    storage::database::{ContactsBackend, ContactsDatabase},
};

const LOG_TARGET: &str = "contacts::contacts_service::initializer";

pub struct ContactsServiceInitializer<T>
where T: ContactsBackend
{
    backend: Option<T>,
    contacts_auto_ping_interval: Duration,
    contacts_online_ping_window: usize,
    subscription_factory: Arc<SubscriptionFactory>,
}

impl<T> ContactsServiceInitializer<T>
where T: ContactsBackend
{
    pub fn new(
        backend: T,
        subscription_factory: Arc<SubscriptionFactory>,
        contacts_auto_ping_interval: Duration,
        online_ping_window: usize,
    ) -> Self {
        Self {
            backend: Some(backend),
            contacts_auto_ping_interval,
            contacts_online_ping_window: online_ping_window,
            subscription_factory,
        }
    }
}

#[async_trait]
impl<T> ServiceInitializer for ContactsServiceInitializer<T>
where T: ContactsBackend + 'static
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        let (liveness_tx, liveness_rx) = reply_channel::unbounded();
        let (publisher, _) = broadcast::channel(250);
        let (message_publisher, _) = broadcast::channel(250);

        let contacts_handle = ContactsServiceHandle::new(liveness_tx, publisher.clone(), message_publisher.clone());

        // Register handle before waiting for handles to be ready
        context.register_handle(contacts_handle);

        let backend = self
            .backend
            .take()
            .expect("Cannot start Contacts Service without setting a storage backend");

        let shutdown_signal = context.get_shutdown_signal();

        let contacts_auto_ping_interval = self.contacts_auto_ping_interval;
        let contacts_online_ping_window = self.contacts_online_ping_window;
        let subscription_factory = self.subscription_factory.clone();
        context.spawn_when_ready(move |handles| async move {
            let liveness = handles.expect_handle::<LivenessHandle>();
            let connectivity = handles.expect_handle::<ConnectivityRequester>();
            let dht = handles.expect_handle::<Dht>();

            let service = ContactsService::new(
                ContactsDatabase::new(backend),
                liveness_rx,
                handles.get_shutdown_signal(),
                liveness,
                connectivity,
                dht,
                subscription_factory,
                publisher,
                message_publisher,
                contacts_auto_ping_interval,
                contacts_online_ping_window,
            )
            .start();
            futures::pin_mut!(service);
            future::select(service, shutdown_signal).await;
            info!(target: LOG_TARGET, "Contacts service shutdown");
        });
        Ok(())
    }
}
