// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use chrono::{DateTime, Local};
use tari_common_types::emoji::EmojiId;
use tari_wallet::contacts_service::storage::database::Contact;

#[derive(Debug, Clone)]
pub struct UiContact {
    pub alias: String,
    pub public_key: String,
    pub emoji_id: String,
    pub last_seen: String,
    pub online_status: String,
}

impl UiContact {
    pub fn with_online_status(mut self, online_status: String) -> Self {
        self.online_status = online_status;
        self
    }
}

impl From<Contact> for UiContact {
    fn from(c: Contact) -> Self {
        Self {
            alias: c.alias,
            public_key: c.public_key.to_string(),
            emoji_id: EmojiId::from_pubkey(&c.public_key).as_str().to_string(),
            last_seen: match c.last_seen {
                Some(val) => DateTime::<Local>::from_utc(val, Local::now().offset().to_owned())
                    .format("%m-%dT%H:%M")
                    .to_string(),
                None => "".to_string(),
            },
            online_status: "".to_string(),
        }
    }
}
