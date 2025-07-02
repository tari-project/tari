//  Copyright 2025, The Tari Project
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

use log::info;
use primitive_types::U512;
use tari_storage::lmdb_store::DatabaseRef;

use crate::{
    blocks::BlockHeaderAccumulatedData,
    chain_storage::{
        blockchain_database::rewind_to_height,
        lmdb_db::{
            lmdb::lmdb_get,
            migrations::{
                manager::{Migration, MigrationStatus},
                MigrationContext,
            },
        },
        ChainStorageError,
        LMDBDatabase,
    },
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db::migrations";

struct Database {
    header_accumulated_data_db: DatabaseRef,
}

/// MIGRATION: Accumulated difficulty migration after the 3rd mining algorithm was introduced
pub struct Migration01 {
    version: u64,
    ctx: MigrationContext,
    db: Database,
}

impl Migration01 {
    pub fn new(ctx: MigrationContext, header_accumulated_data_db: DatabaseRef) -> Self {
        Self {
            version: 1,
            ctx,
            db: Database {
                header_accumulated_data_db,
            },
        }
    }

    fn perform_migration(&self, db_backend: &mut LMDBDatabase) -> Result<(), ChainStorageError> {
        let known_good_difficulties = get_correct_accumulated_difficulty();
        if known_good_difficulties.is_empty() {
            info!(
                target: LOG_TARGET,
                "[MIGRATIONS] v{}: No migration to perform for version network",
                self.version
            );
            return Ok(());
        }
        let mut last_correct_height = 0;
        for (height, correct_difficulty) in known_good_difficulties {
            let txn = self.ctx.read_transaction()?;
            let accum_data: Option<BlockHeaderAccumulatedData> =
                lmdb_get(&txn, &self.db.header_accumulated_data_db, &height)?;
            if let Some(accum_data) = accum_data {
                if accum_data.total_accumulated_difficulty == correct_difficulty {
                    info!(
                        target: LOG_TARGET,
                        "[MIGRATIONS] v{}: Block height {} already has correct accumulated difficulty",
                        self.version, height
                    );
                    last_correct_height = height;
                }
            } else {
                info!(
                    target: LOG_TARGET,
                    "[MIGRATIONS] v{}: No accumulated difficulty found for block height {}",
                    self.version, height
                );
                break;
            }
        }
        if last_correct_height == 0 {
            // this will happen only happen if the db is below the fork height of the RxT fork
            info!(
                target: LOG_TARGET,
                "[MIGRATIONS] v{}: No migration to perform for version network",
                self.version
            );
            return Ok(());
        }
        // lets rewind to last known good accumulated difficulty so the db can be correctly calculated again
        rewind_to_height(db_backend, last_correct_height)?;

        Ok(())
    }
}

impl Migration for Migration01 {
    fn version(&self) -> u64 {
        self.version
    }

    fn run(&self, db: Option<&mut LMDBDatabase>) -> MigrationStatus {
        let db = match db {
            Some(db) => db,
            None => {
                return MigrationStatus {
                    completed_version: None,
                    in_progress_version: None,
                    migration_error: Some((
                        self.version,
                        ChainStorageError::CriticalError("BlockchainBackend required to run migration".to_string())
                            .to_string(),
                    )),
                };
            },
        };

        info!(target: LOG_TARGET, "[MIGRATIONS] Starting migration v{}", self.version);
        if let Err(e) = self.perform_migration(db) {
            return MigrationStatus {
                completed_version: None,
                in_progress_version: None,
                migration_error: Some((self.version, e.to_string())),
            };
        }
        info!(target: LOG_TARGET, "[MIGRATIONS] Migration v{} completed", self.version);

        MigrationStatus {
            completed_version: Some(self.version),
            in_progress_version: None,
            migration_error: None,
        }
    }

    fn requires_backend(&self) -> bool {
        true
    }
}

pub(crate) fn get_correct_accumulated_difficulty() -> Vec<(u64, U512)> {
    #[cfg(tari_target_network_mainnet)]
    {
        vec![
            (
                14999,
                U512::from_dec_str("230963847231029670329787338266632060").expect("should not fail"),
            ),
            (
                16000,
                U512::from_dec_str("37870972808147006178902366165325544920691850526080").expect("should not fail"),
            ),
            (
                17000,
                U512::from_dec_str("123219722351554302645774736761840507999792186766920").expect("should not fail"),
            ),
            (
                18000,
                U512::from_dec_str("245169616636012105701848119083014332169855273375890").expect("should not fail"),
            ),
            (
                19000,
                U512::from_dec_str("428081108397470519627923902616128115025981546384670").expect("should not fail"),
            ),
            (
                20000,
                U512::from_dec_str("678404434598953994059276298108149917133080906779800").expect("should not fail"),
            ),
        ]
    }
    #[cfg(tari_target_network_nextnet)]
    {
        vec![
            (
                1499,
                U512::from_dec_str("17340317256602964156796").expect("should not fail"),
            ),
            (
                2000,
                U512::from_dec_str("267045542397987769905169797604842").expect("should not fail"),
            ),
            (
                3000,
                U512::from_dec_str("2261524423095838119669981829692352").expect("should not fail"),
            ),
        ]
    }
    #[cfg(not(any(tari_target_network_mainnet, tari_target_network_nextnet)))]
    vec![]
}
