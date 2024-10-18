// Copyright 2021. The Tari Project
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

use tari_common_types::tari_address::TariAddress;
use tari_core::transactions::{key_manager::TransactionKeyManagerInterface, CryptoFactories};
use tari_key_manager::key_manager_service::KeyManagerServiceError;
use tari_network::{identity::PeerId, NetworkHandle};
use tari_shutdown::ShutdownSignal;
use tokio::sync::{broadcast, watch};

use crate::{
    base_node_service::handle::BaseNodeServiceHandle,
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInterface},
    output_manager_service::handle::OutputManagerHandle,
    storage::{
        database::{WalletBackend, WalletDatabase},
        sqlite_db::wallet::WalletSqliteDatabase,
    },
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::{
        handle::UtxoScannerEvent,
        service::{UtxoScannerResources, UtxoScannerService},
    },
    WalletSqlite,
};

#[derive(Default, Debug, Clone, PartialEq)]
pub enum UtxoScannerMode {
    #[default]
    Recovery,
    Scanning,
}

#[derive(Debug, Clone)]
pub struct UtxoScannerServiceBuilder {
    retry_limit: usize,
    peers: Vec<PeerId>,
    mode: Option<UtxoScannerMode>,
    one_sided_message: String,
    recovery_message: String,
}

impl Default for UtxoScannerServiceBuilder {
    fn default() -> Self {
        Self {
            retry_limit: 0,
            peers: vec![],
            mode: None,
            one_sided_message: "Detected one-sided payment on blockchain".to_string(),
            recovery_message: "Output found on blockchain during Wallet Recovery".to_string(),
        }
    }
}

impl UtxoScannerServiceBuilder {
    /// Set the maximum number of times we retry recovery. A failed recovery is counted as _all_ peers have failed.
    /// i.e. worst-case number of recovery attempts = number of sync peers * retry limit
    pub fn with_retry_limit(&mut self, limit: usize) -> &mut Self {
        self.retry_limit = limit;
        self
    }

    pub fn with_peers(&mut self, peers: Vec<PeerId>) -> &mut Self {
        self.peers = peers;
        self
    }

    pub fn with_mode(&mut self, mode: UtxoScannerMode) -> &mut Self {
        self.mode = Some(mode);
        self
    }

    pub fn with_one_sided_message(&mut self, message: String) -> &mut Self {
        self.one_sided_message = message;
        self
    }

    pub fn with_recovery_message(&mut self, message: String) -> &mut Self {
        self.recovery_message = message;
        self
    }

    pub async fn build_with_wallet(
        &mut self,
        wallet: &WalletSqlite,
        shutdown_signal: ShutdownSignal,
    ) -> Result<UtxoScannerService<WalletSqliteDatabase, WalletConnectivityHandle>, KeyManagerServiceError> {
        let one_sided_tari_address = wallet.get_wallet_one_sided_address().await?;
        let resources = UtxoScannerResources {
            db: wallet.db.clone(),
            network: wallet.network.clone(),
            wallet_connectivity: wallet.wallet_connectivity.clone(),
            current_base_node_watcher: wallet.wallet_connectivity.get_current_base_node_watcher(),
            output_manager_service: wallet.output_manager_service.clone(),
            transaction_service: wallet.transaction_service.clone(),
            one_sided_tari_address,
            factories: wallet.factories.clone(),
            recovery_message: self.recovery_message.clone(),
            one_sided_payment_message: self.one_sided_message.clone(),
        };

        let (event_sender, _) = broadcast::channel(200);

        Ok(UtxoScannerService::new(
            self.peers.drain(..).collect(),
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            shutdown_signal,
            event_sender,
            wallet.base_node_service.clone(),
            wallet.utxo_scanner_service.get_one_sided_payment_message_watcher(),
            wallet.utxo_scanner_service.get_recovery_message_watcher(),
        ))
    }

    pub async fn build_with_resources<
        TBackend: WalletBackend + 'static,
        TWalletConnectivity: WalletConnectivityInterface,
        TKeyManagerInterface: TransactionKeyManagerInterface,
    >(
        &mut self,
        db: WalletDatabase<TBackend>,
        network: NetworkHandle,
        wallet_connectivity: TWalletConnectivity,
        output_manager_service: OutputManagerHandle,
        transaction_service: TransactionServiceHandle,
        one_sided_tari_address: TariAddress,
        factories: CryptoFactories,
        shutdown_signal: ShutdownSignal,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
        base_node_service: BaseNodeServiceHandle,
        one_sided_message_watch: watch::Receiver<String>,
        recovery_message_watch: watch::Receiver<String>,
    ) -> UtxoScannerService<TBackend, TWalletConnectivity> {
        let resources = UtxoScannerResources {
            db,
            network,
            current_base_node_watcher: wallet_connectivity.get_current_base_node_watcher(),
            wallet_connectivity,
            output_manager_service,
            transaction_service,
            one_sided_tari_address,
            factories,
            recovery_message: self.recovery_message.clone(),
            one_sided_payment_message: self.one_sided_message.clone(),
        };

        UtxoScannerService::new(
            self.peers.drain(..).collect(),
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            shutdown_signal,
            event_sender,
            base_node_service,
            one_sided_message_watch,
            recovery_message_watch,
        )
    }
}
