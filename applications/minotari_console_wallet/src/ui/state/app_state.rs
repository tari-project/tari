// Copyright 2020. The Tari Project
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
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Local, NaiveDateTime};
use log::*;
use minotari_wallet::{
    base_node_service::{handle::BaseNodeEventReceiver, service::BaseNodeState},
    connectivity_service::{OnlineStatus, WalletConnectivityHandle, WalletConnectivityInterface},
    output_manager_service::{handle::OutputManagerEventReceiver, service::Balance, UtxoSelectionCriteria},
    transaction_service::{
        handle::TransactionEventReceiver,
        storage::models::{CompletedTransaction, TxCancellationReason},
    },
    util::wallet_identity::WalletIdentity,
    utxo_scanner_service::handle::UtxoScannerHandle,
    WalletConfig,
    WalletSqlite,
};
use qrcode::{render::unicode, QrCode};
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::{TransactionDirection, TransactionStatus, TxId},
    types::PublicKey,
    wallet_types::WalletType,
};
use tari_contacts::contacts_service::{handle::ContactsLivenessEvent, types::Contact};
use tari_core::transactions::{
    tari_amount::{uT, MicroMinotari},
    transaction_components::{encrypted_data::PaymentId, OutputFeatures, TemplateType, TransactionError},
    weight::TransactionWeight,
};
use tari_network::{identity, multiaddr::Multiaddr, public_key_to_string, Connection, NetworkEvent, Peer};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{broadcast, watch, RwLock},
    task,
};

use super::tasks::send_one_sided_to_stealth_address_transaction;
use crate::{
    notifier::Notifier,
    ui::{
        state::{
            debouncer::BalanceEnquiryDebouncer,
            tasks::{send_burn_transaction_task, send_register_template_transaction_task, send_transaction_task},
            wallet_event_monitor::WalletEventMonitor,
        },
        ui_burnt_proof::UiBurntProof,
        ui_contact::UiContact,
        ui_error::UiError,
    },
    utils::db::{CUSTOM_BASE_NODE_ADDRESS_KEY, CUSTOM_BASE_NODE_PUBLIC_KEY_KEY},
    wallet_modes::PeerConfig,
};

const LOG_TARGET: &str = "wallet::console_wallet::app_state";

#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<AppStateInner>>,
    cached_data: AppStateData,
    cache_update_cooldown: Option<Instant>,
    completed_tx_filter: TransactionFilter,
    config: AppStateConfig,
    wallet_config: WalletConfig,
    wallet_connectivity: WalletConnectivityHandle,
    balance_enquiry_debouncer: BalanceEnquiryDebouncer,
}

impl AppState {
    pub fn new(
        wallet_identity: &WalletIdentity,
        wallet: WalletSqlite,
        base_node_selected: Peer,
        base_node_config: PeerConfig,
        wallet_config: WalletConfig,
    ) -> Self {
        let wallet_connectivity = wallet.wallet_connectivity.clone();
        let output_manager_service = wallet.output_manager_service.clone();
        let inner = AppStateInner::new(wallet_identity, wallet, base_node_selected, base_node_config);
        let cached_data = inner.data.clone();

        let inner = Arc::new(RwLock::new(inner));
        Self {
            inner: inner.clone(),
            cached_data,
            cache_update_cooldown: None,
            config: AppStateConfig::default(),
            completed_tx_filter: TransactionFilter::AbandonedCoinbases,
            wallet_connectivity,
            balance_enquiry_debouncer: BalanceEnquiryDebouncer::new(
                inner,
                wallet_config.balance_enquiry_cooldown_period,
                output_manager_service,
            ),
            wallet_config,
        }
    }

    pub async fn start_event_monitor(&self, notifier: Notifier) {
        let balance_enquiry_debounce_tx = self.balance_enquiry_debouncer.clone().get_sender();
        let event_monitor = WalletEventMonitor::new(self.inner.clone(), balance_enquiry_debounce_tx);
        tokio::spawn(event_monitor.run(notifier));
    }

    pub fn get_all_events(&self) -> VecDeque<EventListItem> {
        self.cached_data.all_events.to_owned()
    }

    pub async fn start_balance_enquiry_debouncer(&self) -> Result<(), UiError> {
        tokio::spawn(self.balance_enquiry_debouncer.clone().run());
        let _size = self
            .balance_enquiry_debouncer
            .clone()
            .get_sender()
            .send(())
            .map_err(|e| UiError::SendError(e.to_string()));
        Ok(())
    }

    pub async fn refresh_transaction_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_full_transaction_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    pub async fn refresh_contacts_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_contacts_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    pub async fn refresh_burnt_proofs_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_burnt_proofs_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    pub async fn refresh_connected_peers_state(&mut self) -> Result<(), UiError> {
        self.check_connectivity().await;
        let mut inner = self.inner.write().await;
        inner.refresh_connected_peers_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    pub fn toggle_abandoned_coinbase_filter(&mut self) {
        self.completed_tx_filter = match self.completed_tx_filter {
            TransactionFilter::AbandonedCoinbases => TransactionFilter::None,
            TransactionFilter::None => TransactionFilter::AbandonedCoinbases,
        };
    }

    pub async fn update_cache(&mut self) {
        let update = match self.cache_update_cooldown {
            Some(last_update) => last_update.elapsed() > self.config.cache_update_cooldown,
            None => true,
        };

        if update {
            let mut inner = self.inner.write().await;
            let updated_state = inner.get_updated_app_state();
            if let Some(data) = updated_state {
                self.cached_data = data;
                self.cache_update_cooldown = Some(Instant::now());
            }
        }
    }

    pub async fn check_connectivity(&mut self) {
        if self.get_custom_base_node().is_none() &&
            self.wallet_connectivity.get_connectivity_status() == OnlineStatus::Offline
        {
            let current = self.get_selected_base_node();
            let list = self.get_base_node_list().clone();
            let mut index: usize = list
                .iter()
                .position(|(_, p)| p.peer_id() == current.peer_id())
                .unwrap_or_default();
            if !list.is_empty() {
                if index == list.len() - 1 {
                    index = 0;
                } else {
                    index += 1;
                }
                let (_, next) = &list[index];
                if let Err(e) = self.set_base_node_peer(next.clone()).await {
                    error!(target: LOG_TARGET, "Base node offline: {:?}", e);
                }
            }
        }
    }

    pub async fn upsert_contact(&mut self, alias: String, tari_emoji: String) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;

        let address = TariAddress::from_str(&tari_emoji).map_err(|_| UiError::PublicKeyParseError)?;

        let contact = Contact::new(alias, address, None, None, false);
        inner.wallet.contacts_service.upsert_contact(contact).await?;

        inner.refresh_contacts_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    // Return alias or pub key if the contact is not in the list.
    pub fn get_alias(&self, address_string: String) -> String {
        if address_string == TariAddress::default().to_base58() {
            return "Offline payment".to_string();
        }

        match self
            .cached_data
            .contacts
            .iter()
            .find(|&contact| contact.address.eq(&address_string))
        {
            Some(contact) => contact.alias.clone(),
            None => address_string,
        }
    }

    pub async fn delete_contact(&mut self, tari_emoji: String) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        let address = TariAddress::from_str(&tari_emoji).map_err(|_| UiError::PublicKeyParseError)?;

        inner.wallet.contacts_service.remove_contact(address).await?;

        inner.refresh_contacts_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    pub async fn delete_burnt_proof(&mut self, proof_id: u32) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;

        inner
            .wallet
            .db
            .delete_burnt_proof(proof_id)
            .map_err(UiError::WalletStorageError)?;

        inner.refresh_burnt_proofs_state().await?;
        drop(inner);
        self.update_cache().await;

        Ok(())
    }

    pub async fn send_transaction(
        &mut self,
        address: String,
        amount: u64,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: u64,
        message: String,
        result_tx: watch::Sender<UiTransactionSendStatus>,
    ) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let address = TariAddress::from_str(&address).map_err(|_| UiError::PublicKeyParseError)?;

        let output_features = OutputFeatures { ..Default::default() };

        let fee_per_gram = fee_per_gram * uT;
        let tx_service_handle = inner.wallet.transaction_service.clone();
        tokio::spawn(send_transaction_task(
            address,
            MicroMinotari::from(amount),
            selection_criteria,
            output_features,
            message,
            fee_per_gram,
            tx_service_handle,
            result_tx,
        ));

        Ok(())
    }

    pub async fn send_one_sided_to_stealth_address_transaction(
        &mut self,
        address: String,
        amount: u64,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: u64,
        message: String,
        payment_id_str: String,
        result_tx: watch::Sender<UiTransactionSendStatus>,
    ) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let address = TariAddress::from_str(&address).map_err(|_| UiError::PublicKeyParseError)?;
        let payment_id = if payment_id_str.is_empty() {
            PaymentId::Empty
        } else {
            let bytes = payment_id_str.as_bytes().to_vec();
            PaymentId::Open(bytes)
        };

        let output_features = OutputFeatures { ..Default::default() };

        let fee_per_gram = fee_per_gram * uT;
        let tx_service_handle = inner.wallet.transaction_service.clone();
        tokio::spawn(send_one_sided_to_stealth_address_transaction(
            address,
            MicroMinotari::from(amount),
            selection_criteria,
            output_features,
            message,
            fee_per_gram,
            payment_id,
            tx_service_handle,
            result_tx,
        ));

        Ok(())
    }

    pub async fn send_burn_transaction(
        &mut self,
        burn_proof_filepath: Option<String>,
        claim_public_key: Option<String>,
        amount: u64,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: u64,
        message: String,
        result_tx: watch::Sender<UiTransactionBurnStatus>,
    ) -> Result<(), UiError> {
        let inner = self.inner.write().await;

        let burn_proof_filepath = match burn_proof_filepath {
            None => None,
            Some(path) => {
                let path = PathBuf::from(path);

                if path.exists() {
                    return Err(UiError::BurntProofFileExists);
                }

                Some(path)
            },
        };

        let fee_per_gram = fee_per_gram * uT;
        let tx_service_handle = inner.wallet.transaction_service.clone();
        let claim_public_key = match claim_public_key {
            None => return Err(UiError::PublicKeyParseError),
            Some(claim_public_key) => match PublicKey::from_hex(claim_public_key.as_str()) {
                Ok(claim_public_key) => Some(claim_public_key),
                Err(_) => return Err(UiError::PublicKeyParseError),
            },
        };

        send_burn_transaction_task(
            burn_proof_filepath,
            claim_public_key,
            MicroMinotari::from(amount),
            selection_criteria,
            message,
            fee_per_gram,
            tx_service_handle,
            inner.wallet.db.clone(),
            result_tx,
        )
        .await;

        Ok(())
    }

    pub async fn register_code_template(
        &mut self,
        template_name: String,
        template_version: u16,
        template_type: TemplateType,
        binary_url: String,
        binary_sha: String,
        repository_url: String,
        repository_commit_hash: String,
        fee_per_gram: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        result_tx: watch::Sender<UiTransactionSendStatus>,
    ) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let tx_service_handle = inner.wallet.transaction_service.clone();

        send_register_template_transaction_task(
            template_name,
            template_version,
            template_type,
            repository_url,
            repository_commit_hash,
            binary_url,
            binary_sha,
            fee_per_gram,
            selection_criteria,
            tx_service_handle,
            inner.wallet.db.clone(),
            result_tx,
        )
        .await;

        Ok(())
    }

    pub async fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let mut tx_service_handle = inner.wallet.transaction_service.clone();
        tx_service_handle.cancel_transaction(tx_id).await?;
        Ok(())
    }

    pub async fn rebroadcast_all(&mut self) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let mut tx_service = inner.wallet.transaction_service.clone();
        tx_service.restart_broadcast_protocols().await?;
        Ok(())
    }

    pub async fn restart_transaction_protocols(&mut self) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let mut tx_service = inner.wallet.transaction_service.clone();
        tx_service.restart_transaction_protocols().await?;
        Ok(())
    }

    pub fn get_identity(&self) -> &MyIdentity {
        &self.cached_data.my_identity
    }

    pub fn get_burnt_proofs(&self) -> &[UiBurntProof] {
        self.cached_data.burnt_proofs.as_slice()
    }

    pub fn get_burnt_proof_by_index(&self, idx: usize) -> Option<&UiBurntProof> {
        self.cached_data.burnt_proofs.get(idx)
    }

    pub fn get_contacts(&self) -> &[UiContact] {
        self.cached_data.contacts.as_slice()
    }

    pub fn get_contact(&self, index: usize) -> Option<&UiContact> {
        if index < self.cached_data.contacts.len() {
            Some(&self.cached_data.contacts[index])
        } else {
            None
        }
    }

    pub fn get_contacts_slice(&self, start: usize, end: usize) -> &[UiContact] {
        if self.cached_data.contacts.is_empty() || start > end || end > self.cached_data.contacts.len() {
            return &[];
        }

        &self.cached_data.contacts[start..end]
    }

    pub fn get_burnt_proofs_slice(&self, start: usize, end: usize) -> &[UiBurntProof] {
        if self.cached_data.burnt_proofs.is_empty() || start >= end {
            return &[];
        }

        &self.cached_data.burnt_proofs[start..end]
    }

    pub fn get_pending_txs(&self) -> &Vec<CompletedTransactionInfo> {
        &self.cached_data.pending_txs
    }

    pub fn get_pending_txs_slice(&self, start: usize, end: usize) -> &[CompletedTransactionInfo] {
        if self.cached_data.pending_txs.is_empty() || start > end || end > self.cached_data.pending_txs.len() {
            return &[];
        }

        &self.cached_data.pending_txs[start..end]
    }

    pub fn get_pending_tx(&self, index: usize) -> Option<&CompletedTransactionInfo> {
        if index < self.cached_data.pending_txs.len() {
            Some(&self.cached_data.pending_txs[index])
        } else {
            None
        }
    }

    pub fn get_completed_txs(&self) -> Vec<&CompletedTransactionInfo> {
        if self.completed_tx_filter == TransactionFilter::AbandonedCoinbases {
            self.cached_data
                .completed_txs
                .iter()
                .filter(|tx| !matches!(tx.status, TransactionStatus::CoinbaseNotInBlockChain))
                .collect()
        } else {
            self.cached_data.completed_txs.iter().collect()
        }
    }

    pub fn get_confirmations(&self, tx_id: TxId) -> Option<&u64> {
        self.cached_data.confirmations.get(&tx_id)
    }

    pub fn get_completed_tx(&self, index: usize) -> Option<&CompletedTransactionInfo> {
        let filtered_completed_txs = self.get_completed_txs();
        if index < filtered_completed_txs.len() {
            Some(filtered_completed_txs[index])
        } else {
            None
        }
    }

    pub fn get_connected_peers(&self) -> &[Connection] {
        &self.cached_data.connected_peers
    }

    pub fn get_balance(&self) -> &Balance {
        &self.cached_data.balance
    }

    pub fn get_base_node_state(&self) -> &BaseNodeState {
        &self.cached_data.base_node_state
    }

    pub fn get_wallet_scanned_height(&self) -> u64 {
        self.cached_data.wallet_scanned_height
    }

    pub fn get_wallet_connectivity(&self) -> WalletConnectivityHandle {
        self.wallet_connectivity.clone()
    }

    pub fn get_selected_base_node(&self) -> &Peer {
        &self.cached_data.base_node_selected
    }

    pub fn get_previous_base_node(&self) -> &Peer {
        &self.cached_data.base_node_previous
    }

    pub fn get_custom_base_node(&self) -> &Option<Peer> {
        &self.cached_data.base_node_peer_custom
    }

    pub fn get_base_node_list(&self) -> &Vec<(String, Peer)> {
        &self.cached_data.base_node_list
    }

    pub async fn set_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.set_base_node_peer(peer).await?;
        Ok(())
    }

    pub async fn set_custom_base_node(&mut self, public_key: String, address: String) -> Result<Peer, UiError> {
        let pub_key = PublicKey::from_hex(public_key.as_str())?;
        let addr = address.parse::<Multiaddr>().map_err(|_| UiError::AddressParseError)?;
        let pub_key = identity::PublicKey::from(identity::sr25519::PublicKey::from(pub_key));
        let peer = Peer::new(pub_key, vec![addr]);

        let mut inner = self.inner.write().await;
        inner.set_custom_base_node_peer(peer.clone()).await?;
        Ok(peer)
    }

    pub async fn clear_custom_base_node(&mut self) -> Result<(), UiError> {
        {
            let mut inner = self.inner.write().await;
            inner.clear_custom_base_node_peer().await?;
        }
        self.update_cache().await;
        Ok(())
    }

    pub fn get_required_confirmations(&self) -> u64 {
        self.wallet_config.num_required_confirmations
    }

    pub fn get_notifications(&self) -> &[(DateTime<Local>, String)] {
        &self.cached_data.notifications
    }

    pub fn unread_notifications_count(&self) -> u32 {
        self.cached_data.new_notification_count
    }

    pub async fn mark_notifications_as_read(&mut self) {
        // Do not update if not necessary
        if self.unread_notifications_count() > 0 {
            {
                let mut inner = self.inner.write().await;
                inner.mark_notifications_as_read();
            }
            self.update_cache().await;
        }
    }

    pub async fn clear_notifications(&mut self) {
        let mut inner = self.inner.write().await;
        inner.clear_notifications();
    }

    pub fn get_default_fee_per_gram(&self) -> MicroMinotari {
        self.wallet_config.fee_per_gram.into()
    }

    pub async fn get_network(&self) -> Network {
        self.inner.read().await.get_network()
    }

    pub async fn get_wallet_type(&self) -> Result<WalletType, UiError> {
        let inner = self.inner.write().await;
        inner.get_wallet_type()
    }
}
pub struct AppStateInner {
    updated: bool,
    data: AppStateData,
    wallet: WalletSqlite,
}

impl AppStateInner {
    pub fn new(
        wallet_identity: &WalletIdentity,
        wallet: WalletSqlite,
        base_node_selected: Peer,
        base_node_config: PeerConfig,
    ) -> Self {
        let data = AppStateData::new(wallet_identity, base_node_selected, base_node_config);

        AppStateInner {
            updated: false,
            data,
            wallet,
        }
    }

    pub fn get_wallet_type(&self) -> Result<WalletType, UiError> {
        self.wallet
            .db
            .get_wallet_type()
            .map_err(UiError::WalletStorageError)
            .and_then(|opt| opt.ok_or(UiError::WalletTypeError))
    }

    pub fn get_network(&self) -> Network {
        self.wallet.network_consensus.as_network()
    }

    pub fn add_event(&mut self, event: EventListItem) {
        if self.data.all_events.len() > 30 {
            self.data.all_events.pop_back();
        }
        self.data.all_events.push_front(event);
        self.updated = true;
    }

    /// If there has been an update to the state since the last call to this function it will provide a cloned snapshot
    /// of the data for caching, if there has been no change then returns None
    fn get_updated_app_state(&mut self) -> Option<AppStateData> {
        if self.updated {
            self.updated = false;
            Some(self.data.clone())
        } else {
            None
        }
    }

    pub fn get_transaction_weight(&self) -> TransactionWeight {
        *self
            .wallet
            .network_consensus
            .create_consensus_constants()
            .last()
            .unwrap()
            .transaction_weight_params()
    }

    pub async fn refresh_full_transaction_state(&mut self) -> Result<(), UiError> {
        let mut pending_transactions: Vec<CompletedTransaction> = Vec::new();
        pending_transactions.extend(
            self.wallet
                .transaction_service
                .get_pending_inbound_transactions()
                .await?
                .values()
                .map(|t| CompletedTransaction::from(t.clone()))
                .collect::<Vec<CompletedTransaction>>(),
        );
        pending_transactions.extend(
            self.wallet
                .transaction_service
                .get_pending_outbound_transactions()
                .await?
                .values()
                .map(|t| CompletedTransaction::from(t.clone()))
                .collect::<Vec<CompletedTransaction>>(),
        );

        pending_transactions.sort_by(|a: &CompletedTransaction, b: &CompletedTransaction| {
            b.timestamp.partial_cmp(&a.timestamp).unwrap()
        });
        self.data.pending_txs = pending_transactions
            .iter()
            .map(|tx| {
                CompletedTransactionInfo::from_completed_transaction(tx.clone(), &self.get_transaction_weight())
                    .map_err(|e| UiError::TransactionError(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut completed_transactions: Vec<CompletedTransaction> = Vec::new();
        completed_transactions.extend(
            self.wallet
                .transaction_service
                .get_completed_transactions()
                .await?
                .values()
                .cloned()
                .collect::<Vec<CompletedTransaction>>(),
        );

        completed_transactions.extend(
            self.wallet
                .transaction_service
                .get_cancelled_completed_transactions()
                .await?
                .values()
                .cloned()
                .collect::<Vec<CompletedTransaction>>(),
        );

        completed_transactions.sort_by(|a, b| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .expect("Should be able to compare timestamps")
        });

        self.data.completed_txs = completed_transactions
            .iter()
            .map(|tx| {
                CompletedTransactionInfo::from_completed_transaction(tx.clone(), &self.get_transaction_weight())
                    .map_err(|e| UiError::TransactionError(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_single_confirmation_state(&mut self, tx_id: TxId, confirmations: u64) -> Result<(), UiError> {
        let stat = self.data.confirmations.entry(tx_id).or_insert(confirmations);
        *stat = confirmations;
        Ok(())
    }

    pub async fn cleanup_single_confirmation_state(&mut self, tx_id: TxId) -> Result<(), UiError> {
        self.data.confirmations.remove_entry(&tx_id);
        Ok(())
    }

    pub async fn refresh_single_transaction_state(&mut self, tx_id: TxId) -> Result<(), UiError> {
        let found = self.wallet.transaction_service.get_any_transaction(tx_id).await?;

        match found {
            None => {
                // If it's not in the backend then remove it from AppState
                let _completed_transaction: Option<CompletedTransaction> = self
                    .data
                    .pending_txs
                    .iter()
                    .position(|i| i.tx_id == tx_id)
                    .and_then(|index| {
                        let _completed_transaction_info = self.data.pending_txs.remove(index);
                        None
                    });
                let _completed_transaction: Option<CompletedTransaction> = self
                    .data
                    .completed_txs
                    .iter()
                    .position(|i| i.tx_id == tx_id)
                    .and_then(|index| {
                        let _completed_transaction_info = self.data.pending_txs.remove(index);
                        None
                    });
            },
            Some(tx) => {
                let tx =
                    CompletedTransactionInfo::from_completed_transaction(tx.into(), &self.get_transaction_weight())
                        .map_err(|e| UiError::TransactionError(e.to_string()))?;
                if let Some(index) = self.data.pending_txs.iter().position(|i| i.tx_id == tx_id) {
                    if tx.status == TransactionStatus::Pending && tx.cancelled.is_none() {
                        self.data.pending_txs[index] = tx;
                        self.updated = true;
                        return Ok(());
                    } else {
                        let _completed_transaction_info = self.data.pending_txs.remove(index);
                    }
                } else if tx.status == TransactionStatus::Pending && tx.cancelled.is_none() {
                    self.data.pending_txs.push(tx);
                    self.data.pending_txs.sort_by(|a, b| {
                        b.timestamp
                            .partial_cmp(&a.timestamp)
                            .expect("Should be able to compare timestamps")
                    });
                    self.updated = true;
                    return Ok(());
                } else {
                    // dont care
                }

                if let Some(index) = self.data.completed_txs.iter().position(|i| i.tx_id == tx_id) {
                    self.data.completed_txs[index] = tx;
                } else {
                    self.data.completed_txs.push(tx);
                }
                self.data.completed_txs.sort_by(|a, b| {
                    b.timestamp
                        .partial_cmp(&a.timestamp)
                        .expect("Should be able to compare timestamps")
                });
            },
        }
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_contacts_state(&mut self) -> Result<(), UiError> {
        let db_contacts = self.wallet.contacts_service.get_contacts().await?;
        let mut ui_contacts: Vec<UiContact> = vec![];
        for contact in db_contacts {
            // A contact's online status is a function of current time and can therefore not be stored in a database
            let online_status = self
                .wallet
                .contacts_service
                .get_contact_online_status(contact.clone())
                .await?;
            ui_contacts.push(UiContact::from(contact.clone()).with_online_status(format!("{}", online_status)));
        }

        ui_contacts.sort_by(|a, b| {
            a.alias
                .partial_cmp(&b.alias)
                .expect("Should be able to compare contact aliases")
        });

        self.data.contacts = ui_contacts;
        self.refresh_network_id().await?;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_burnt_proofs_state(&mut self) -> Result<(), UiError> {
        let db_burnt_proofs = self.wallet.db.fetch_burnt_proofs()?;
        let mut ui_proofs: Vec<UiBurntProof> = vec![];

        for proof in db_burnt_proofs {
            ui_proofs.push(UiBurntProof {
                id: proof.0,
                reciprocal_claim_public_key: proof.1,
                payload: proof.2,
                burned_at: proof.3,
            });
        }

        ui_proofs.sort_by(|a, b| a.burned_at.cmp(&b.burned_at));

        self.data.burnt_proofs = ui_proofs;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_network_id(&mut self) -> Result<(), UiError> {
        let wallet_id = self.wallet.get_wallet_id().await?;
        let qr_link = format!(
            "tari://{}/transactions/send?tariAddress={}",
            wallet_id.network(),
            wallet_id.address_interactive.to_base58()
        );
        let code = QrCode::new(qr_link).unwrap();
        let image = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Dark)
            .light_color(unicode::Dense1x2::Light)
            .build()
            .lines()
            .skip(1)
            .fold("".to_string(), |acc, l| format!("{}{}\n", acc, l));
        let identity = MyIdentity {
            tari_address_interactive: wallet_id.address_interactive.clone(),
            tari_address_one_sided: wallet_id.address_one_sided.clone(),
            // TODO: we could display the current public addresses (either by config or autonat)
            network_address: String::new(),
            qr_code: image,
            node_id: wallet_id.peer_id.to_string(),
            public_key: wallet_id.public_key.to_string(),
        };
        self.data.my_identity = identity;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_connected_peers_state(&mut self) -> Result<(), UiError> {
        self.refresh_network_id().await?;
        let connections = self.wallet.network.get_active_connections().await?;
        self.data.connected_peers = connections;
        self.updated = true;
        Ok(())
    }

    pub fn has_time_locked_balance(&self) -> bool {
        if let Some(time_locked_balance) = self.data.balance.time_locked_balance {
            if time_locked_balance > MicroMinotari::from(0) {
                return true;
            }
        }
        false
    }

    pub async fn refresh_balance(&mut self, balance: Balance) -> Result<(), UiError> {
        self.data.balance = balance;
        self.updated = true;

        Ok(())
    }

    pub async fn refresh_base_node_state(&mut self, state: BaseNodeState) -> Result<(), UiError> {
        self.data.base_node_state = state;
        self.updated = true;

        Ok(())
    }

    pub fn update_base_node_peer(&mut self, peer: Peer) {
        self.data.base_node_selected = peer;
        self.updated = true;
    }

    pub async fn trigger_wallet_scanned_height_update(&mut self, height: u64) -> Result<(), UiError> {
        self.data.wallet_scanned_height = height;
        self.updated = true;
        Ok(())
    }

    pub fn get_shutdown_signal(&self) -> ShutdownSignal {
        self.wallet.shutdown_signal.clone()
    }

    pub fn get_transaction_service_event_stream(&self) -> TransactionEventReceiver {
        self.wallet.transaction_service.get_event_stream()
    }

    pub fn get_contacts_liveness_event_stream(&self) -> broadcast::Receiver<Arc<ContactsLivenessEvent>> {
        self.wallet.contacts_service.get_contacts_liveness_event_stream()
    }

    pub fn get_output_manager_service_event_stream(&self) -> OutputManagerEventReceiver {
        self.wallet.output_manager_service.get_event_stream()
    }

    pub fn get_network_events(&self) -> broadcast::Receiver<NetworkEvent> {
        self.wallet.network.subscribe_events()
    }

    pub fn get_wallet_connectivity(&self) -> WalletConnectivityHandle {
        self.wallet.wallet_connectivity.clone()
    }

    pub fn get_wallet_utxo_scanner(&self) -> UtxoScannerHandle {
        self.wallet.utxo_scanner_service.clone()
    }

    pub fn get_base_node_event_stream(&self) -> BaseNodeEventReceiver {
        self.wallet.base_node_service.get_event_stream()
    }

    pub async fn set_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        let pk = peer
            .public_key()
            .clone()
            .try_into_sr25519()
            .map_err(|_| UiError::PublicKeyParseError)?
            .inner_key()
            .clone();
        info!(
            target: LOG_TARGET,
            "Setting new base node peer for wallet: {}::{}",
            pk,
            peer.addresses().first().ok_or(UiError::NoAddress)?.to_string(),
        );
        self.wallet
            .set_base_node_peer(
                pk,
                Some(peer.addresses().first().ok_or(UiError::NoAddress)?.clone()),
                None,
            )
            .await?;

        self.spawn_restart_transaction_protocols_task();
        self.spawn_transaction_revalidation_task();

        self.data.base_node_previous = self.data.base_node_selected.clone();
        self.data.base_node_selected = peer;
        self.updated = true;

        Ok(())
    }

    pub async fn set_custom_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        let pk = peer
            .public_key()
            .clone()
            .try_into_sr25519()
            .map_err(|_| UiError::PublicKeyParseError)?
            .inner_key()
            .clone();
        self.wallet
            .set_base_node_peer(
                pk,
                Some(peer.addresses().first().cloned().ok_or(UiError::NoAddress)?),
                None,
            )
            .await?;

        self.spawn_restart_transaction_protocols_task();
        self.spawn_transaction_revalidation_task();

        self.data.base_node_previous = self.data.base_node_selected.clone();
        self.data.base_node_selected = peer.clone();
        self.data.base_node_peer_custom = Some(peer.clone());
        self.data
            .base_node_list
            .insert(0, ("Custom Base Node".to_string(), peer.clone()));
        self.updated = true;

        // persist the custom node in wallet db
        let pk_str = public_key_to_string(peer.public_key());
        self.wallet
            .db
            .set_client_key_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string(), pk_str.clone())?;
        self.wallet.db.set_client_key_value(
            CUSTOM_BASE_NODE_ADDRESS_KEY.to_string(),
            peer.addresses().first().ok_or(UiError::NoAddress)?.to_string(),
        )?;
        info!(
            target: LOG_TARGET,
            "Setting custom base node peer for wallet: {}::{}",
            pk_str,
            peer.addresses().first().ok_or(UiError::NoAddress)?.to_string(),
        );

        Ok(())
    }

    pub async fn clear_custom_base_node_peer(&mut self) -> Result<(), UiError> {
        let previous = self.data.base_node_previous.clone();
        let pk = previous
            .public_key()
            .clone()
            .try_into_sr25519()
            .map_err(|_| UiError::PublicKeyParseError)?
            .inner_key()
            .clone();
        self.wallet
            .set_base_node_peer(
                pk,
                Some(previous.addresses().first().ok_or(UiError::NoAddress)?.clone()),
                None,
            )
            .await?;

        self.spawn_restart_transaction_protocols_task();
        self.spawn_transaction_revalidation_task();

        self.data.base_node_peer_custom = None;
        self.data.base_node_selected = previous;
        self.data.base_node_list.remove(0);
        self.updated = true;

        // clear from wallet db
        self.wallet
            .db
            .clear_client_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string())?;
        self.wallet
            .db
            .clear_client_value(CUSTOM_BASE_NODE_ADDRESS_KEY.to_string())?;
        Ok(())
    }

    pub fn spawn_transaction_revalidation_task(&mut self) {
        let mut txn_service = self.wallet.transaction_service.clone();
        let mut output_manager_service = self.wallet.output_manager_service.clone();

        task::spawn(async move {
            if let Err(e) = txn_service.validate_transactions().await {
                error!(target: LOG_TARGET, "Problem validating transactions: {}", e);
            }

            if let Err(e) = output_manager_service.validate_txos().await {
                error!(target: LOG_TARGET, "Problem validating UTXOs: {}", e);
            }
        });
    }

    pub fn spawn_restart_transaction_protocols_task(&mut self) {
        let mut txn_service = self.wallet.transaction_service.clone();

        task::spawn(async move {
            if let Err(e) = txn_service.restart_transaction_protocols().await {
                error!(target: LOG_TARGET, "Problem restarting transaction protocols: {}", e);
            }
        });
    }

    pub fn add_notification(&mut self, notification: String) {
        self.data.notifications.push((Local::now(), notification));
        self.data.new_notification_count += 1;

        const MAX_NOTIFICATIONS: usize = 100;
        if self.data.notifications.len() > MAX_NOTIFICATIONS {
            let _notification = self.data.notifications.remove(0);
        }

        self.updated = true;
    }

    pub fn mark_notifications_as_read(&mut self) {
        self.data.new_notification_count = 0;
        self.updated = true;
    }

    pub fn clear_notifications(&mut self) {
        self.data.notifications.clear();
        self.data.new_notification_count = 0;
        self.updated = true;
    }
}

#[derive(Clone)]
pub struct CompletedTransactionInfo {
    pub tx_id: TxId,
    pub source_address: TariAddress,
    pub destination_address: TariAddress,
    pub amount: MicroMinotari,
    pub fee: MicroMinotari,
    pub excess_signature: String,
    pub maturity: u64,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub mined_timestamp: Option<NaiveDateTime>,
    pub cancelled: Option<TxCancellationReason>,
    pub direction: TransactionDirection,
    pub mined_height: Option<u64>,
    pub weight: u64,
    pub inputs_count: usize,
    pub outputs_count: usize,
    pub payment_id: Option<PaymentId>,
    pub coinbase: bool,
    pub burn: bool,
}

impl CompletedTransactionInfo {
    pub fn from_completed_transaction(
        tx: CompletedTransaction,
        transaction_weighting: &TransactionWeight,
    ) -> Result<Self, TransactionError> {
        let excess_signature = format!(
            "{},{}",
            tx.transaction
                .first_kernel_excess_sig()
                .map(|s| s.get_signature().to_hex())
                .unwrap_or_default(),
            tx.transaction
                .first_kernel_excess_sig()
                .map(|s| s.get_public_nonce().to_hex())
                .unwrap_or_default()
        );
        let weight = tx.transaction.calculate_weight(transaction_weighting)?;
        let inputs_count = tx.transaction.body.inputs().len();
        let outputs_count = tx.transaction.body.outputs().len();
        let coinbase = tx.transaction.body.contains_coinbase();
        let burn = tx.transaction.body.contains_burn();

        Ok(Self {
            tx_id: tx.tx_id,
            source_address: tx.source_address.clone(),
            destination_address: tx.destination_address.clone(),
            amount: tx.amount,
            fee: tx.fee,
            excess_signature,
            maturity: tx
                .transaction
                .body
                .outputs()
                .first()
                .map(|o| o.features.maturity)
                .unwrap_or(0),
            status: tx.status,
            message: tx.message,
            timestamp: tx.timestamp,
            mined_timestamp: tx.mined_timestamp,
            cancelled: tx.cancelled,
            direction: tx.direction,
            mined_height: tx.mined_height,
            weight,
            inputs_count,
            outputs_count,
            payment_id: tx.payment_id,
            coinbase,
            burn,
        })
    }
}

#[derive(Clone)]
struct AppStateData {
    pending_txs: Vec<CompletedTransactionInfo>,
    completed_txs: Vec<CompletedTransactionInfo>,
    confirmations: HashMap<TxId, u64>,
    my_identity: MyIdentity,
    contacts: Vec<UiContact>,
    burnt_proofs: Vec<UiBurntProof>,
    connected_peers: Vec<Connection>,
    balance: Balance,
    base_node_state: BaseNodeState,
    base_node_selected: Peer,
    base_node_previous: Peer,
    base_node_list: Vec<(String, Peer)>,
    base_node_peer_custom: Option<Peer>,
    all_events: VecDeque<EventListItem>,
    notifications: Vec<(DateTime<Local>, String)>,
    new_notification_count: u32,
    wallet_scanned_height: u64,
}

#[derive(Clone)]
pub struct EventListItem {
    pub event_type: String,
    pub desc: String,
}

impl AppStateData {
    pub fn new(wallet_identity: &WalletIdentity, base_node_selected: Peer, base_node_config: PeerConfig) -> Self {
        let qr_link = format!(
            "tari://{}/transactions/send?tariAddress={}",
            wallet_identity.network(),
            wallet_identity.address_interactive.to_base58()
        );
        let code = QrCode::new(qr_link).unwrap();
        let image = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Dark)
            .light_color(unicode::Dense1x2::Light)
            .build()
            .lines()
            .skip(1)
            .fold("".to_string(), |acc, l| format!("{}{}\n", acc, l));

        let identity = MyIdentity {
            tari_address_interactive: wallet_identity.address_interactive.clone(),
            tari_address_one_sided: wallet_identity.address_one_sided.clone(),
            // TODO: Display configured addresses or public address from autonat
            network_address: String::new(),
            qr_code: image,
            node_id: wallet_identity.peer_id.to_string(),
            public_key: wallet_identity.public_key.to_string(),
        };
        let base_node_previous = base_node_selected.clone();

        // set up our base node list from config
        let mut base_node_list = base_node_config
            .base_node_peers
            .iter()
            .map(|peer| ("Service Peer".to_string(), peer.clone()))
            .collect::<Vec<(String, Peer)>>();

        // add peer seeds
        let peer_seeds = base_node_config
            .peer_seeds
            .iter()
            .map(|peer| ("Peer Seed".to_string(), peer.clone()))
            .collect::<Vec<(String, Peer)>>();

        base_node_list.extend(peer_seeds);

        // and prepend the custom base node if it exists
        if let Some(peer) = base_node_config.base_node_custom.clone() {
            base_node_list.insert(0, ("Custom Base Node".to_string(), peer));
        }

        AppStateData {
            pending_txs: Vec::new(),
            completed_txs: Vec::new(),
            confirmations: HashMap::new(),
            my_identity: identity,
            contacts: Vec::new(),
            burnt_proofs: vec![],
            connected_peers: Vec::new(),
            balance: Balance::zero(),
            base_node_state: BaseNodeState::default(),
            base_node_selected,
            base_node_previous,
            base_node_list,
            base_node_peer_custom: base_node_config.base_node_custom,
            all_events: VecDeque::new(),
            notifications: Vec::new(),
            new_notification_count: 0,
            wallet_scanned_height: 0,
        }
    }
}

#[derive(Clone)]
pub struct MyIdentity {
    pub tari_address_interactive: TariAddress,
    pub tari_address_one_sided: TariAddress,
    pub network_address: String,
    pub qr_code: String,
    pub node_id: String,
    pub public_key: String,
}

#[derive(Clone, Debug)]
pub enum UiTransactionSendStatus {
    Initiated,
    Queued,
    SentDirect,
    TransactionComplete,
    DiscoveryInProgress,
    SentViaSaf,
    Error(String),
}

#[derive(Clone, Debug)]
pub enum UiTransactionBurnStatus {
    Initiated,
    TransactionComplete((u32, String, String)),
    Error(String),
}

#[derive(Clone)]
struct AppStateConfig {
    pub cache_update_cooldown: Duration,
}

impl Default for AppStateConfig {
    fn default() -> Self {
        Self {
            cache_update_cooldown: Duration::from_millis(100),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum TransactionFilter {
    None,
    AbandonedCoinbases,
}
