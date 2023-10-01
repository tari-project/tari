// Copyright 2019. The Tari Project
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

use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
    sync::Arc,
};

use chrono::NaiveDateTime;
use log::*;
use tari_common_types::tari_address::TariAddress;
use tari_comms::peer_manager::NodeId;

use crate::contacts_service::{
    error::ContactsServiceStorageError,
    types::{Contact, Message},
};

const LOG_TARGET: &str = "contacts::contacts_service::database";

/// This trait defines the functionality that a database backend need to provide for the Contacts Service
pub trait ContactsBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ContactsServiceStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, ContactsServiceStorageError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbKey {
    Contact(TariAddress),
    ContactId(NodeId),
    Contacts,
    Message(Vec<u8>),
    Messages(TariAddress, i64, i64),
}

pub enum DbValue {
    Contact(Box<Contact>),
    Contacts(Vec<Contact>),
    TariAddress(Box<TariAddress>),
    Message(Box<Message>),
    Messages(Vec<Message>),
}

#[allow(clippy::large_enum_variant)]
pub enum DbKeyValuePair {
    Contact(TariAddress, Contact),
    MessageConfirmations(Vec<u8>, Option<NaiveDateTime>, Option<NaiveDateTime>),
    LastSeen(NodeId, NaiveDateTime, Option<i32>),
}

pub enum WriteOperation {
    Insert(Box<DbValue>),
    Upsert(Box<DbKeyValuePair>),
    UpdateLastSeen(Box<DbKeyValuePair>),
    Remove(DbKey),
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($db:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $db.fetch(&key) {
            Ok(None) => Err(ContactsServiceStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

pub struct ContactsDatabase<T>
where T: ContactsBackend
{
    db: Arc<T>,
}

impl<T> ContactsDatabase<T>
where T: ContactsBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub fn get_contact(&self, address: TariAddress) -> Result<Contact, ContactsServiceStorageError> {
        let db_clone = self.db.clone();
        fetch!(db_clone, address, Contact)
    }

    pub fn get_contacts(&self) -> Result<Vec<Contact>, ContactsServiceStorageError> {
        let db_clone = self.db.clone();
        match db_clone.fetch(&DbKey::Contacts) {
            Ok(None) => log_error(
                DbKey::Contacts,
                ContactsServiceStorageError::UnexpectedResult("Could not retrieve contacts".to_string()),
            ),
            Ok(Some(DbValue::Contacts(c))) => Ok(c),
            Ok(Some(other)) => unexpected_result(DbKey::Contacts, other),
            Err(e) => log_error(DbKey::Contacts, e),
        }
    }

    pub fn upsert_contact(&self, contact: Contact) -> Result<(), ContactsServiceStorageError> {
        self.db.write(WriteOperation::Upsert(Box::new(DbKeyValuePair::Contact(
            contact.address.clone(),
            contact,
        ))))?;
        Ok(())
    }

    // converting u32 to i32 is okay here as its just the latency which wont reach u32 max.
    #[allow(clippy::cast_possible_wrap)]
    pub fn update_contact_last_seen(
        &self,
        node_id: &NodeId,
        last_seen: NaiveDateTime,
        latency: Option<u32>,
    ) -> Result<TariAddress, ContactsServiceStorageError> {
        let result = self
            .db
            .write(WriteOperation::UpdateLastSeen(Box::new(DbKeyValuePair::LastSeen(
                node_id.clone(),
                last_seen,
                latency.map(|val| val as i32),
            ))))?
            .ok_or_else(|| ContactsServiceStorageError::ValueNotFound(DbKey::ContactId(node_id.clone())))?;
        match result {
            DbValue::TariAddress(k) => Ok(*k),
            _ => Err(ContactsServiceStorageError::UnexpectedResult(
                "Incorrect response from backend.".to_string(),
            )),
        }
    }

    pub fn remove_contact(&self, address: TariAddress) -> Result<Contact, ContactsServiceStorageError> {
        let result = self
            .db
            .write(WriteOperation::Remove(DbKey::Contact(address.clone())))?
            .ok_or_else(|| ContactsServiceStorageError::ValueNotFound(DbKey::Contact(address.clone())))?;
        match result {
            DbValue::Contact(c) => Ok(*c),
            _ => Err(ContactsServiceStorageError::UnexpectedResult(
                "Incorrect response from backend.".to_string(),
            )),
        }
    }

    pub fn get_messages(
        &self,
        address: TariAddress,
        limit: i64,
        page: i64,
    ) -> Result<Vec<Message>, ContactsServiceStorageError> {
        let key = DbKey::Messages(address, limit, page);
        let db_clone = self.db.clone();
        match db_clone.fetch(&key) {
            Ok(None) => log_error(
                key,
                ContactsServiceStorageError::UnexpectedResult("Could not retrieve messages".to_string()),
            ),
            Ok(Some(DbValue::Messages(messages))) => Ok(messages),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }

    pub fn save_message(&self, message: Message) -> Result<(), ContactsServiceStorageError> {
        self.db
            .write(WriteOperation::Insert(Box::new(DbValue::Message(Box::new(message)))))?;

        Ok(())
    }

    pub fn confirm_message(
        &self,
        message_id: Vec<u8>,
        delivery_confirmation: Option<u64>,
        read_confirmation: Option<u64>,
    ) -> Result<(), ContactsServiceStorageError> {
        self.db
            .write(WriteOperation::Upsert(Box::new(DbKeyValuePair::MessageConfirmations(
                message_id,
                delivery_confirmation
                    .map(|d| NaiveDateTime::from_timestamp_opt(i64::try_from(d).unwrap_or(0), 0).unwrap()),
                read_confirmation.map(|d| NaiveDateTime::from_timestamp_opt(i64::try_from(d).unwrap_or(0), 0).unwrap()),
            ))))?;

        Ok(())
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ContactsServiceStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(ContactsServiceStorageError::UnexpectedResult(msg))
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::Contact(c) => f.write_str(&format!("Contact: {:?}", c)),
            DbKey::ContactId(id) => f.write_str(&format!("Contact: {:?}", id)),
            DbKey::Contacts => f.write_str("Contacts"),
            DbKey::Messages(c, _l, _p) => f.write_str(&format!("Messages for id: {:?}", c)),
            DbKey::Message(m) => f.write_str(&format!("Message for id: {:?}", m)),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::Contact(_) => f.write_str("Contact"),
            DbValue::Contacts(_) => f.write_str("Contacts"),
            DbValue::TariAddress(_) => f.write_str("Address"),
            DbValue::Messages(_) => f.write_str("Messages"),
            DbValue::Message(_) => f.write_str("Message"),
        }
    }
}

fn log_error<T>(req: DbKey, err: ContactsServiceStorageError) -> Result<T, ContactsServiceStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req,
        err.to_string()
    );
    Err(err)
}
