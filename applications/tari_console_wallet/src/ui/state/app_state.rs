use crate::{
    ui::{
        state::wallet_event_monitor::WalletEventMonitor,
        UiContact,
        UiError,
        CUSTOM_BASE_NODE_ADDRESS_KEY,
        CUSTOM_BASE_NODE_PUBLIC_KEY_KEY,
    },
    wallet_modes::PeerConfig,
};
use futures::{stream::Fuse, StreamExt};
use log::*;
use qrcode::{render::unicode, QrCode};
use std::sync::Arc;
use tari_common::Network;
use tari_comms::{
    connectivity::ConnectivityEventRx,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_core::transactions::{
    tari_amount::{uT, MicroTari},
    types::PublicKey,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    base_node_service::{handle::BaseNodeEventReceiver, service::BaseNodeState},
    contacts_service::storage::database::Contact,
    output_manager_service::{handle::OutputManagerEventReceiver, service::Balance, TxId},
    transaction_service::{
        handle::{TransactionEvent, TransactionEventReceiver, TransactionServiceHandle},
        storage::models::{CompletedTransaction, TransactionStatus},
    },
    util::emoji::EmojiId,
    WalletSqlite,
};
use tokio::sync::{watch, RwLock};

const LOG_TARGET: &str = "wallet::console_wallet::app_state";

#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<AppStateInner>>,
    cached_data: AppStateData,
}

impl AppState {
    pub fn new(
        node_identity: &NodeIdentity,
        network: Network,
        wallet: WalletSqlite,
        base_node_selected: Peer,
        base_node_config: PeerConfig,
    ) -> Self
    {
        let inner = AppStateInner::new(node_identity, network, wallet, base_node_selected, base_node_config);
        let cached_data = inner.data.clone();
        Self {
            inner: Arc::new(RwLock::new(inner)),
            cached_data,
        }
    }

    pub async fn start_event_monitor(&self) {
        let event_monitor = WalletEventMonitor::new(self.inner.clone());
        tokio::spawn(event_monitor.run());
    }

    pub async fn refresh_transaction_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_full_transaction_state().await?;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
        }
        Ok(())
    }

    pub async fn refresh_contacts_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_contacts_state().await?;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
        }
        Ok(())
    }

    pub async fn refresh_connected_peers_state(&mut self) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        inner.refresh_connected_peers_state().await?;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
        }
        Ok(())
    }

    pub async fn update_cache(&mut self) {
        let mut inner = self.inner.write().await;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
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

        let contact = Contact { alias, public_key };
        inner.wallet.contacts_service.upsert_contact(contact).await?;

        inner.refresh_contacts_state().await?;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
        }
        Ok(())
    }

    pub async fn delete_contact(&mut self, public_key: String) -> Result<(), UiError> {
        let mut inner = self.inner.write().await;
        let public_key = match CommsPublicKey::from_hex(public_key.as_str()) {
            Ok(pk) => pk,
            Err(_) => EmojiId::str_to_pubkey(public_key.as_str()).map_err(|_| UiError::PublicKeyParseError)?,
        };

        inner.wallet.contacts_service.remove_contact(public_key).await?;

        inner.refresh_contacts_state().await?;
        if let Some(data) = inner.get_updated_app_state() {
            self.cached_data = data;
        }
        Ok(())
    }

    pub async fn send_transaction(
        &mut self,
        public_key: String,
        amount: u64,
        fee_per_gram: u64,
        message: String,
        result_tx: watch::Sender<UiTransactionSendStatus>,
    ) -> Result<(), UiError>
    {
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

    pub fn get_identity(&self) -> &MyIdentity {
        &self.cached_data.my_identity
    }

    pub fn get_contacts(&self) -> &Vec<UiContact> {
        &self.cached_data.contacts
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

    pub fn get_pending_txs(&self) -> &Vec<CompletedTransaction> {
        &self.cached_data.pending_txs
    }

    pub fn get_pending_txs_slice(&self, start: usize, end: usize) -> &[CompletedTransaction] {
        if self.cached_data.pending_txs.is_empty() || start > end || end > self.cached_data.pending_txs.len() {
            return &[];
        }

        &self.cached_data.pending_txs[start..end]
    }

    pub fn get_pending_tx(&self, index: usize) -> Option<&CompletedTransaction> {
        if index < self.cached_data.pending_txs.len() {
            Some(&self.cached_data.pending_txs[index])
        } else {
            None
        }
    }

    pub fn get_completed_txs_slice(&self, start: usize, end: usize) -> &[CompletedTransaction] {
        if self.cached_data.completed_txs.is_empty() || start > end || end > self.cached_data.completed_txs.len() {
            return &[];
        }

        &self.cached_data.completed_txs[start..end]
    }

    pub fn get_completed_txs(&self) -> &Vec<CompletedTransaction> {
        &self.cached_data.completed_txs
    }

    pub fn get_completed_tx(&self, index: usize) -> Option<&CompletedTransaction> {
        if index < self.cached_data.completed_txs.len() {
            Some(&self.cached_data.completed_txs[index])
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
        let node_id = NodeId::from_key(&pub_key)?;
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
    ) -> Self
    {
        let data = AppStateData::new(node_identity, network, base_node_selected, base_node_config);

        AppStateInner {
            updated: false,
            data,
            wallet,
        }
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
        self.data.pending_txs = pending_transactions;

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

        self.data.completed_txs = completed_transactions;
        self.refresh_balance().await?;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_single_transaction_state(&mut self, tx_id: TxId) -> Result<(), UiError> {
        let found = self.wallet.transaction_service.get_any_transaction(tx_id).await?;

        match found {
            None => {
                // In its not in the backend then make sure it is not left behind in the AppState
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
                let tx = CompletedTransaction::from(tx);
                if let Some(index) = self.data.pending_txs.iter().position(|i| i.tx_id == tx_id) {
                    if tx.status == TransactionStatus::Pending && !tx.cancelled {
                        self.data.pending_txs[index] = tx;
                        self.updated = true;
                        return Ok(());
                    } else {
                        let _ = self.data.pending_txs.remove(index);
                    }
                } else if tx.status == TransactionStatus::Pending && !tx.cancelled {
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
        self.refresh_balance().await?;
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
            if let Ok(p) = peer_manager.find_by_node_id(c.peer_node_id()).await {
                peers.push(p);
            }
        }

        self.data.connected_peers = peers;
        self.updated = true;
        Ok(())
    }

    pub async fn refresh_balance(&mut self) -> Result<(), UiError> {
        let balance = self.wallet.output_manager_service.get_balance().await?;
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

    pub fn get_transaction_service_event_stream(&self) -> Fuse<TransactionEventReceiver> {
        self.wallet.transaction_service.get_event_stream_fused()
    }

    pub fn get_output_manager_service_event_stream(&self) -> Fuse<OutputManagerEventReceiver> {
        self.wallet.output_manager_service.get_event_stream_fused()
    }

    pub fn get_connectivity_event_stream(&self) -> Fuse<ConnectivityEventRx> {
        self.wallet.comms.connectivity().get_event_subscription().fuse()
    }

    pub fn get_base_node_event_stream(&self) -> Fuse<BaseNodeEventReceiver> {
        self.wallet.base_node_service.clone().get_event_stream_fused()
    }

    pub async fn set_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        self.wallet
            .set_base_node_peer(
                peer.public_key.clone(),
                peer.clone()
                    .addresses
                    .first()
                    .ok_or_else(|| UiError::NoAddressError)?
                    .to_string(),
            )
            .await?;

        self.data.base_node_previous = self.data.base_node_selected.clone();
        self.data.base_node_selected = peer.clone();
        self.updated = true;

        info!(
            target: LOG_TARGET,
            "Setting new base node peer for wallet: {}::{}",
            peer.public_key,
            peer.addresses
                .first()
                .ok_or_else(|| UiError::NoAddressError)?
                .to_string(),
        );

        Ok(())
    }

    pub async fn set_custom_base_node_peer(&mut self, peer: Peer) -> Result<(), UiError> {
        self.wallet
            .set_base_node_peer(
                peer.public_key.clone(),
                peer.clone()
                    .addresses
                    .first()
                    .ok_or_else(|| UiError::NoAddressError)?
                    .to_string(),
            )
            .await?;

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
                peer.addresses
                    .first()
                    .ok_or_else(|| UiError::NoAddressError)?
                    .to_string(),
            )
            .await?;

        info!(
            target: LOG_TARGET,
            "Setting custom base node peer for wallet: {}::{}",
            peer.public_key,
            peer.addresses
                .first()
                .ok_or_else(|| UiError::NoAddressError)?
                .to_string(),
        );

        Ok(())
    }

    pub async fn clear_custom_base_node_peer(&mut self) -> Result<(), UiError> {
        let previous = self.data.base_node_previous.clone();
        self.wallet
            .set_base_node_peer(
                previous.public_key.clone(),
                previous
                    .addresses
                    .first()
                    .ok_or_else(|| UiError::NoAddressError)?
                    .to_string(),
            )
            .await?;

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
}

#[derive(Clone)]
struct AppStateData {
    pending_txs: Vec<CompletedTransaction>,
    completed_txs: Vec<CompletedTransaction>,
    my_identity: MyIdentity,
    contacts: Vec<UiContact>,
    connected_peers: Vec<Peer>,
    balance: Balance,
    base_node_state: BaseNodeState,
    base_node_selected: Peer,
    base_node_previous: Peer,
    base_node_list: Vec<(String, Peer)>,
    base_node_peer_custom: Option<Peer>,
}

impl AppStateData {
    pub fn new(
        node_identity: &NodeIdentity,
        network: Network,
        base_node_selected: Peer,
        base_node_config: PeerConfig,
    ) -> Self
    {
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
            my_identity: identity,
            contacts: Vec::new(),
            connected_peers: Vec::new(),
            balance: Balance::zero(),
            base_node_state: BaseNodeState::default(),
            base_node_selected,
            base_node_previous,
            base_node_list,
            base_node_peer_custom: base_node_config.base_node_custom,
        }
    }
}

#[derive(Clone)]
pub struct MyIdentity {
    pub public_key: String,
    pub public_address: String,
    pub emoji_id: String,
    pub qr_code: String,
}

pub async fn send_transaction_task(
    public_key: CommsPublicKey,
    amount: MicroTari,
    message: String,
    fee_per_gram: MicroTari,
    mut transaction_service_handle: TransactionServiceHandle,
    result_tx: watch::Sender<UiTransactionSendStatus>,
)
{
    let _ = result_tx.broadcast(UiTransactionSendStatus::Initiated);
    let mut event_stream = transaction_service_handle.get_event_stream_fused();
    let mut send_direct_received_result = (false, false);
    let mut send_saf_received_result = (false, false);
    match transaction_service_handle
        .send_transaction(public_key, amount, fee_per_gram, message)
        .await
    {
        Err(e) => {
            let _ = result_tx.broadcast(UiTransactionSendStatus::Error(UiError::from(e).to_string()));
        },
        Ok(our_tx_id) => {
            while let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(event) => match &*event {
                        TransactionEvent::TransactionDiscoveryInProgress(tx_id) => {
                            if our_tx_id == *tx_id {
                                let _ = result_tx.broadcast(UiTransactionSendStatus::DiscoveryInProgress);
                            }
                        },
                        TransactionEvent::TransactionDirectSendResult(tx_id, result) => {
                            if our_tx_id == *tx_id {
                                send_direct_received_result = (true, *result);
                                if send_saf_received_result.0 {
                                    break;
                                }
                            }
                        },
                        TransactionEvent::TransactionStoreForwardSendResult(tx_id, result) => {
                            if our_tx_id == *tx_id {
                                send_saf_received_result = (true, *result);
                                if send_direct_received_result.0 {
                                    break;
                                }
                            }
                        },
                        _ => (),
                    },
                    Err(e) => {
                        log::warn!(target: LOG_TARGET, "Error reading from event broadcast channel {:?}", e);
                        break;
                    },
                }
            }

            if send_direct_received_result.1 {
                let _ = result_tx.broadcast(UiTransactionSendStatus::SentDirect);
            } else if send_saf_received_result.1 {
                let _ = result_tx.broadcast(UiTransactionSendStatus::SentViaSaf);
            } else {
                let _ = result_tx.broadcast(UiTransactionSendStatus::Error(
                    "Transaction could not be sent".to_string(),
                ));
            }
        },
    }
}

#[derive(Clone)]
pub enum UiTransactionSendStatus {
    Initiated,
    SentDirect,
    DiscoveryInProgress,
    SentViaSaf,
    Error(String),
}
