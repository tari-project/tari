use std::sync::Arc;

use futures::future;
use log::*;
use tari_comms::{connectivity::ConnectivityRequester, NodeIdentity};
use tari_core::transactions::CryptoFactories;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::sync::broadcast;

use crate::{
    base_node_service::handle::BaseNodeServiceHandle,
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInterface},
    output_manager_service::handle::OutputManagerHandle,
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::{
        handle::UtxoScannerHandle,
        service::UtxoScannerService,
        uxto_scanner_service_builder::UtxoScannerMode,
    },
};

pub mod error;
pub mod handle;
pub mod service;
mod utxo_scanner_task;
pub mod uxto_scanner_service_builder;

pub use utxo_scanner_task::RECOVERY_KEY;

const LOG_TARGET: &str = "wallet::utxo_scanner_service::initializer";

pub struct UtxoScannerServiceInitializer<T> {
    backend: Option<WalletDatabase<T>>,
    factories: CryptoFactories,
    node_identity: Arc<NodeIdentity>,
}

impl<T> UtxoScannerServiceInitializer<T>
where T: WalletBackend + 'static
{
    pub fn new(backend: WalletDatabase<T>, factories: CryptoFactories, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            backend: Some(backend),
            factories,
            node_identity,
        }
    }
}

#[async_trait]
impl<T> ServiceInitializer for UtxoScannerServiceInitializer<T>
where T: WalletBackend + 'static
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        trace!(target: LOG_TARGET, "Utxo scanner initialization");

        let (event_sender, _) = broadcast::channel(200);

        // Register handle before waiting for handles to be ready
        let utxo_scanner_handle = UtxoScannerHandle::new(event_sender.clone());
        context.register_handle(utxo_scanner_handle);

        let backend = self
            .backend
            .take()
            .expect("Cannot start Utxo scanner service without setting a storage backend");
        let factories = self.factories.clone();
        let node_identity = self.node_identity.clone();

        context.spawn_when_ready(move |handles| async move {
            let transaction_service = handles.expect_handle::<TransactionServiceHandle>();
            let output_manager_service = handles.expect_handle::<OutputManagerHandle>();
            let comms_connectivity = handles.expect_handle::<ConnectivityRequester>();
            let wallet_connectivity = handles.expect_handle::<WalletConnectivityHandle>();
            let base_node_service_handle = handles.expect_handle::<BaseNodeServiceHandle>();

            let scanning_service = UtxoScannerService::<T>::builder()
                .with_peers(vec![])
                .with_retry_limit(2)
                .with_mode(UtxoScannerMode::Scanning)
                .build_with_resources(
                    backend,
                    comms_connectivity,
                    wallet_connectivity.get_current_base_node_watcher(),
                    output_manager_service,
                    transaction_service,
                    node_identity,
                    factories,
                    handles.get_shutdown_signal(),
                    event_sender,
                    base_node_service_handle,
                )
                .run();

            futures::pin_mut!(scanning_service);
            future::select(scanning_service, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Utxo scanner service shutdown");
        });
        Ok(())
    }
}
