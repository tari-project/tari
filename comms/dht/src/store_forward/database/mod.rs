// Copyright 2020, The Taiji Project
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
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::{dsl, result::DatabaseErrorKind, BoolExpressionMethods, ExpressionMethods, QueryDsl, RunQueryDsl};
pub use stored_message::{NewStoredMessage, StoredMessage};
use taiji_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_utilities::hex::Hex;

use crate::{
    envelope::DhtMessageType,
    schema::stored_messages,
    storage::{DbConnection, StorageError},
    store_forward::message::StoredMessagePriority,
};

pub struct StoreAndForwardDatabase {
    connection: DbConnection,
}

impl StoreAndForwardDatabase {
    pub fn new(connection: DbConnection) -> Self {
        Self { connection }
    }

    /// Inserts and returns Ok(true) if the item already existed and Ok(false) if it didn't
    pub fn insert_message_if_unique(&self, message: NewStoredMessage) -> Result<bool, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        match diesel::insert_into(stored_messages::table)
            .values(message)
            .execute(&mut conn)
        {
            Ok(_) => Ok(false),
            Err(diesel::result::Error::DatabaseError(kind, e_info)) => match kind {
                DatabaseErrorKind::UniqueViolation => Ok(true),
                _ => Err(diesel::result::Error::DatabaseError(kind, e_info).into()),
            },
            Err(e) => Err(e.into()),
        }
    }

    pub fn remove_message(&self, message_ids: Vec<i32>) -> Result<usize, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        diesel::delete(stored_messages::table)
            .filter(stored_messages::id.eq_any(message_ids))
            .execute(&mut conn)
            .map_err(Into::into)
    }

    pub fn find_messages_for_peer(
        &self,
        public_key: &CommsPublicKey,
        node_id: &NodeId,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        let pk_hex = public_key.to_hex();
        let node_id_hex = node_id.to_hex();
        let mut conn = self.connection.get_pooled_connection()?;
        let mut query = stored_messages::table
            .select(stored_messages::all_columns)
            .filter(
                stored_messages::destination_pubkey
                    .eq(pk_hex)
                    .or(stored_messages::destination_node_id.eq(node_id_hex)),
            )
            .filter(stored_messages::message_type.eq(DhtMessageType::None as i32))
            .into_boxed();

        if let Some(since) = since {
            query = query.filter(stored_messages::stored_at.gt(since.naive_utc()));
        }

        query
            .order_by(stored_messages::stored_at.desc())
            .limit(limit)
            .get_results(&mut conn)
            .map_err(Into::into)
    }

    pub fn find_anonymous_messages(
        &self,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        let mut query = stored_messages::table
            .select(stored_messages::all_columns)
            .filter(stored_messages::origin_pubkey.is_null())
            .filter(stored_messages::destination_pubkey.is_null())
            .filter(stored_messages::is_encrypted.eq(true))
            .filter(stored_messages::message_type.eq(DhtMessageType::None as i32))
            .into_boxed();

        if let Some(since) = since {
            query = query.filter(stored_messages::stored_at.gt(since.naive_utc()));
        }

        query
            .order_by(stored_messages::stored_at.desc())
            .limit(limit)
            .get_results(&mut conn)
            .map_err(Into::into)
    }

    pub fn find_join_messages(
        &self,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        let mut query = stored_messages::table
            .select(stored_messages::all_columns)
            .filter(stored_messages::message_type.eq(DhtMessageType::Join as i32))
            .into_boxed();

        if let Some(since) = since {
            query = query.filter(stored_messages::stored_at.gt(since.naive_utc()));
        }

        query
            .order_by(stored_messages::stored_at.desc())
            .limit(limit)
            .get_results(&mut conn)
            .map_err(Into::into)
    }

    pub fn find_messages_of_type_for_pubkey(
        &self,
        public_key: &CommsPublicKey,
        message_type: DhtMessageType,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        let pk_hex = public_key.to_hex();
        let mut conn = self.connection.get_pooled_connection()?;
        let mut query = stored_messages::table
            .select(stored_messages::all_columns)
            .filter(stored_messages::destination_pubkey.eq(pk_hex))
            .filter(stored_messages::message_type.eq(message_type as i32))
            .into_boxed();

        if let Some(since) = since {
            query = query.filter(stored_messages::stored_at.gt(since.naive_utc()));
        }

        query
            .order_by(stored_messages::stored_at.desc())
            .limit(limit)
            .get_results(&mut conn)
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn get_all_messages(&self) -> Result<Vec<StoredMessage>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        stored_messages::table
            .select(stored_messages::all_columns)
            .get_results(&mut conn)
            .map_err(Into::into)
    }

    pub(crate) fn delete_messages_with_priority_older_than(
        &self,
        priority: StoredMessagePriority,
        since: NaiveDateTime,
    ) -> Result<usize, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        diesel::delete(stored_messages::table)
            .filter(stored_messages::stored_at.lt(since))
            .filter(stored_messages::priority.eq(priority as i32))
            .execute(&mut conn)
            .map_err(Into::into)
    }

    pub(crate) fn delete_messages_older_than(&self, since: NaiveDateTime) -> Result<usize, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        diesel::delete(stored_messages::table)
            .filter(stored_messages::stored_at.lt(since))
            .execute(&mut conn)
            .map_err(Into::into)
    }

    pub(crate) fn truncate_messages(&self, max_size: usize) -> Result<usize, StorageError> {
        let mut num_removed = 0;
        let mut conn = self.connection.get_pooled_connection()?;
        let max_size = max_size as u64;
        #[allow(clippy::cast_sign_loss)]
        let msg_count = stored_messages::table
            .select(dsl::count(stored_messages::id))
            .first::<i64>(&mut conn)? as u64;
        if msg_count > max_size {
            let remove_count = msg_count - max_size;
            #[allow(clippy::cast_possible_wrap)]
            let message_ids: Vec<i32> = stored_messages::table
                .select(stored_messages::id)
                .order_by(stored_messages::stored_at.asc())
                .limit(remove_count as i64)
                .get_results(&mut conn)?;
            num_removed = diesel::delete(stored_messages::table)
                .filter(stored_messages::id.eq_any(message_ids))
                .execute(&mut conn)?;
        }
        Ok(num_removed)
    }
}

#[cfg(test)]
mod test {
    use taiji_test_utils::random;

    use super::*;

    #[tokio::test]
    async fn insert_messages() {
        let conn = DbConnection::connect_memory(random::string(8)).unwrap();
        conn.migrate().unwrap();
        let db = StoreAndForwardDatabase::new(conn);
        let mut msg1 = NewStoredMessage::default();
        msg1.body_hash.push('1');
        let mut msg2 = NewStoredMessage::default();
        msg2.body_hash.push('2');
        let mut msg3 = NewStoredMessage::default();
        msg3.body_hash.push('2'); // Duplicate message
        db.insert_message_if_unique(msg1.clone()).unwrap();
        db.insert_message_if_unique(msg2.clone()).unwrap();
        db.insert_message_if_unique(msg3.clone()).unwrap();
        let messages = db.get_all_messages().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].body_hash, msg1.body_hash);
        assert_eq!(messages[1].body_hash, msg2.body_hash);
    }

    #[tokio::test]
    async fn remove_messages() {
        let conn = DbConnection::connect_memory(random::string(8)).unwrap();
        conn.migrate().unwrap();
        let db = StoreAndForwardDatabase::new(conn);
        // Create 3 unique messages
        let mut msg1 = NewStoredMessage::default();
        msg1.body_hash.push('1');
        let mut msg2 = NewStoredMessage::default();
        msg2.body_hash.push('2');
        let mut msg3 = NewStoredMessage::default();
        msg3.body_hash.push('3');
        db.insert_message_if_unique(msg1.clone()).unwrap();
        db.insert_message_if_unique(msg2.clone()).unwrap();
        db.insert_message_if_unique(msg3.clone()).unwrap();
        let messages = db.get_all_messages().unwrap();
        assert_eq!(messages.len(), 3);
        let msg1_id = messages[0].id;
        let msg2_id = messages[1].id;
        let msg3_id = messages[2].id;

        db.remove_message(vec![msg1_id, msg3_id]).unwrap();
        let messages = db.get_all_messages().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, msg2_id);
    }

    #[tokio::test]
    async fn truncate_messages() {
        let conn = DbConnection::connect_memory(random::string(8)).unwrap();
        conn.migrate().unwrap();
        let db = StoreAndForwardDatabase::new(conn);
        let mut msg1 = NewStoredMessage::default();
        msg1.body_hash.push('1');
        let mut msg2 = NewStoredMessage::default();
        msg2.body_hash.push('2');
        let mut msg3 = NewStoredMessage::default();
        msg3.body_hash.push('3');
        let mut msg4 = NewStoredMessage::default();
        msg4.body_hash.push('4');
        db.insert_message_if_unique(msg1.clone()).unwrap();
        db.insert_message_if_unique(msg2.clone()).unwrap();
        db.insert_message_if_unique(msg3.clone()).unwrap();
        db.insert_message_if_unique(msg4.clone()).unwrap();
        let num_removed = db.truncate_messages(2).unwrap();
        assert_eq!(num_removed, 2);
        let messages = db.get_all_messages().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].body_hash, msg3.body_hash);
        assert_eq!(messages[1].body_hash, msg4.body_hash);
    }
}
