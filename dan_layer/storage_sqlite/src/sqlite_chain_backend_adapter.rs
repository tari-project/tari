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

use std::convert::{TryFrom, TryInto};

use diesel::{prelude::*, Connection, SqliteConnection};
use log::*;
use tari_dan_core::{
    models::{HotStuffMessageType, QuorumCertificate, Signature, TariDanPayload, TreeNodeHash, ViewId},
    storage::chain::{ChainDbBackendAdapter, DbInstruction, DbNode, DbQc},
};

use crate::{
    error::SqliteStorageError,
    models::{
        instruction::{Instruction, NewInstruction},
        locked_qc::LockedQc,
        node::{NewNode, Node},
        prepare_qc::PrepareQc,
    },
    schema::*,
    SqliteTransaction,
};

const LOG_TARGET: &str = "tari::dan_layer::storage_sqlite::sqlite_chain_backend_adapter";

#[derive(Clone)]
pub struct SqliteChainBackendAdapter {
    database_url: String,
}

impl SqliteChainBackendAdapter {
    pub fn new(database_url: String) -> SqliteChainBackendAdapter {
        Self { database_url }
    }

    pub fn get_connection(&self) -> ConnectionResult<SqliteConnection> {
        SqliteConnection::establish(self.database_url.as_str())
    }
}

impl ChainDbBackendAdapter for SqliteChainBackendAdapter {
    type BackendTransaction = SqliteTransaction;
    type Error = SqliteStorageError;
    type Id = i32;
    type Payload = TariDanPayload;

    fn is_empty(&self) -> Result<bool, Self::Error> {
        let connection = self.get_connection()?;
        let n: Option<Node> =
            nodes::table
                .first(&connection)
                .optional()
                .map_err(|source| SqliteStorageError::DieselError {
                    source,
                    operation: "is_empty".to_string(),
                })?;
        Ok(n.is_none())
    }

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        let connection = self.get_connection()?;
        connection
            .execute("PRAGMA foreign_keys = ON;")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "set pragma".to_string(),
            })?;
        connection
            .execute("BEGIN EXCLUSIVE TRANSACTION;")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "begin transaction".to_string(),
            })?;

        Ok(SqliteTransaction::new(connection))
    }

    fn node_exists(&self, node_hash: &TreeNodeHash) -> Result<bool, Self::Error> {
        let connection = self.get_connection()?;
        use crate::schema::nodes::dsl;
        let count = dsl::nodes
            .filter(nodes::parent.eq(node_hash.as_bytes()))
            .limit(1)
            .count()
            .first::<i64>(&connection)
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "node_exists: count".to_string(),
            })?;

        Ok(count > 0)
    }

    fn get_tip_node(&self) -> Result<Option<DbNode>, Self::Error> {
        use crate::schema::nodes::dsl;

        let connection = self.get_connection()?;
        let node = dsl::nodes
            .order_by(dsl::height.desc())
            .first::<Node>(&connection)
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get_tip_node".to_string(),
            })?;

        match node {
            Some(node) => Ok(Some(DbNode {
                hash: node.hash.try_into()?,
                parent: node.parent.try_into()?,
                height: node.height as u32,
                is_committed: node.is_committed,
            })),
            None => Ok(None),
        }
    }

    fn insert_node(&self, item: &DbNode, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        debug!(target: LOG_TARGET, "Inserting {:?}", item);
        let new_node = NewNode {
            hash: Vec::from(item.hash.as_bytes()),
            parent: Vec::from(item.parent.as_bytes()),
            height: item.height as i32,
        };
        diesel::insert_into(nodes::table)
            .values(&new_node)
            .execute(transaction.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "insert::node".to_string(),
            })?;
        Ok(())
    }

    fn update_node(
        &self,
        id: &Self::Id,
        item: &DbNode,
        transaction: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
        use crate::schema::nodes::dsl;
        // Should not be allowed to update hash, parent and height
        diesel::update(dsl::nodes.find(id))
            .set((
                // dsl::hash.eq(&hash.0),
                // dsl::parent.eq(&parent.0),
                // dsl::height.eq(*height as i32),
                dsl::is_committed.eq(item.is_committed),
            ))
            .execute(transaction.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "update::nodes".to_string(),
            })?;
        Ok(())
    }

    fn update_locked_qc(&self, item: &DbQc, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        use crate::schema::locked_qc::dsl;
        let message_type = item.message_type.as_u8() as i32;
        let existing: Result<LockedQc, _> = dsl::locked_qc.find(1).first(transaction.connection());
        match existing {
            Ok(_) => {
                diesel::update(dsl::locked_qc.find(1))
                    .set((
                        dsl::message_type.eq(message_type),
                        dsl::view_number.eq(item.view_number.0 as i64),
                        dsl::node_hash.eq(item.node_hash.as_bytes()),
                        dsl::signature.eq(item.signature.as_ref().map(|s| s.to_bytes())),
                    ))
                    .execute(transaction.connection())
                    .map_err(|source| SqliteStorageError::DieselError {
                        source,
                        operation: "update::locked_qc".to_string(),
                    })?;
            },
            Err(_) => {
                diesel::insert_into(locked_qc::table)
                    .values((
                        dsl::id.eq(1),
                        dsl::message_type.eq(message_type),
                        dsl::view_number.eq(item.view_number.0 as i64),
                        dsl::node_hash.eq(item.node_hash.as_bytes()),
                        dsl::signature.eq(item.signature.as_ref().map(|s| s.to_bytes())),
                    ))
                    .execute(transaction.connection())
                    .map_err(|source| SqliteStorageError::DieselError {
                        source,
                        operation: "insert::locked_qc".to_string(),
                    })?;
            },
        }
        Ok(())
    }

    fn update_prepare_qc(&self, item: &DbQc, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        use crate::schema::prepare_qc::dsl;
        let message_type = item.message_type.as_u8() as i32;
        let existing: Result<PrepareQc, _> = dsl::prepare_qc.find(1).first(transaction.connection());
        match existing {
            Ok(_) => {
                diesel::update(dsl::prepare_qc.find(1))
                    .set((
                        dsl::message_type.eq(message_type),
                        dsl::view_number.eq(item.view_number.0 as i64),
                        dsl::node_hash.eq(item.node_hash.as_bytes()),
                        dsl::signature.eq(item.signature.as_ref().map(|s| s.to_bytes())),
                    ))
                    .execute(transaction.connection())
                    .map_err(|source| SqliteStorageError::DieselError {
                        source,
                        operation: "update::prepare_qc".to_string(),
                    })?;
            },
            Err(_) => {
                diesel::insert_into(prepare_qc::table)
                    .values((
                        dsl::id.eq(1),
                        dsl::message_type.eq(message_type),
                        dsl::view_number.eq(item.view_number.0 as i64),
                        dsl::node_hash.eq(item.node_hash.as_bytes()),
                        dsl::signature.eq(item.signature.as_ref().map(|s| s.to_bytes())),
                    ))
                    .execute(transaction.connection())
                    .map_err(|source| SqliteStorageError::DieselError {
                        source,
                        operation: "insert::prepare_qc".to_string(),
                    })?;
            },
        }
        Ok(())
    }

    fn get_prepare_qc(&self) -> Result<Option<QuorumCertificate>, Self::Error> {
        let connection = self.get_connection()?;
        use crate::schema::prepare_qc::dsl;
        let qc: Option<PrepareQc> = dsl::prepare_qc
            .find(1)
            .first(&connection)
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get_prepare_qc".to_string(),
            })?;
        qc.map(|qc| {
            Ok(QuorumCertificate::new(
                HotStuffMessageType::try_from(qc.message_type as u8).unwrap(),
                ViewId::from(qc.view_number as u64),
                qc.node_hash.try_into()?,
                qc.signature.map(|s| Signature::from_bytes(s.as_slice())),
            ))
        })
        .transpose()
    }

    fn commit(&self, transaction: &Self::BackendTransaction) -> Result<(), Self::Error> {
        debug!(target: LOG_TARGET, "Committing transaction");
        transaction
            .connection()
            .execute("COMMIT TRANSACTION")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "commit::chain".to_string(),
            })?;
        Ok(())
    }

    fn locked_qc_id(&self) -> Self::Id {
        1
    }

    fn prepare_qc_id(&self) -> Self::Id {
        1
    }

    fn find_highest_prepared_qc(&self) -> Result<QuorumCertificate, Self::Error> {
        use crate::schema::locked_qc::dsl;
        let connection = self.get_connection()?;
        // TODO: this should be a single row
        let result: Option<PrepareQc> = prepare_qc::table
            .order_by(prepare_qc::view_number.desc())
            .first(&connection)
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "find_highest_prepared_qc".to_string(),
            })?;
        let qc = match result {
            Some(r) => r,
            None => {
                let l: LockedQc = dsl::locked_qc
                    .find(self.locked_qc_id())
                    .first(&connection)
                    .map_err(|source| SqliteStorageError::DieselError {
                        source,
                        operation: "find_locked_qc".to_string(),
                    })?;
                PrepareQc {
                    id: 1,
                    message_type: l.message_type,
                    view_number: l.view_number,
                    node_hash: l.node_hash.clone(),
                    signature: l.signature,
                }
            },
        };

        Ok(QuorumCertificate::new(
            HotStuffMessageType::try_from(qc.message_type as u8).unwrap(),
            ViewId::from(qc.view_number as u64),
            qc.node_hash.try_into()?,
            qc.signature.map(|s| Signature::from_bytes(s.as_slice())),
        ))
    }

    fn get_locked_qc(&self) -> Result<QuorumCertificate, Self::Error> {
        use crate::schema::locked_qc::dsl;
        let connection = self.get_connection()?;
        let qc: LockedQc = dsl::locked_qc
            .find(self.locked_qc_id())
            .first(&connection)
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get_locked_qc".to_string(),
            })?;
        Ok(QuorumCertificate::new(
            HotStuffMessageType::try_from(qc.message_type as u8).unwrap(),
            ViewId::from(qc.view_number as u64),
            qc.node_hash.try_into()?,
            qc.signature.map(|s| Signature::from_bytes(s.as_slice())),
        ))
    }

    fn find_node_by_hash(&self, node_hash: &TreeNodeHash) -> Result<Option<(Self::Id, DbNode)>, Self::Error> {
        use crate::schema::nodes::dsl;
        let connection = self.get_connection()?;
        let node = dsl::nodes
            .filter(nodes::hash.eq(node_hash.as_bytes()))
            .first::<Node>(&connection)
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "find_node_by_hash".to_string(),
            })?;

        match node {
            Some(node) => Ok(Some((node.id, DbNode {
                hash: node.hash.try_into()?,
                parent: node.parent.try_into()?,
                height: node.height as u32,
                is_committed: node.is_committed,
            }))),
            None => Ok(None),
        }
    }

    fn find_node_by_parent_hash(&self, parent_hash: &TreeNodeHash) -> Result<Option<(Self::Id, DbNode)>, Self::Error> {
        use crate::schema::nodes::dsl;
        let connection = self.get_connection()?;
        let node = dsl::nodes
            .filter(nodes::parent.eq(parent_hash.as_bytes()))
            .first::<Node>(&connection)
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "find_node_by_hash".to_string(),
            })?;

        match node {
            Some(node) => Ok(Some((node.id, DbNode {
                hash: node.hash.try_into()?,
                parent: node.parent.try_into()?,
                height: node.height as u32,
                is_committed: node.is_committed,
            }))),
            None => Ok(None),
        }
    }

    fn insert_instruction(
        &self,
        item: &DbInstruction,
        transaction: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
        use crate::schema::nodes::dsl;
        // TODO: this could be made more efficient
        let node: Node = dsl::nodes
            .filter(nodes::hash.eq(&item.node_hash.as_bytes()))
            .first(transaction.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "insert_instruction::find_node".to_string(),
            })?;
        let new_instruction = NewInstruction {
            hash: item.instruction.hash().to_vec(),
            node_id: node.id,
            template_id: item.instruction.template_id() as i32,
            method: item.instruction.method().to_string(),
            args: Vec::from(item.instruction.args()),
        };
        diesel::insert_into(instructions::table)
            .values(new_instruction)
            .execute(transaction.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "insert_instruction".to_string(),
            })?;
        Ok(())
    }

    fn find_all_instructions_by_node(&self, node_id: Self::Id) -> Result<Vec<DbInstruction>, Self::Error> {
        use crate::schema::{instructions::dsl as instructions_dsl, nodes::dsl as nodes_dsl};
        let connection = self.get_connection()?;
        let node = nodes_dsl::nodes
            .filter(nodes::id.eq(node_id))
            .first::<Node>(&connection)
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "find_all_instructions_by_node::find_node".to_string(),
            })?;
        let instructions = instructions_dsl::instructions
            .filter(instructions::node_id.eq(&node.id))
            .load::<Instruction>(&connection)
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "find_all_instructions_by_node::filter_by_node_id".to_string(),
            })?;
        let node_hash = node.hash.try_into()?;
        let instructions = instructions
            .into_iter()
            .map(|i| {
                Ok(DbInstruction {
                    instruction: i.try_into()?,
                    node_hash,
                })
            })
            .collect::<Result<_, Self::Error>>()?;

        Ok(instructions)
    }
}
