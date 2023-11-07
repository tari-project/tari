//  Copyright 2022 The Tari Project
//  SPDX-License-Identifier: BSD-3-Clause
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use chrono::{DateTime, Local};
use tari_contacts::contacts_service::types::Contact;

#[derive(Debug, Clone)]
pub struct UiContact {
    pub alias: String,
    pub address: String,
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
            address: c.address.to_hex(),
            emoji_id: c.address.to_emoji_string(),
            last_seen: match c.last_seen {
                Some(val) => DateTime::<Local>::from_naive_utc_and_offset(val, Local::now().offset().to_owned())
                    .format("%m-%dT%H:%M")
                    .to_string(),
                None => "".to_string(),
            },
            online_status: "".to_string(),
        }
    }
}
