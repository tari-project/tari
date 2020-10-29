use tari_wallet::{contacts_service::storage::database::Contact, util::emoji::EmojiId};

#[derive(Debug, Clone)]
pub struct UiContact {
    pub alias: String,
    pub public_key: String,
    pub emoji_id: String,
}

impl From<Contact> for UiContact {
    fn from(c: Contact) -> Self {
        Self {
            alias: c.alias,
            public_key: c.public_key.to_string(),
            emoji_id: EmojiId::from_pubkey(&c.public_key).as_str().to_string(),
        }
    }
}
