use crate::{
    dummy_data::get_dummy_contacts,
    ui::{state::MyIdentity, StatefulList, UiContact},
};
use qrcode::{render::unicode, QrCode};
use tari_common::Network;
use tari_comms::NodeIdentity;
use tari_crypto::tari_utilities::hex::Hex;
use tari_wallet::{transaction_service::storage::models::CompletedTransaction, util::emoji::EmojiId};

pub struct AppState {
    pub pending_txs: StatefulList<CompletedTransaction>,
    pub completed_txs: StatefulList<CompletedTransaction>,
    pub detailed_transaction: Option<CompletedTransaction>,
    pub my_identity: MyIdentity,
    pub contacts: StatefulList<UiContact>,
    pub to_field: String,
    pub amount_field: String,
}

impl AppState {
    pub fn new(node_identity: &NodeIdentity, network: Network) -> Self {
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
            contacts: StatefulList::with_items(
                get_dummy_contacts()
                    .iter()
                    .map(|c| UiContact::from(c.clone()))
                    .collect(),
            ),
            to_field: "".to_string(),
            amount_field: "".to_string(),
        }
    }
}
