//  Copyright 2021. The Tari Project
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

use crate::{error::SqliteStorageError, models::node::NewNode, schema::*, SqliteTransaction};
use diesel::{prelude::*, Connection, SqliteConnection};
use diesel_migrations::embed_migrations;
use tari_dan_core::storage::{BackendAdapter, NewUnitOfWorkTracker, StorageError};

#[derive(Clone)]
pub struct SqliteBackendAdapter {
    database_url: String,
}

impl SqliteBackendAdapter {
    pub fn new(database_url: String) -> SqliteBackendAdapter {
        Self { database_url }
    }
}

impl BackendAdapter for SqliteBackendAdapter {
    type BackendTransaction = SqliteTransaction;
    type Error = SqliteStorageError;

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        connection.execute("PRAGMA foreign_keys = ON;");
        connection.execute("BEGIN EXCLUSIVE TRANSACTION;");

        Ok(SqliteTransaction::new(connection))
    }

    fn insert(&self, item: &NewUnitOfWorkTracker, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        match item {
            NewUnitOfWorkTracker::Node { hash, parent } => {
                let new_node = NewNode {
                    hash: Vec::from(hash.as_bytes()),
                    parent: Vec::from(parent.as_bytes()),
                };
                diesel::insert_into(nodes::table)
                    .values(&new_node)
                    .execute(transaction.connection())?;
            },
            NewUnitOfWorkTracker::Instruction { .. } => {
                todo!()
            },
        }
        Ok(())
    }

    fn commit(&self, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        transaction.connection().execute("COMMIT TRANSACTION;")?;
        Ok(())
    }
}
