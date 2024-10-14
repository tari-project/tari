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

use std::{marker::PhantomData, sync::Arc};

use futures::StreamExt;
use log::*;
use tari_common::configuration::Network;
use tari_common_types::wallet_types::WalletType;
use tari_core::{
    consensus::ConsensusManager,
    transactions::{key_manager::TransactionKeyManagerInterface, CryptoFactories},
};
use tari_network::{identity, OutboundMessaging};
use tari_p2p::{
    message::{DomainMessage, TariMessageType, TariNodeMessageSpec},
    proto::{base_node as base_node_proto, transaction_protocol as proto},
    Dispatcher,
};
use tari_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::{broadcast, mpsc};

use crate::{
    base_node_service::handle::BaseNodeServiceHandle,
    connectivity_service::WalletConnectivityHandle,
    output_manager_service::handle::OutputManagerHandle,
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionServiceHandle,
        service::TransactionService,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
};

pub mod config;
pub mod error;
pub mod handle;
pub mod protocols;
pub mod service;
pub mod storage;
pub mod tasks;
mod utc;

const LOG_TARGET: &str = "wallet::transaction_service";
const SUBSCRIPTION_LABEL: &str = "Transaction Service";

pub struct TransactionServiceInitializer<T, W, TKeyManagerInterface>
where
    T: TransactionBackend,
    W: WalletBackend,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    config: TransactionServiceConfig,
    dispatcher: Dispatcher,
    tx_backend: Option<T>,
    node_identity: Arc<identity::Keypair>,
    network: Network,
    consensus_manager: ConsensusManager,
    factories: CryptoFactories,
    wallet_database: Option<WalletDatabase<W>>,
    wallet_type: Arc<WalletType>,
    _phantom_data: PhantomData<TKeyManagerInterface>,
}

impl<T, W, TKeyManagerInterface> TransactionServiceInitializer<T, W, TKeyManagerInterface>
where
    T: TransactionBackend,
    W: WalletBackend,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub fn new(
        config: TransactionServiceConfig,
        dispatcher: Dispatcher,
        backend: T,
        node_identity: Arc<identity::Keypair>,
        network: Network,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
        wallet_database: WalletDatabase<W>,
        wallet_type: Arc<WalletType>,
    ) -> Self {
        Self {
            config,
            dispatcher,
            tx_backend: Some(backend),
            node_identity,
            network,
            consensus_manager,
            factories,
            wallet_database: Some(wallet_database),
            wallet_type,
            _phantom_data: Default::default(),
        }
    }
}

#[async_trait]
impl<T, W, TKeyManagerInterface> ServiceInitializer for TransactionServiceInitializer<T, W, TKeyManagerInterface>
where
    T: TransactionBackend + 'static,
    W: WalletBackend + 'static,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        let (sender, receiver) = reply_channel::unbounded();
        let (tx_messages, rx_messages) = mpsc::unbounded_channel();

        self.dispatcher
            .register(TariMessageType::SenderPartialTransaction, tx_messages.clone());
        self.dispatcher
            .register(TariMessageType::ReceiverPartialTransactionReply, tx_messages.clone());
        self.dispatcher
            .register(TariMessageType::TransactionFinalized, tx_messages.clone());
        self.dispatcher
            .register(TariMessageType::BaseNodeResponse, tx_messages.clone());
        self.dispatcher
            .register(TariMessageType::TransactionCancelled, tx_messages.clone());

        let (publisher, _) = broadcast::channel(self.config.transaction_event_channel_size);

        let transaction_handle = TransactionServiceHandle::new(sender, publisher.clone());

        // Register handle before waiting for handles to be ready
        context.register_handle(transaction_handle);

        let tx_backend = self
            .tx_backend
            .take()
            .expect("Cannot start Transaction Service without providing a backend");

        let wallet_database = self
            .wallet_database
            .take()
            .expect("Cannot start Transaction Service without providing a wallet database");

        let node_identity = self.node_identity.clone();
        let consensus_manager = self.consensus_manager.clone();
        let factories = self.factories.clone();
        let config = self.config.clone();
        let wallet_type = self.wallet_type.clone();
        let network = self.network;

        context.spawn_when_ready(move |handles| async move {
            let outbound_message_service = handles.expect_handle::<OutboundMessaging<TariNodeMessageSpec>>();
            let output_manager_service = handles.expect_handle::<OutputManagerHandle>();
            let core_key_manager_service = handles.expect_handle::<TKeyManagerInterface>();
            let connectivity = handles.expect_handle::<WalletConnectivityHandle>();
            let base_node_service_handle = handles.expect_handle::<BaseNodeServiceHandle>();

            let result = TransactionService::new(
                config,
                TransactionDatabase::new(tx_backend),
                wallet_database,
                receiver,
                rx_messages,
                output_manager_service,
                core_key_manager_service,
                outbound_message_service,
                connectivity,
                publisher,
                node_identity,
                network,
                consensus_manager,
                factories,
                handles.get_shutdown_signal(),
                base_node_service_handle,
                wallet_type,
            )
            .await
            .expect("Could not initialize Transaction Manager Service")
            .start()
            .await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Transaction Service error: {}", e);
            }
            info!(target: LOG_TARGET, "Transaction Service shutdown");
        });

        Ok(())
    }
}
