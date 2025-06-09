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
use tari_core::transactions::transaction_key_manager::TransactionKeyManagerInterface;
use tari_shutdown::ShutdownSignal;
use tokio::sync::{broadcast, watch};
use url::Url;

use crate::{
    base_node_service::handle::BaseNodeServiceHandle,
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
    WalletKeyManager,
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
    mode: Option<UtxoScannerMode>,
    one_sided_message: String,
    recovery_message: String,
    node_url: Option<Url>,
}

impl Default for UtxoScannerServiceBuilder {
    fn default() -> Self {
        Self {
            retry_limit: 0,
            mode: None,
            one_sided_message: "Detected one-sided payment on blockchain".to_string(),
            recovery_message: "Output found on blockchain during Wallet Recovery".to_string(),
            node_url: None,
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

    pub fn with_http_node_url(&mut self, node_url: Url) -> &mut Self {
        self.node_url = Some(node_url);
        self
    }

    pub async fn build_with_wallet(
        &mut self,
        wallet: &WalletSqlite,
        shutdown_signal: ShutdownSignal,
    ) -> Result<UtxoScannerService<WalletSqliteDatabase, WalletKeyManager>, anyhow::Error> {
        let one_sided_tari_address = wallet.get_wallet_one_sided_address().await?;
        let http_client_url = match &self.node_url {
            Some(url) => url.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "Node URL must be set before building the UTXO scanner service."
                ))
            },
        };
        let resources = UtxoScannerResources {
            db: wallet.db.clone(),
            output_manager_service: wallet.output_manager_service.clone(),
            transaction_service: wallet.transaction_service.clone(),
            one_sided_tari_address,
            recovery_message: self.recovery_message.clone(),
            one_sided_payment_message: self.one_sided_message.clone(),
            birthday_offset: wallet.config.birthday_offset,
            http_client_url,
        };

        let (event_sender, _) = broadcast::channel(200);

        Ok(UtxoScannerService::new(
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            shutdown_signal,
            event_sender,
            wallet.base_node_service.clone(),
            wallet.utxo_scanner_service.get_one_sided_payment_message_watcher(),
            wallet.utxo_scanner_service.get_recovery_message_watcher(),
            wallet.key_manager_service.clone(),
        ))
    }

    pub async fn build_with_resources<
        TBackend: WalletBackend + 'static,
        TKeyManager: TransactionKeyManagerInterface + 'static,
    >(
        &mut self,
        db: WalletDatabase<TBackend>,
        output_manager_service: OutputManagerHandle,
        transaction_service: TransactionServiceHandle,
        one_sided_tari_address: TariAddress,
        shutdown_signal: ShutdownSignal,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
        base_node_service: BaseNodeServiceHandle,
        one_sided_message_watch: watch::Receiver<String>,
        recovery_message_watch: watch::Receiver<String>,
        birthday_offset: u16,
        key_manager: TKeyManager,
    ) -> Result<UtxoScannerService<TBackend, TKeyManager>, anyhow::Error> {
        let http_client_url = match &self.node_url {
            Some(url) => url.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "Node URL must be set before building the UTXO scanner service."
                ))
            },
        };
        let resources = UtxoScannerResources {
            db,
            output_manager_service,
            transaction_service,
            one_sided_tari_address,
            recovery_message: self.recovery_message.clone(),
            one_sided_payment_message: self.one_sided_message.clone(),
            birthday_offset,
            http_client_url,
        };

        Ok(UtxoScannerService::new(
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            shutdown_signal,
            event_sender,
            base_node_service,
            one_sided_message_watch,
            recovery_message_watch,
            key_manager,
        ))
    }
}
