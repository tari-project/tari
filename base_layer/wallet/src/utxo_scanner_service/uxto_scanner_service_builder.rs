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

use log::*;
use tari_common_types::{tari_address::TariAddress, types::CompressedCommitment};
use tari_shutdown::ShutdownSignal;
use tari_transaction_key_manager::legacy_key_manager::LegacyTransactionKeyManagerInterface;
use tokio::sync::broadcast;

use crate::{
    WalletKeyManager,
    WalletSqlite,
    client::http_client_factory::HttpClientFactory,
    output_manager_service::handle::OutputManagerHandle,
    storage::{
        database::{WalletBackend, WalletDatabase},
        sqlite_db::wallet::WalletSqliteDatabase,
    },
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::{
        handle::UtxoScannerEvent,
        service::{LOG_TARGET, UtxoScannerResources, UtxoScannerService},
    },
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
    client_factory: Option<TWalletClientFactory>,
    scanning_interval: u64,
    excluded_commitments: Vec<CompressedCommitment>,
}

impl<T> Default for UtxoScannerServiceBuilder<T> {
    fn default() -> Self {
        Self {
            retry_limit: 0,
            mode: None,
            client_factory: None,
            scanning_interval: 60, // Default scanning interval in seconds
            excluded_commitments: Vec::new(),
        }
    }
}

impl<T> UtxoScannerServiceBuilder<T> {
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

    pub fn with_scanning_interval(&mut self, interval: u64) -> &mut Self {
        self.scanning_interval = interval;
        self
    }

    pub fn with_excluded_commitments(&mut self, excluded_commitments: Vec<CompressedCommitment>) -> &mut Self {
        self.excluded_commitments = excluded_commitments;
        self
    }
}

impl<T: HttpClientFactory + Clone + Send + Sync + 'static> UtxoScannerServiceBuilder<T> {
    pub fn with_client_factory(&mut self, factory: T) -> &mut Self {
        self.client_factory = Some(factory);
        self
    }

    pub async fn build_with_wallet(
        &mut self,
        wallet: &WalletSqlite,
        shutdown_signal: ShutdownSignal,
    ) -> Result<UtxoScannerService<WalletSqliteDatabase, WalletKeyManager, T>, anyhow::Error> {
        let one_sided_tari_address = wallet.get_wallet_one_sided_address()?;
        let client_factory = match &self.client_factory {
            Some(t) => t.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "Node URL must be set before building the UTXO scanner service."
                ));
            },
        };

        let mut excluded_commitments = self.excluded_commitments.clone();
        match wallet.config.get_excluded_commitments() {
            Ok(commitments) => {
                for c in commitments {
                    if !excluded_commitments.contains(&c) {
                        excluded_commitments.push(c);
                    }
                }
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to parse excluded commitment from wallet config: {e}"
                );
            },
        }

        let resources = UtxoScannerResources {
            db: wallet.db.clone(),
            output_manager_service: wallet.output_manager_service.clone(),
            transaction_service: wallet.transaction_service.clone(),
            one_sided_tari_address,
            birthday_offset: wallet.config.birthday_offset,
            client_factory: client_factory.clone(),
            excluded_commitments,
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
        TKeyManager: LegacyTransactionKeyManagerInterface + 'static,
    >(
        &mut self,
        db: WalletDatabase<TBackend>,
        output_manager_service: OutputManagerHandle<TKeyManager>,
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
                ));
            },
        };

        let resources = UtxoScannerResources {
            db,
            output_manager_service,
            transaction_service,
            one_sided_tari_address,
            birthday_offset,
            client_factory,
            excluded_commitments: self.excluded_commitments.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tari_utilities::hex::Hex;

    #[test]
    fn test_builder_with_excluded_commitments() {
        let mut builder = UtxoScannerServiceBuilder::<()>::default();
        assert!(builder.excluded_commitments.is_empty());

        let comm = CompressedCommitment::from_hex("006399307893ae875ac7677b564ba068a9bc18eb903f5245a39a78aeebecc87b").unwrap();
        builder.with_excluded_commitments(vec![comm.clone()]);
        assert_eq!(builder.excluded_commitments.len(), 1);
        assert_eq!(builder.excluded_commitments[0], comm);
    }
}

