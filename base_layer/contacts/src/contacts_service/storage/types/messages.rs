// Copyright 2023. The Tari Project
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

use std::convert::TryFrom;

use chrono::NaiveDateTime;
use diesel::{prelude::*, SqliteConnection};
use tari_common_types::tari_address::TariAddress;

use crate::{
    contacts_service::{
        error::ContactsServiceStorageError,
        types::{Direction, Message},
    },
    schema::messages,
};

/// A Sql version of the Contact struct
#[derive(Clone, Debug, Insertable, PartialEq, Eq)]
#[diesel(table_name = messages)]
#[diesel(primary_key(message_id))]
pub struct MessagesSqlInsert {
    pub address: Vec<u8>,
    pub body: Vec<u8>,
    pub direction: i32,
    pub stored_at: NaiveDateTime,
    pub message_id: Vec<u8>,
}

#[derive(Clone, Debug, Queryable, PartialEq, Eq, QueryableByName)]
#[diesel(table_name = messages)]
#[diesel(primary_key(message_id))]
pub struct MessagesSql {
    pub address: Vec<u8>,
    pub message_id: Vec<u8>,
    pub body: Vec<u8>,
    pub stored_at: NaiveDateTime,
    pub direction: i32,
}

impl MessagesSqlInsert {
    /// Write this struct to the database
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), ContactsServiceStorageError> {
        diesel::insert_into(messages::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }
}

impl MessagesSql {
    /// Find a particular message by their address, if it exists
    pub fn find_by_address(
        address: &[u8],
        limit: i64,
        page: i64,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<MessagesSql>, ContactsServiceStorageError> {
        Ok(messages::table
            .filter(messages::address.eq(address))
            .order(messages::stored_at.desc())
            .offset(page)
            .limit(limit)
            .load::<MessagesSql>(conn)?)
    }
}

/// Conversion from an Message to the Sql datatype form
impl TryFrom<MessagesSql> for Message {
    type Error = ContactsServiceStorageError;

    #[allow(clippy::cast_sign_loss)]
    fn try_from(o: MessagesSql) -> Result<Self, Self::Error> {
        let address = TariAddress::from_bytes(&o.address).map_err(|_| ContactsServiceStorageError::ConversionError)?;
        Ok(Self {
            address,
            direction: Direction::from_byte(
                u8::try_from(o.direction).map_err(|_| ContactsServiceStorageError::ConversionError)?,
            )
            .unwrap_or_else(|| panic!("Direction from byte {}", o.direction)),
            stored_at: o.stored_at.timestamp() as u64,
            body: o.body,
            message_id: o.message_id,
        })
    }
}

/// Conversion from a Contact to the Sql datatype form
#[allow(clippy::cast_possible_wrap)]
impl From<Message> for MessagesSqlInsert {
    fn from(o: Message) -> Self {
        Self {
            address: o.address.to_bytes().to_vec(),
            direction: i32::from(o.direction.as_byte()),
            stored_at: NaiveDateTime::from_timestamp_opt(o.stored_at as i64, 0).unwrap(),
            body: o.body,
            message_id: o.message_id,
        }
    }
}
