// Copyright 2020, The Tari Project
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

use super::{dht_setting_entry::DhtSettingsEntry, DbConnection, StorageError};
use crate::{
    schema::dht_settings,
    storage::{dht_setting_entry::NewDhtSettingEntry, DhtSettingKey},
};
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use tari_crypto::tari_utilities::message_format::MessageFormat;

#[derive(Clone)]
pub struct DhtDatabase {
    connection: DbConnection,
}

impl DhtDatabase {
    pub fn new(connection: DbConnection) -> Self {
        Self { connection }
    }

    pub async fn get_value<T: MessageFormat>(&self, key: DhtSettingKey) -> Result<Option<T>, StorageError> {
        match self.get_value_bytes(key).await? {
            Some(bytes) => T::from_binary(&bytes).map(Some).map_err(Into::into),
            None => Ok(None),
        }
    }

    pub async fn get_value_bytes(&self, key: DhtSettingKey) -> Result<Option<Vec<u8>>, StorageError> {
        self.connection
            .with_connection_async(move |conn| {
                dht_settings::table
                    .filter(dht_settings::key.eq(key.to_string()))
                    .first(conn)
                    .map(|rec: DhtSettingsEntry| Some(rec.value))
                    .or_else(|err| match err {
                        diesel::result::Error::NotFound => Ok(None),
                        err => Err(err.into()),
                    })
            })
            .await
    }

    pub async fn set_value(&self, key: DhtSettingKey, value: Vec<u8>) -> Result<(), StorageError> {
        self.connection
            .with_connection_async(move |conn| {
                diesel::replace_into(dht_settings::table)
                    .values(NewDhtSettingEntry {
                        key: key.to_string(),
                        value,
                    })
                    .execute(conn)
                    .map(|_| ())
                    .map_err(Into::into)
            })
            .await
    }
}
