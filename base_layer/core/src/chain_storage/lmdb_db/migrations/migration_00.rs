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
use primitive_types::{U256, U512};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{BlockHash, HashOutput, PrivateKey};
use tari_storage::lmdb_store::DatabaseRef;

use crate::{
    blocks::BlockHeaderAccumulatedData,
    chain_storage::{
        lmdb_db::{
            lmdb::{lmdb_all, lmdb_get, lmdb_replace},
            lmdb_db::{fetch_chain_height, MetadataKey, MetadataValue},
            migrations::{
                manager::{Migration, MigrationStatus},
                MigrationContext,
            },
        },
        BlockchainBackend,
        ChainStorageError,
        ChainTipData,
        HorizonData,
        LMDBDatabase,
    },
    proof_of_work::{AccumulatedDifficulty, Difficulty},
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db::migrations";

struct Database {
    header_accumulated_data_db: DatabaseRef,
    orphan_header_accumulated_data_db: DatabaseRef,
    block_accumulated_data_db: DatabaseRef,
    orphan_chain_tips_db: DatabaseRef,
}

/// MIGRATION: Accumulated difficulty migration after the 3rd mining algorithm was introduced
pub struct Migration00 {
    version: u64,
    ctx: MigrationContext,
    db: Database,
}

impl Migration00 {
    pub fn new(
        ctx: MigrationContext,
        header_accumulated_data_db: DatabaseRef,
        orphan_header_accumulated_data_db: DatabaseRef,
        block_accumulated_data_db: DatabaseRef,
        orphan_chain_tips_db: DatabaseRef,
    ) -> Self {
        Self {
            version: 0,
            ctx,
            db: Database {
                header_accumulated_data_db,
                orphan_header_accumulated_data_db,
                block_accumulated_data_db,
                orphan_chain_tips_db,
            },
        }
    }

    fn perform_migration(&self, db_backend: &mut LMDBDatabase) -> Result<(), ChainStorageError> {
        let txn = self.ctx.read_transaction()?;

        let chain_height = match fetch_chain_height(&txn, &self.ctx.metadata_db) {
            Ok(v) => v,
            Err(_) => {
                // if the chain height does not exist, then we know we dont have a db
                info!(target: LOG_TARGET, "[MIGRATIONS] v{}: No chain db to migrate, skipping migration", self.version);
                return Ok(());
            },
        };

        let k = MetadataKey::AccumulatedWork;

        let val: Option<OldMetadataValue> = lmdb_get(&txn, &self.ctx.metadata_db, &k.as_u32())?;
        if val.is_some() {
            let accum_data = match val {
                Some(OldMetadataValue::AccumulatedWork(accumulated_difficulty)) => {
                    Ok(U512::from(accumulated_difficulty))
                },
                _ => Err(ChainStorageError::ValueNotFound {
                    entity: "ChainMetadata",
                    field: "AccumulatedWork",
                    value: "".to_string(),
                }),
            }?;
            let txn = self.ctx.write_transaction()?;
            lmdb_replace(
                &txn,
                &self.ctx.metadata_db,
                &k.as_u32(),
                &MetadataValue::AccumulatedWork(accum_data),
                None,
            )?;
            txn.commit()?;
            info!(target: LOG_TARGET, "[MIGRATIONS] v{}: Replaced tip accumulated data ", self.version);
        }
        let txn = self.ctx.write_transaction()?;
        for height in 0..=chain_height {
            let block_accum_data: V0BLockHeaderAccumulatedData =
                lmdb_get(&txn, &self.db.header_accumulated_data_db, &height)?.ok_or_else(|| {
                    ChainStorageError::ValueNotFound {
                        entity: "BlockAccumulatedData",
                        field: "height",
                        value: height.to_string(),
                    }
                })?;
            let new_block_accum_data = BlockHeaderAccumulatedData {
                hash: block_accum_data.hash,
                total_kernel_offset: block_accum_data.total_kernel_offset,
                achieved_difficulty: block_accum_data.achieved_difficulty,
                total_accumulated_difficulty: U512::from(block_accum_data.total_accumulated_difficulty),
                accumulated_monero_randomx_difficulty: block_accum_data.accumulated_randomx_difficulty,
                accumulated_tari_randomx_difficulty: AccumulatedDifficulty::min(),
                accumulated_sha3x_difficulty: block_accum_data.accumulated_sha3x_difficulty,
                target_difficulty: block_accum_data.target_difficulty,
            };

            lmdb_replace(
                &txn,
                &self.db.header_accumulated_data_db,
                &height,
                &new_block_accum_data,
                None,
            )?;

            // Update stats progress
            if height % 50 == 0 {
                db_backend.update_stats_progress(height);
            }
        }
        txn.commit()?;
        let txn = self.ctx.write_transaction()?;
        info!(target: LOG_TARGET, "[MIGRATIONS] v{}: Replaced accumulated data for blocks", self.version);
        let orphan_headers_accum_data: Vec<(Vec<u8>, V0BLockHeaderAccumulatedData)> =
            lmdb_all(&txn, &self.db.orphan_header_accumulated_data_db)?;
        for (hash, orphan_header_accum_data) in orphan_headers_accum_data {
            let new_orphan_block_accum_data = BlockHeaderAccumulatedData {
                hash: orphan_header_accum_data.hash,
                total_kernel_offset: orphan_header_accum_data.total_kernel_offset,
                achieved_difficulty: orphan_header_accum_data.achieved_difficulty,
                total_accumulated_difficulty: U512::from(orphan_header_accum_data.total_accumulated_difficulty),
                accumulated_monero_randomx_difficulty: orphan_header_accum_data.accumulated_randomx_difficulty,
                accumulated_tari_randomx_difficulty: AccumulatedDifficulty::min(),
                accumulated_sha3x_difficulty: orphan_header_accum_data.accumulated_sha3x_difficulty,
                target_difficulty: orphan_header_accum_data.target_difficulty,
            };
            lmdb_replace(
                &txn,
                &self.db.block_accumulated_data_db,
                &hash,
                &new_orphan_block_accum_data,
                None,
            )?;
        }
        txn.commit()?;
        let txn = self.ctx.write_transaction()?;
        info!(target: LOG_TARGET, "[MIGRATIONS] v{}: Replaced accumulated data for orphan blocks", self.version);
        let orphan_chain_tips: Vec<(Vec<u8>, OldChainTipData)> = lmdb_all(&txn, &self.db.orphan_chain_tips_db)?;

        for (parent_hash, val) in orphan_chain_tips {
            let val = ChainTipData {
                hash: val.hash,
                total_accumulated_difficulty: U512::from(val.total_accumulated_difficulty),
            };
            lmdb_replace(&txn, &self.db.orphan_chain_tips_db, &parent_hash, &val, None)?;
        }
        txn.commit()?;

        Ok(())
    }
}

impl Migration for Migration00 {
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

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub(crate) struct V0BLockHeaderAccumulatedData {
    /// The block hash.
    pub hash: HashOutput,
    /// The total accumulated offset for all kernels in the block.
    pub total_kernel_offset: PrivateKey,
    /// The achieved difficulty for solving the current block using the specified proof of work algorithm.
    pub achieved_difficulty: Difficulty,
    /// The total accumulated difficulty for all blocks since Genesis, but not including this block, tracked
    /// separately.
    pub total_accumulated_difficulty: U256,
    /// The total accumulated difficulty for RandomX proof of work for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_randomx_difficulty: AccumulatedDifficulty,
    /// The total accumulated difficulty for SHA3 proof of work for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_sha3x_difficulty: AccumulatedDifficulty,
    /// The target difficulty for solving the current block using the specified proof of work algorithm.
    pub target_difficulty: Difficulty,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) enum OldMetadataValue {
    ChainHeight(u64),
    BestBlock(BlockHash),
    AccumulatedWork(U256),
    PruningHorizon(u64),
    PrunedHeight(u64),
    HorizonData(HorizonData),
    BestBlockTimestamp(u64),
    MigrationVersion(u64),
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct OldChainTipData {
    pub hash: HashOutput,
    pub total_accumulated_difficulty: U256,
}
