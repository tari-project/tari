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

use std::fmt::Debug;

use tari_common_types::tari_address::TariAddress;
use tari_core::transactions::transaction_key_manager::TransactionKeyManagerInterface;
use tari_shutdown::ShutdownSignal;
use tokio::sync::broadcast;

use crate::{
    client::http_client_factory::HttpClientFactory,
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

#[derive(Default, Clone, PartialEq)]
pub enum UtxoScannerMode {
    #[default]
    Recovery,
    Scanning,
}

impl Debug for UtxoScannerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UtxoScannerMode::Recovery => write!(f, "UtxoRecoveryMode"),
            UtxoScannerMode::Scanning => write!(f, "UtxoScanningMode"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UtxoScannerServiceBuilder<TWalletClientFactory> {
    retry_limit: usize,
    mode: Option<UtxoScannerMode>,
    one_sided_message: String,
    recovery_message: String,
    client_factory: Option<TWalletClientFactory>,
    scanning_interval: u64,
}

impl<T> Default for UtxoScannerServiceBuilder<T> {
    fn default() -> Self {
        Self {
            retry_limit: 0,
            mode: None,
            one_sided_message: "Detected one-sided payment on blockchain".to_string(),
            recovery_message: "Output found on blockchain during Wallet Recovery".to_string(),
            client_factory: None,
            scanning_interval: 60, // Default scanning interval in seconds
        }
    }
}

impl<T: HttpClientFactory + Clone + Send + Sync + 'static> UtxoScannerServiceBuilder<T> {
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

    pub fn with_client_factory(&mut self, factory: T) -> &mut Self {
        self.client_factory = Some(factory);
        self
    }

    pub fn with_scanning_interval(&mut self, interval: u64) -> &mut Self {
        self.scanning_interval = interval;
        self
    }

    pub async fn build_with_wallet(
        &mut self,
        wallet: &WalletSqlite,
        shutdown_signal: ShutdownSignal,
    ) -> Result<UtxoScannerService<WalletSqliteDatabase, WalletKeyManager, T>, anyhow::Error> {
        let one_sided_tari_address = wallet.get_wallet_one_sided_address().await?;
        let client_factory = match &self.client_factory {
            Some(t) => t.clone(),
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
            birthday_offset: wallet.config.birthday_offset,
            client_factory: client_factory.clone(),
        };

        let (event_sender, _) = broadcast::channel(2000);

        Ok(UtxoScannerService::new(
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            shutdown_signal,
            wallet.config.scanning_interval,
            event_sender,
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
        birthday_offset: u16,
        key_manager: TKeyManager,
    ) -> Result<UtxoScannerService<TBackend, TKeyManager, T>, anyhow::Error> {
        let client_factory = match &self.client_factory {
            Some(factory) => factory.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "No client factory was set before building the UTXO scanner service."
                ))
            },
        };

        let resources = UtxoScannerResources {
            db,
            output_manager_service,
            transaction_service,
            one_sided_tari_address,
            birthday_offset,
            client_factory,
        };

        Ok(UtxoScannerService::new(
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            shutdown_signal,
            self.scanning_interval,
            event_sender,
            key_manager,
        ))
    }
}
