use crate::ui::{state::MyIdentity, widgets::StatefulList, UiContact, UiError};
use qrcode::{render::unicode, QrCode};
use tari_common::Network;
use tari_comms::{types::CommsPublicKey, NodeIdentity};
use tari_crypto::tari_utilities::hex::Hex;
use tari_wallet::{
    contacts_service::storage::database::Contact,
    transaction_service::storage::models::CompletedTransaction,
    util::emoji::EmojiId,
    WalletSqlite,
};

pub struct AppState {
    pub pending_txs: StatefulList<CompletedTransaction>,
    pub completed_txs: StatefulList<CompletedTransaction>,
    pub detailed_transaction: Option<CompletedTransaction>,
    pub my_identity: MyIdentity,
    pub contacts: StatefulList<UiContact>,
    pub wallet: WalletSqlite,
}

impl AppState {
    pub fn new(node_identity: &NodeIdentity, network: Network, wallet: WalletSqlite) -> Self {
        let eid = EmojiId::from_pubkey(node_identity.public_key()).to_string();
        let qr_link = format!("tari://{}/pubkey/{}", network, &node_identity.public_key().to_hex());
        let code = QrCode::new(qr_link).unwrap();
        let image = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Dark)
            .light_color(unicode::Dense1x2::Light)
            .build()
            .trim()
            .to_string();

        let identity = MyIdentity {
            public_key: node_identity.public_key().to_string(),
            public_address: node_identity.public_address().to_string(),
            emoji_id: eid,
            qr_code: image,
        };
        AppState {
            pending_txs: StatefulList::new(),
            completed_txs: StatefulList::new(),
            detailed_transaction: None,
            my_identity: identity,
            contacts: StatefulList::new(),
            wallet,
        }
    }

    pub async fn refresh_transaction_state(&mut self) -> Result<(), UiError> {
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
        self.pending_txs.items = pending_transactions;
        let mut completed_transactions: Vec<CompletedTransaction> = self
            .wallet
            .transaction_service
            .get_completed_transactions()
            .await?
            .values()
            .cloned()
            .collect();
        completed_transactions.sort_by(|a, b| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .expect("Should be able to compare timestamps")
        });

        self.completed_txs.items = completed_transactions;
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

        self.contacts.items = contacts;
        Ok(())
    }

    pub async fn upsert_contact(&mut self, alias: String, public_key_or_emoji_id: String) -> Result<(), UiError> {
        let public_key = match CommsPublicKey::from_hex(public_key_or_emoji_id.as_str()) {
            Ok(pk) => pk,
            Err(_) => {
                EmojiId::str_to_pubkey(public_key_or_emoji_id.as_str()).map_err(|_| UiError::PublicKeyParseError)?
            },
        };

        let contact = Contact { alias, public_key };
        self.wallet.contacts_service.upsert_contact(contact).await?;

        self.refresh_contacts_state().await?;

        Ok(())
    }

    pub async fn delete_contact(&mut self, public_key: String) -> Result<(), UiError> {
        let public_key = match CommsPublicKey::from_hex(public_key.as_str()) {
            Ok(pk) => pk,
            Err(_) => EmojiId::str_to_pubkey(public_key.as_str()).map_err(|_| UiError::PublicKeyParseError)?,
        };

        self.wallet.contacts_service.remove_contact(public_key).await?;

        self.refresh_contacts_state().await?;

        Ok(())
    }

    pub async fn send_transaction(&mut self, public_key: String, _amount: u64) -> Result<(), UiError> {
        let _public_key = match CommsPublicKey::from_hex(public_key.as_str()) {
            Ok(pk) => pk,
            Err(_) => EmojiId::str_to_pubkey(public_key.as_str()).map_err(|_| UiError::PublicKeyParseError)?,
        };

        Ok(())
    }
}
