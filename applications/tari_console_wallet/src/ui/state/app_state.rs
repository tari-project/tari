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
    sync::Arc,
    time::{Duration, Instant},
};

use bitflags::bitflags;
use chrono::{DateTime, Local, NaiveDateTime};
use log::*;
use qrcode::{render::unicode, QrCode};
use tari_common::{configuration::Network, GlobalConfig};
use tari_common_types::{
    emoji::EmojiId,
    transaction::{TransactionDirection, TransactionStatus, TxId},
    types::PublicKey,
};
use tari_comms::{
    connectivity::ConnectivityEventRx,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_core::transactions::{
    tari_amount::{uT, MicroTari},
    weight::TransactionWeight,
};
use tari_crypto::{ristretto::RistrettoPublicKey, tari_utilities::hex::Hex};
use tari_p2p::auto_update::SoftwareUpdaterHandle;
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    assets::Asset,
    base_node_service::{handle::BaseNodeEventReceiver, service::BaseNodeState},
    connectivity_service::{OnlineStatus, WalletConnectivityHandle, WalletConnectivityInterface},
    contacts_service::storage::database::Contact,
    output_manager_service::{handle::OutputManagerEventReceiver, service::Balance},
    tokens::Token,
    transaction_service::{
        handle::TransactionEventReceiver,
        storage::models::{CompletedTransaction, TxCancellationReason},
    },
    WalletSqlite,
};
use tokio::{
    sync::{watch, RwLock},
    task,
};

use crate::{
    notifier::Notifier,
    ui::{
        state::{
            debouncer::BalanceEnquiryDebouncer,
            tasks::{send_one_sided_transaction_task, send_transaction_task},
            wallet_event_monitor::WalletEventMonitor,
        },
        UiContact,
        UiError,
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
    node_config: GlobalConfig,
    config: AppStateConfig,
    wallet_connectivity: WalletConnectivityHandle,
    balance_enquiry_debouncer: BalanceEnquiryDebouncer,
}

impl AppState {
    pub fn new(
        node_identity: &NodeIdentity,
        network: Network,
        wallet: WalletSqlite,
        base_node_selected: Peer,
        base_node_config: PeerConfig,
        node_config: GlobalConfig,
    ) -> Self {
        let wallet_connectivity = wallet.wallet_connectivity.clone();
        let output_manager_service = wallet.output_manager_service.clone();
        let inner = AppStateInner::new(node_identity, network, wallet, base_node_selected, base_node_config);
        let cached_data = inner.data.clone();

        let inner = Arc::new(RwLock::new(inner));
        Self {
            inner: inner.clone(),
            cached_data,
            cache_update_cooldown: None,
            completed_tx_filter: TransactionFilter::ABANDONED_COINBASES,
            node_config: node_config.clone(),
            config: AppStateConfig::default(),
            wallet_connectivity,
            balance_enquiry_debouncer: BalanceEnquiryDebouncer::new(
                inner,
                Duration::from_secs(node_config.wallet_balance_enquiry_cooldown_period),
                output_manager_service,
            ),
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
        let _ = self
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

    pub async fn refresh_connected_peers_state(&mut self) -> Result<(), UiError> {
        self.check_connectivity().await;
        let mut inner = self.inner.write().await;
        inner.refresh_connected_peers_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    pub async fn refresh_assets_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_assets_state().await?;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
        }
        Ok(())
    }

    pub async fn refresh_tokens_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_tokens_state().await?;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
        }
        Ok(())
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
            let mut index: usize = list.iter().position(|(_, p)| p == current).unwrap_or_default();
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

    pub async fn upsert_contact(&mut self, alias: String, public_key_or_emoji_id: String) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;

        let public_key = match CommsPublicKey::from_hex(public_key_or_emoji_id.as_str()) {
            Ok(pk) => pk,
            Err(_) => {
                EmojiId::str_to_pubkey(public_key_or_emoji_id.as_str()).map_err(|_| UiError::PublicKeyParseError)?
            },
        };

        let contact = Contact::new(alias, public_key, None, None);
        inner.wallet.contacts_service.upsert_contact(contact).await?;

        inner.refresh_contacts_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    // Return alias or pub key if the contact is not in the list.
    pub fn get_alias(&self, pub_key: &RistrettoPublicKey) -> String {
        let pub_key_hex = format!("{}", pub_key);

        match self
            .cached_data
            .contacts
            .iter()
            .find(|&contact| contact.public_key.eq(&pub_key_hex))
        {
            Some(contact) => contact.alias.clone(),
            None => pub_key_hex,
        }
    }

    pub async fn delete_contact(&mut self, public_key: String) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        let public_key = match CommsPublicKey::from_hex(public_key.as_str()) {
            Ok(pk) => pk,
            Err(_) => EmojiId::str_to_pubkey(public_key.as_str()).map_err(|_| UiError::PublicKeyParseError)?,
        };

        inner.wallet.contacts_service.remove_contact(public_key).await?;

        inner.refresh_contacts_state().await?;
        drop(inner);
        self.update_cache().await;
        Ok(())
    }

    pub async fn send_transaction(
        &mut self,
        public_key: String,
        amount: u64,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: u64,
        message: String,
        result_tx: watch::Sender<UiTransactionSendStatus>,
    ) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let public_key = match CommsPublicKey::from_hex(public_key.as_str()) {
            Ok(pk) => pk,
            Err(_) => EmojiId::str_to_pubkey(public_key.as_str()).map_err(|_| UiError::PublicKeyParseError)?,
        };

        let fee_per_gram = fee_per_gram * uT;
        let tx_service_handle = inner.wallet.transaction_service.clone();
        tokio::spawn(send_transaction_task(
            public_key,
            MicroTari::from(amount),
            unique_id,
            parent_public_key,
            message,
            fee_per_gram,
            tx_service_handle,
            result_tx,
        ));

        Ok(())
    }

    pub async fn send_one_sided_transaction(
        &mut self,
        public_key: String,
        amount: u64,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: u64,
        message: String,
        result_tx: watch::Sender<UiTransactionSendStatus>,
    ) -> Result<(), UiError> {
        let inner = self.inner.write().await;
        let public_key = match CommsPublicKey::from_hex(public_key.as_str()) {
            Ok(pk) => pk,
            Err(_) => EmojiId::str_to_pubkey(public_key.as_str()).map_err(|_| UiError::PublicKeyParseError)?,
        };

        let fee_per_gram = fee_per_gram * uT;
        let tx_service_handle = inner.wallet.transaction_service.clone();
        tokio::spawn(send_one_sided_transaction_task(
            public_key,
            MicroTari::from(amount),
            unique_id,
            parent_public_key,
            message,
            fee_per_gram,
            tx_service_handle,
            result_tx,
        ));

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

    pub fn get_identity(&self) -> &MyIdentity {
        &self.cached_data.my_identity
    }

    pub fn get_owned_assets(&self) -> &[Asset] {
        self.cached_data.owned_assets.as_slice()
    }

    pub fn get_owned_tokens(&self) -> &[Token] {
        self.cached_data.owned_tokens.as_slice()
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
        if self
            .completed_tx_filter
            .contains(TransactionFilter::ABANDONED_COINBASES)
        {
            self.cached_data
                .completed_txs
                .iter()
                .filter(|tx| !matches!(tx.cancelled, Some(TxCancellationReason::AbandonedCoinbase)))
                .collect()
        } else {
            self.cached_data.completed_txs.iter().collect()
        }
    }

    pub fn get_confirmations(&self, tx_id: &TxId) -> Option<&u64> {
        (&self.cached_data.confirmations).get(tx_id)
    }

    pub fn get_completed_tx(&self, index: usize) -> Option<&CompletedTransactionInfo> {
        let filtered_completed_txs = self.get_completed_txs();
        if index < filtered_completed_txs.len() {
            Some(filtered_completed_txs[index])
        } else {
            None
        }
    }

    pub fn get_connected_peers(&self) -> &Vec<Peer> {
        &self.cached_data.connected_peers
    }

    pub fn get_balance(&self) -> &Balance {
        &self.cached_data.balance
    }

    pub fn get_base_node_state(&self) -> &BaseNodeState {
        &self.cached_data.base_node_state
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
        let node_id = NodeId::from_key(&pub_key);
        let peer = Peer::new(
            pub_key,
            node_id,
            addr.into(),
            PeerFlags::default(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            Default::default(),
        );

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
        (&self.node_config.transaction_num_confirmations_required).to_owned()
    }

    pub fn toggle_abandoned_coinbase_filter(&mut self) {
        self.completed_tx_filter.toggle(TransactionFilter::ABANDONED_COINBASES);
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

    pub fn get_default_fee_per_gram(&self) -> MicroTari {
        // this should not be empty as we this should have been created, but lets just be safe and use the default value
        // from the config
        match self.node_config.wallet_config.as_ref() {
            Some(config) => config.fee_per_gram.into(),
            _ => MicroTari::from(5),
        }
    }

    pub fn get_network(&self) -> Network {
        self.node_config.network
    }
}
pub struct AppStateInner {
    updated: bool,
    data: AppStateData,
    wallet: WalletSqlite,
}

impl AppStateInner {
    pub fn new(
        node_identity: &NodeIdentity,
        network: Network,
        wallet: WalletSqlite,
        base_node_selected: Peer,
        base_node_config: PeerConfig,
    ) -> Self {
        let data = AppStateData::new(node_identity, network, base_node_selected, base_node_config);

        AppStateInner {
            updated: false,
            data,
            wallet,
        }
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
            .network
            .create_consensus_constants()
            .last()
            .unwrap()
            .transaction_weight()
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
            .map(|tx| CompletedTransactionInfo::from_completed_transaction(tx.clone(), &self.get_transaction_weight()))
            .collect();

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
            .map(|tx| CompletedTransactionInfo::from_completed_transaction(tx.clone(), &self.get_transaction_weight()))
            .collect();
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
                let _: Option<CompletedTransaction> = self
                    .data
                    .pending_txs
                    .iter()
                    .position(|i| i.tx_id == tx_id)
                    .and_then(|index| {
                        let _ = self.data.pending_txs.remove(index);
                        None
                    });
                let _: Option<CompletedTransaction> = self
                    .data
                    .completed_txs
                    .iter()
                    .position(|i| i.tx_id == tx_id)
                    .and_then(|index| {
                        let _ = self.data.pending_txs.remove(index);
                        None
                    });
            },
            Some(tx) => {
                let tx =
                    CompletedTransactionInfo::from_completed_transaction(tx.into(), &self.get_transaction_weight());
                if let Some(index) = self.data.pending_txs.iter().position(|i| i.tx_id == tx_id) {
                    if tx.status == TransactionStatus::Pending && tx.cancelled.is_none() {
                        self.data.pending_txs[index] = tx;
                        self.updated = true;
                        return Ok(());
                    } else {
                        let _ = self.data.pending_txs.remove(index);
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
        self.refresh_assets_state().await?;
        self.refresh_tokens_state().await?;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_contacts_state(&mut self) -> Result<(), UiError> {
        let mut contacts: Vec<UiContact> = self
            .wallet
            .contacts_service
            .get_contacts()
            .await?
            .iter()
            .map(|c| UiContact::from(c.clone()))
            .collect();

        contacts.sort_by(|a, b| {
            a.alias
                .partial_cmp(&b.alias)
                .expect("Should be able to compare contact aliases")
        });

        self.data.contacts = contacts;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_connected_peers_state(&mut self) -> Result<(), UiError> {
        let connections = self.wallet.comms.connectivity().get_active_connections().await?;
        let peer_manager = self.wallet.comms.peer_manager();
        let mut peers = Vec::with_capacity(connections.len());
        for c in connections.iter() {
            if let Ok(Some(p)) = peer_manager.find_by_node_id(c.peer_node_id()).await {
                peers.push(p);
            }
        }
        self.data.connected_peers = peers;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_assets_state(&mut self) -> Result<(), UiError> {
        let asset_utxos = self.wallet.asset_manager.list_owned_assets().await?;
        self.data.owned_assets = asset_utxos;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_tokens_state(&mut self) -> Result<(), UiError> {
        let token_utxos = self.wallet.token_manager.list_owned_tokens().await?;
        self.data.owned_tokens = token_utxos;
        self.updated = true;
        Ok(())
    }

    pub fn has_time_locked_balance(&self) -> bool {
        if let Some(time_locked_balance) = self.data.balance.time_locked_balance {
            if time_locked_balance > MicroTari::from(0) {
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

    pub async fn refresh_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        self.data.base_node_selected = peer;
        self.updated = true;

        Ok(())
    }

    pub fn get_shutdown_signal(&self) -> ShutdownSignal {
        self.wallet.comms.shutdown_signal()
    }

    pub fn get_transaction_service_event_stream(&self) -> TransactionEventReceiver {
        self.wallet.transaction_service.get_event_stream()
    }

    pub fn get_output_manager_service_event_stream(&self) -> OutputManagerEventReceiver {
        self.wallet.output_manager_service.get_event_stream()
    }

    pub fn get_connectivity_event_stream(&self) -> ConnectivityEventRx {
        self.wallet.comms.connectivity().get_event_subscription()
    }

    pub fn get_wallet_connectivity(&self) -> WalletConnectivityHandle {
        self.wallet.wallet_connectivity.clone()
    }

    pub fn get_base_node_event_stream(&self) -> BaseNodeEventReceiver {
        self.wallet.base_node_service.get_event_stream()
    }

    pub async fn set_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        self.wallet
            .set_base_node_peer(
                peer.public_key.clone(),
                peer.addresses.first().ok_or(UiError::NoAddress)?.address.clone(),
            )
            .await?;

        self.spawn_transaction_revalidation_task();

        self.data.base_node_previous = self.data.base_node_selected.clone();
        self.data.base_node_selected = peer.clone();
        self.updated = true;

        info!(
            target: LOG_TARGET,
            "Setting new base node peer for wallet: {}::{}",
            peer.public_key,
            peer.addresses.first().ok_or(UiError::NoAddress)?.to_string(),
        );

        Ok(())
    }

    pub async fn set_custom_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        self.wallet
            .set_base_node_peer(
                peer.public_key.clone(),
                peer.addresses.first().ok_or(UiError::NoAddress)?.address.clone(),
            )
            .await?;

        self.spawn_transaction_revalidation_task();

        self.data.base_node_previous = self.data.base_node_selected.clone();
        self.data.base_node_selected = peer.clone();
        self.data.base_node_peer_custom = Some(peer.clone());
        self.data
            .base_node_list
            .insert(0, ("Custom Base Node".to_string(), peer.clone()));
        self.updated = true;

        // persist the custom node in wallet db
        self.wallet
            .db
            .set_client_key_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string(), peer.public_key.to_string())
            .await?;
        self.wallet
            .db
            .set_client_key_value(
                CUSTOM_BASE_NODE_ADDRESS_KEY.to_string(),
                peer.addresses.first().ok_or(UiError::NoAddress)?.to_string(),
            )
            .await?;
        info!(
            target: LOG_TARGET,
            "Setting custom base node peer for wallet: {}::{}",
            peer.public_key,
            peer.addresses.first().ok_or(UiError::NoAddress)?.to_string(),
        );

        Ok(())
    }

    pub async fn clear_custom_base_node_peer(&mut self) -> Result<(), UiError> {
        let previous = self.data.base_node_previous.clone();
        self.wallet
            .set_base_node_peer(
                previous.public_key.clone(),
                previous.addresses.first().ok_or(UiError::NoAddress)?.address.clone(),
            )
            .await?;

        self.spawn_transaction_revalidation_task();

        self.data.base_node_peer_custom = None;
        self.data.base_node_selected = previous;
        self.data.base_node_list.remove(0);
        self.updated = true;

        // clear from wallet db
        self.wallet
            .db
            .clear_client_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string())
            .await?;
        self.wallet
            .db
            .clear_client_value(CUSTOM_BASE_NODE_ADDRESS_KEY.to_string())
            .await?;
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

    pub fn add_notification(&mut self, notification: String) {
        self.data.notifications.push((Local::now(), notification));
        self.data.new_notification_count += 1;
        self.updated = true;
    }

    pub fn mark_notifications_as_read(&mut self) {
        self.data.new_notification_count = 0;
        self.updated = true;
    }

    pub fn get_software_updater(&self) -> SoftwareUpdaterHandle {
        self.wallet.get_software_updater()
    }
}

#[derive(Clone)]
pub struct CompletedTransactionInfo {
    pub tx_id: TxId,
    pub source_public_key: CommsPublicKey,
    pub destination_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub fee: MicroTari,
    pub excess_signature: String,
    pub maturity: u64,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub cancelled: Option<TxCancellationReason>,
    pub direction: TransactionDirection,
    pub mined_height: Option<u64>,
    pub is_coinbase: bool,
    pub weight: u64,
    pub inputs_count: usize,
    pub outputs_count: usize,
    pub unique_id: String,
}

fn first_unique_id(tx: &CompletedTransaction) -> String {
    let body = tx.transaction.body();
    for input in body.inputs() {
        if let Ok(features) = input.features() {
            if let Some(ref unique_id) = features.unique_id {
                return unique_id.to_hex();
            }
        }
    }
    for output in body.outputs() {
        if let Some(ref unique_id) = output.features.unique_id {
            return unique_id.to_hex();
        }
    }

    String::new()
}

impl CompletedTransactionInfo {
    pub fn from_completed_transaction(tx: CompletedTransaction, transaction_weighting: &TransactionWeight) -> Self {
        let excess_signature = tx
            .transaction
            .first_kernel_excess_sig()
            .map(|s| s.get_signature().to_hex())
            .unwrap_or_default();
        let is_coinbase = tx.is_coinbase();
        let weight = tx.transaction.calculate_weight(transaction_weighting);
        let inputs_count = tx.transaction.body.inputs().len();
        let outputs_count = tx.transaction.body.outputs().len();
        let unique_id = first_unique_id(&tx);

        Self {
            tx_id: tx.tx_id,
            source_public_key: tx.source_public_key.clone(),
            destination_public_key: tx.destination_public_key.clone(),
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
            cancelled: tx.cancelled,
            direction: tx.direction,
            mined_height: tx.mined_height,
            is_coinbase,
            weight,
            inputs_count,
            outputs_count,
            unique_id,
        }
    }
}

#[derive(Clone)]
struct AppStateData {
    owned_assets: Vec<Asset>,
    owned_tokens: Vec<Token>,
    pending_txs: Vec<CompletedTransactionInfo>,
    completed_txs: Vec<CompletedTransactionInfo>,
    confirmations: HashMap<TxId, u64>,
    my_identity: MyIdentity,
    contacts: Vec<UiContact>,
    connected_peers: Vec<Peer>,
    balance: Balance,
    base_node_state: BaseNodeState,
    base_node_selected: Peer,
    base_node_previous: Peer,
    base_node_list: Vec<(String, Peer)>,
    base_node_peer_custom: Option<Peer>,
    all_events: VecDeque<EventListItem>,
    notifications: Vec<(DateTime<Local>, String)>,
    new_notification_count: u32,
}

#[derive(Clone)]
pub struct EventListItem {
    pub event_type: String,
    pub desc: String,
}

impl AppStateData {
    pub fn new(
        node_identity: &NodeIdentity,
        network: Network,
        base_node_selected: Peer,
        base_node_config: PeerConfig,
    ) -> Self {
        let eid = EmojiId::from_pubkey(node_identity.public_key()).to_string();
        let qr_link = format!("tari://{}/pubkey/{}", network, &node_identity.public_key().to_hex());
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
            public_key: node_identity.public_key().to_string(),
            public_address: node_identity.public_address().to_string(),
            emoji_id: eid,
            qr_code: image,
            node_id: node_identity.node_id().to_string(),
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
            owned_assets: Vec::new(),
            owned_tokens: Vec::new(),
            pending_txs: Vec::new(),
            completed_txs: Vec::new(),
            confirmations: HashMap::new(),
            my_identity: identity,
            contacts: Vec::new(),
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
        }
    }
}

#[derive(Clone)]
pub struct MyIdentity {
    pub public_key: String,
    pub public_address: String,
    pub emoji_id: String,
    pub qr_code: String,
    pub node_id: String,
}

#[derive(Clone)]
pub enum UiTransactionSendStatus {
    Initiated,
    SentDirect,
    TransactionComplete,
    DiscoveryInProgress,
    SentViaSaf,
    Error(String),
}

bitflags! {
    pub struct TransactionFilter: u8 {
        const NONE = 0b0000_0000;
        const ABANDONED_COINBASES = 0b0000_0001;
    }
}

#[derive(Clone)]
struct AppStateConfig {
    pub cache_update_cooldown: Duration,
}

impl Default for AppStateConfig {
    fn default() -> Self {
        Self {
            cache_update_cooldown: Duration::from_secs(2),
        }
    }
}
