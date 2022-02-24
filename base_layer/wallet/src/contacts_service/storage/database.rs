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
    fmt::{Display, Error, Formatter},
    sync::Arc,
};

use chrono::NaiveDateTime;
use log::*;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};

use crate::contacts_service::error::ContactsServiceStorageError;

const LOG_TARGET: &str = "wallet::contacts_service::database";

#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub alias: String,
    pub public_key: CommsPublicKey,
    pub node_id: NodeId,
    pub last_seen: Option<NaiveDateTime>,
    pub latency: Option<u32>,
}

impl Contact {
    pub fn new(
        alias: String,
        public_key: CommsPublicKey,
        last_seen: Option<NaiveDateTime>,
        latency: Option<u32>,
    ) -> Self {
        Self {
            alias,
            public_key: public_key.clone(),
            node_id: NodeId::from_key(&public_key),
            last_seen,
            latency,
        }
    }
}

/// This trait defines the functionality that a database backend need to provide for the Contacts Service
pub trait ContactsBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ContactsServiceStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, ContactsServiceStorageError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    Contact(CommsPublicKey),
    ContactId(NodeId),
    Contacts,
}

pub enum DbValue {
    Contact(Box<Contact>),
    Contacts(Vec<Contact>),
    PublicKey(Box<CommsPublicKey>),
}

#[allow(clippy::large_enum_variant)]
pub enum DbKeyValuePair {
    Contact(CommsPublicKey, Contact),
    LastSeen(NodeId, NaiveDateTime, Option<i32>),
}

pub enum WriteOperation {
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

    pub async fn get_contact(&self, pub_key: CommsPublicKey) -> Result<Contact, ContactsServiceStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || fetch!(db_clone, pub_key.clone(), Contact))
            .await
            .map_err(|err| ContactsServiceStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn get_contacts(&self) -> Result<Vec<Contact>, ContactsServiceStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::Contacts) {
            Ok(None) => log_error(
                DbKey::Contacts,
                ContactsServiceStorageError::UnexpectedResult("Could not retrieve contacts".to_string()),
            ),
            Ok(Some(DbValue::Contacts(c))) => Ok(c),
            Ok(Some(other)) => unexpected_result(DbKey::Contacts, other),
            Err(e) => log_error(DbKey::Contacts, e),
        })
        .await
        .map_err(|err| ContactsServiceStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn upsert_contact(&self, contact: Contact) -> Result<(), ContactsServiceStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Upsert(Box::new(DbKeyValuePair::Contact(
                contact.public_key.clone(),
                contact,
            ))))
        })
        .await
        .map_err(|err| ContactsServiceStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn update_contact_last_seen(
        &self,
        node_id: &NodeId,
        last_seen: NaiveDateTime,
        latency: Option<u32>,
    ) -> Result<CommsPublicKey, ContactsServiceStorageError> {
        let db_clone = self.db.clone();
        let node_id_clone = node_id.clone();

        let result = tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::UpdateLastSeen(Box::new(DbKeyValuePair::LastSeen(
                node_id_clone,
                last_seen,
                latency.map(|val| val as i32),
            ))))
        })
        .await
        .map_err(|err| ContactsServiceStorageError::BlockingTaskSpawnError(err.to_string()))
        .and_then(|inner_result| inner_result)?
        .ok_or_else(|| ContactsServiceStorageError::ValueNotFound(DbKey::ContactId(node_id.clone())))?;
        match result {
            DbValue::PublicKey(k) => Ok(*k),
            _ => Err(ContactsServiceStorageError::UnexpectedResult(
                "Incorrect response from backend.".to_string(),
            )),
        }
    }

    pub async fn remove_contact(&self, pub_key: CommsPublicKey) -> Result<Contact, ContactsServiceStorageError> {
        let db_clone = self.db.clone();
        let pub_key_clone = pub_key.clone();
        let result =
            tokio::task::spawn_blocking(move || db_clone.write(WriteOperation::Remove(DbKey::Contact(pub_key_clone))))
                .await
                .map_err(|err| ContactsServiceStorageError::BlockingTaskSpawnError(err.to_string()))
                .and_then(|inner_result| inner_result)?
                .ok_or_else(|| ContactsServiceStorageError::ValueNotFound(DbKey::Contact(pub_key.clone())))?;

        match result {
            DbValue::Contact(c) => Ok(*c),
            DbValue::Contacts(_) | DbValue::PublicKey(_) => Err(ContactsServiceStorageError::UnexpectedResult(
                "Incorrect response from backend.".to_string(),
            )),
        }
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
            DbKey::Contacts => f.write_str(&"Contacts".to_string()),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::Contact(_) => f.write_str(&"Contact".to_string()),
            DbValue::Contacts(_) => f.write_str(&"Contacts".to_string()),
            DbValue::PublicKey(_) => f.write_str(&"PublicKey".to_string()),
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
