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

mod stored_message;
pub use stored_message::{NewStoredMessage, StoredMessage};

use crate::{
    envelope::DhtMessageType,
    schema::stored_messages,
    storage::{DbConnection, StorageError},
    store_forward::message::StoredMessagePriority,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, RunQueryDsl};
use tari_comms::types::CommsPublicKey;
use tari_crypto::tari_utilities::hex::Hex;

pub struct StoreAndForwardDatabase {
    connection: DbConnection,
}

impl StoreAndForwardDatabase {
    pub fn new(connection: DbConnection) -> Self {
        Self { connection }
    }

    pub async fn insert_message(&self, message: NewStoredMessage) -> Result<(), StorageError> {
        self.connection
            .with_connection_async(|conn| {
                diesel::insert_into(stored_messages::table)
                    .values(message)
                    .execute(conn)?;
                Ok(())
            })
            .await
    }

    pub async fn find_messages_for_public_key(
        &self,
        public_key: &CommsPublicKey,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredMessage>, StorageError>
    {
        let pk_hex = public_key.to_hex();
        self.connection
            .with_connection_async(move |conn| {
                let mut query = stored_messages::table
                    .select(stored_messages::all_columns)
                    .filter(stored_messages::destination_pubkey.eq(pk_hex))
                    .filter(stored_messages::message_type.eq(DhtMessageType::None as i32))
                    .into_boxed();

                if let Some(since) = since {
                    query = query.filter(stored_messages::stored_at.ge(since.naive_utc()));
                }

                query
                    .order_by(stored_messages::stored_at.desc())
                    .limit(limit)
                    .get_results(conn)
                    .map_err(Into::into)
            })
            .await
    }

    pub async fn find_messages_of_type_for_pubkey(
        &self,
        public_key: &CommsPublicKey,
        message_type: DhtMessageType,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredMessage>, StorageError>
    {
        let pk_hex = public_key.to_hex();
        self.connection
            .with_connection_async(move |conn| {
                let mut query = stored_messages::table
                    .select(stored_messages::all_columns)
                    .filter(
                        stored_messages::destination_pubkey
                            .eq(pk_hex)
                            .or(stored_messages::destination_pubkey.is_null()),
                    )
                    .filter(stored_messages::message_type.eq(message_type as i32))
                    .into_boxed();

                if let Some(since) = since {
                    query = query.filter(stored_messages::stored_at.ge(since.naive_utc()));
                }

                query
                    .order_by(stored_messages::stored_at.desc())
                    .limit(limit)
                    .get_results(conn)
                    .map_err(Into::into)
            })
            .await
    }

    #[cfg(test)]
    pub(crate) async fn get_all_messages(&self) -> Result<Vec<StoredMessage>, StorageError> {
        self.connection
            .with_connection_async(|conn| {
                stored_messages::table
                    .select(stored_messages::all_columns)
                    .get_results(conn)
                    .map_err(Into::into)
            })
            .await
    }

    pub(crate) async fn delete_messages_with_priority_older_than(
        &self,
        priority: StoredMessagePriority,
        since: NaiveDateTime,
    ) -> Result<usize, StorageError>
    {
        self.connection
            .with_connection_async(move |conn| {
                diesel::delete(stored_messages::table)
                    .filter(stored_messages::stored_at.lt(since))
                    .filter(stored_messages::priority.eq(priority as i32))
                    .execute(conn)
                    .map_err(Into::into)
            })
            .await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_test_utils::random;

    #[tokio_macros::test_basic]
    async fn insert_messages() {
        let conn = DbConnection::connect_memory(random::string(8)).await.unwrap();
        // let conn = DbConnection::connect_path("/tmp/tmp.db").await.unwrap();
        conn.migrate().await.unwrap();
        let db = StoreAndForwardDatabase::new(conn);
        db.insert_message(Default::default()).await.unwrap();
        let messages = db.get_all_messages().await.unwrap();
        assert_eq!(messages.len(), 1);
    }
}
