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

use crate::{
    error::SqliteStorageError,
    models::{
        locked_qc::LockedQc,
        node::{NewNode, Node},
        prepare_qc::PrepareQc,
    },
    schema::{locked_qc::dsl, *},
    SqliteTransaction,
};
use diesel::{prelude::*, Connection, SqliteConnection};
use diesel_migrations::embed_migrations;
use std::convert::TryFrom;
use tari_dan_core::{
    models::{HotStuffMessageType, Payload, QuorumCertificate, Signature, TariDanPayload, TreeNodeHash, ViewId},
    storage::{BackendAdapter, NewUnitOfWorkTracker, StorageError, UnitOfWorkTracker},
};

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
    type Id = i32;
    type Payload = TariDanPayload;

    fn is_empty(&self) -> Result<bool, Self::Error> {
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        let n: Option<Node> = nodes::table.first(&connection).optional()?;
        Ok(n.is_none())
    }

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        connection.execute("PRAGMA foreign_keys = ON;");
        connection.execute("BEGIN EXCLUSIVE TRANSACTION;");

        Ok(SqliteTransaction::new(connection))
    }

    fn insert(&self, item: &NewUnitOfWorkTracker, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        match item {
            NewUnitOfWorkTracker::Node { hash, parent, height } => {
                let new_node = NewNode {
                    hash: Vec::from(hash.as_bytes()),
                    parent: Vec::from(parent.as_bytes()),
                    height: *height as i32,
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

    fn update(
        &self,
        id: &Self::Id,
        item: &UnitOfWorkTracker,
        transaction: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
        match item {
            UnitOfWorkTracker::SidechainMetadata => {
                todo!()
            },
            UnitOfWorkTracker::LockedQc {
                message_type,
                view_number,
                node_hash,
                signature,
            } => {
                use crate::schema::locked_qc::dsl;
                let message_type = message_type.as_u8() as i32;
                let existing: Result<LockedQc, _> = dsl::locked_qc.find(id).first(transaction.connection());
                match existing {
                    Ok(x) => {
                        diesel::update(dsl::locked_qc.find(id))
                            .set((
                                dsl::message_type.eq(message_type),
                                dsl::view_number.eq(view_number.0 as i64),
                                dsl::node_hash.eq(node_hash.as_bytes()),
                                dsl::signature.eq(signature.as_ref().map(|s| s.to_bytes())),
                            ))
                            .execute(transaction.connection())?;
                    },
                    Err(_) => {
                        diesel::insert_into(locked_qc::table)
                            .values((
                                dsl::id.eq(id),
                                dsl::message_type.eq(message_type),
                                dsl::view_number.eq(view_number.0 as i64),
                                dsl::node_hash.eq(node_hash.as_bytes()),
                                dsl::signature.eq(signature.as_ref().map(|s| s.to_bytes())),
                            ))
                            .execute(transaction.connection())?;
                    },
                }
            },
            UnitOfWorkTracker::PrepareQc {
                message_type,
                view_number,
                node_hash,
                signature,
            } => {
                use crate::schema::prepare_qc::dsl;
                let message_type = message_type.as_u8() as i32;
                let existing: Result<PrepareQc, _> = dsl::prepare_qc.find(id).first(transaction.connection());
                match existing {
                    Ok(x) => {
                        diesel::update(dsl::prepare_qc.find(id))
                            .set((
                                dsl::message_type.eq(message_type),
                                dsl::view_number.eq(view_number.0 as i64),
                                dsl::node_hash.eq(node_hash.as_bytes()),
                                dsl::signature.eq(signature.as_ref().map(|s| s.to_bytes())),
                            ))
                            .execute(transaction.connection())?;
                    },
                    Err(_) => {
                        diesel::insert_into(prepare_qc::table)
                            .values((
                                dsl::id.eq(id),
                                dsl::message_type.eq(message_type),
                                dsl::view_number.eq(view_number.0 as i64),
                                dsl::node_hash.eq(node_hash.as_bytes()),
                                dsl::signature.eq(signature.as_ref().map(|s| s.to_bytes())),
                            ))
                            .execute(transaction.connection())?;
                    },
                }
            },
            UnitOfWorkTracker::Node {
                hash,
                parent,
                height,
                is_committed,
            } => {
                use crate::schema::nodes::dsl;
                diesel::update(dsl::nodes.find(id))
                    .set((
                        dsl::hash.eq(&hash.0),
                        dsl::parent.eq(&parent.0),
                        dsl::height.eq(*height as i32),
                        dsl::is_committed.eq(is_committed),
                    ))
                    .execute(transaction.connection())?;
            },
        }
        Ok(())
    }

    fn commit(&self, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        transaction.connection().execute("COMMIT TRANSACTION;")?;
        Ok(())
    }

    fn locked_qc_id(&self) -> Self::Id {
        1
    }

    fn prepare_qc_id(&self) -> Self::Id {
        1
    }

    fn find_highest_prepared_qc(&self) -> Result<QuorumCertificate, Self::Error> {
        use crate::schema::*;
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        // TODO: this should be a single row
        let result: Option<PrepareQc> = prepare_qc::table
            .order_by(prepare_qc::view_number.desc())
            .first(&connection)
            .optional()?;
        let qc = match result {
            Some(r) => r,
            None => {
                let l: LockedQc = dsl::locked_qc.find(self.locked_qc_id()).first(&connection)?;
                PrepareQc {
                    id: 1,
                    message_type: l.message_type,
                    view_number: l.view_number,
                    node_hash: l.node_hash.clone(),
                    signature: l.signature.clone(),
                }
            },
        };

        Ok(QuorumCertificate::new(
            HotStuffMessageType::try_from(qc.message_type as u8).unwrap(),
            ViewId::from(qc.view_number as u64),
            TreeNodeHash(qc.node_hash.clone()),
            qc.signature.map(|s| Signature::from_bytes(s.as_slice())),
        ))
    }

    fn get_locked_qc(&self) -> Result<QuorumCertificate, Self::Error> {
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        let qc: LockedQc = dsl::locked_qc.find(self.locked_qc_id()).first(&connection)?;
        Ok(QuorumCertificate::new(
            HotStuffMessageType::try_from(qc.message_type as u8).unwrap(),
            ViewId::from(qc.view_number as u64),
            TreeNodeHash(qc.node_hash.clone()),
            qc.signature.map(|s| Signature::from_bytes(s.as_slice())),
        ))
    }

    fn find_node_by_hash(&self, node_hash: &TreeNodeHash) -> Result<(Self::Id, UnitOfWorkTracker), Self::Error> {
        use crate::schema::nodes::dsl;
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        let node: Node = dsl::nodes.filter(nodes::hash.eq(&node_hash.0)).first(&connection)?;
        Ok((node.id, UnitOfWorkTracker::Node {
            hash: TreeNodeHash(node.hash),
            parent: TreeNodeHash(node.parent),
            height: node.height as u32,
            is_committed: node.is_committed,
        }))
    }
}
