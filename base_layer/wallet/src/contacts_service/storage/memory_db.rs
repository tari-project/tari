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

use crate::contacts_service::{
    error::ContactsServiceStorageError,
    storage::database::{Contact, ContactsBackend, DbKey, DbKeyValuePair, DbValue, WriteOperation},
};
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct InnerDatabase {
    contacts: Vec<Contact>,
}

impl InnerDatabase {
    pub fn new() -> Self {
        Self { contacts: Vec::new() }
    }
}

#[derive(Default, Clone)]
pub struct ContactsServiceMemoryDatabase {
    db: Arc<RwLock<InnerDatabase>>,
}

impl ContactsServiceMemoryDatabase {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(InnerDatabase::new())),
        }
    }
}

impl ContactsBackend for ContactsServiceMemoryDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ContactsServiceStorageError> {
        let db = acquire_read_lock!(self.db);
        let result = match key {
            DbKey::Contact(pk) => db
                .contacts
                .iter()
                .find(|v| &v.public_key == pk)
                .map(|c| DbValue::Contact(Box::new(c.clone()))),
            DbKey::Contacts => Some(DbValue::Contacts(db.contacts.clone())),
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, ContactsServiceStorageError> {
        let mut db = acquire_write_lock!(self.db);
        match op {
            WriteOperation::Upsert(kvp) => match kvp {
                DbKeyValuePair::Contact(pk, c) => match db.contacts.iter_mut().find(|i| i.public_key == pk) {
                    None => db.contacts.push(c),
                    Some(existing_contact) => existing_contact.alias = c.alias,
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::Contact(pk) => match db.contacts.iter().position(|c| c.public_key == pk) {
                    None => return Err(ContactsServiceStorageError::ValueNotFound(DbKey::Contact(pk))),
                    Some(pos) => return Ok(Some(DbValue::Contact(Box::new(db.contacts.remove(pos))))),
                },
                DbKey::Contacts => {
                    return Err(ContactsServiceStorageError::OperationNotSupported);
                },
            },
        }

        Ok(None)
    }
}
