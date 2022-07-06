//  Copyright 2022. The Tari Project
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

use diesel::{prelude::*, Connection, RunQueryDsl, SqliteConnection};
use tari_common_types::types::FixedHash;
use tari_dan_core::storage::global::{ContractState, GlobalDbBackendAdapter, GlobalDbMetadataKey};

use crate::{
    error::SqliteStorageError,
    global::models::{
        contract::{Contract, NewContract},
        metadata::GlobalMetadata,
    },
    SqliteTransaction,
};

#[derive(Clone)]
pub struct SqliteGlobalDbBackendAdapter {
    database_url: String,
}

impl SqliteGlobalDbBackendAdapter {
    pub fn new(database_url: String) -> Self {
        SqliteGlobalDbBackendAdapter { database_url }
    }
}

impl GlobalDbBackendAdapter for SqliteGlobalDbBackendAdapter {
    type BackendTransaction = SqliteTransaction;
    type Error = SqliteStorageError;
    type Model = Contract;
    type NewModel = NewContract;

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        let connection = SqliteConnection::establish(self.database_url.as_str())?;

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

    fn set_data(&self, key: GlobalDbMetadataKey, value: &[u8]) -> Result<(), Self::Error> {
        use crate::global::schema::metadata;
        let tx = self.create_transaction()?;

        match self.get_data_with_connection(&key, &tx) {
            Ok(Some(r)) => diesel::update(&GlobalMetadata {
                key_name: key.as_key_bytes().to_vec(),
                value: r,
            })
            .set(metadata::value.eq(value))
            .execute(tx.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "update::metadata".to_string(),
            })?,
            Ok(None) => diesel::insert_into(metadata::table)
                .values((metadata::key_name.eq(key.as_key_bytes()), metadata::value.eq(value)))
                .execute(tx.connection())
                .map_err(|source| SqliteStorageError::DieselError {
                    source,
                    operation: "insert::metadata".to_string(),
                })?,
            Err(e) => return Err(e),
        };

        self.commit(&tx)?;

        Ok(())
    }

    fn get_data(&self, key: GlobalDbMetadataKey) -> Result<Option<Vec<u8>>, Self::Error> {
        use crate::global::schema::metadata::dsl;
        let connection = SqliteConnection::establish(self.database_url.as_str())?;

        let row: Option<GlobalMetadata> = dsl::metadata
            .find(key.as_key_bytes())
            .first(&connection)
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get::metadata_key".to_string(),
            })?;

        Ok(row.map(|r| r.value))
    }

    fn get_data_with_connection(
        &self,
        key: &GlobalDbMetadataKey,
        tx: &Self::BackendTransaction,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        use crate::global::schema::metadata::dsl;

        let row: Option<GlobalMetadata> = dsl::metadata
            .find(key.as_key_bytes())
            .first(tx.connection())
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get::metadata_key".to_string(),
            })?;

        Ok(row.map(|r| r.value))
    }

    fn commit(&self, tx: &Self::BackendTransaction) -> Result<(), Self::Error> {
        tx.connection()
            .execute("COMMIT TRANSACTION;")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "commit::state".to_string(),
            })?;
        Ok(())
    }

    fn save_contract(&self, mut contract: NewContract, state: ContractState) -> Result<(), Self::Error> {
        use crate::global::schema::contracts;
        let tx = self.create_transaction()?;

        contract.with_state(state);

        diesel::insert_into(contracts::table)
            .values(&contract)
            .execute(tx.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "insert::contract".to_string(),
            })?;

        self.commit(&tx)?;

        Ok(())
    }

    fn update_contract_state(&self, contract_id: FixedHash, state: ContractState) -> Result<(), Self::Error> {
        use crate::global::schema::contracts;
        let tx = self.create_transaction()?;

        diesel::update(contracts::table.filter(contracts::contract_id.eq(contract_id.to_vec())))
            .set(contracts::state.eq(i32::from(state.as_byte())))
            .execute(tx.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "update::contract_state".to_string(),
            })?;

        self.commit(&tx)?;

        Ok(())
    }

    fn get_contracts_with_state(&self, state: ContractState) -> Result<Vec<Contract>, Self::Error> {
        use crate::global::schema::{contracts, contracts::dsl};
        let tx = self.create_transaction()?;

        dsl::contracts
            .filter(contracts::state.eq(i32::from(state.as_byte())))
            .load::<Contract>(tx.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get::contracts".to_string(),
            })
    }
}
