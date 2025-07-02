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

use log::{debug, error, info, trace};
use tari_storage::lmdb_store::DatabaseRef;

use crate::{
    blocks::BlockHeader,
    chain_storage::{
        lmdb_db::{
            lmdb::{lmdb_fetch_matching_after, lmdb_get, lmdb_replace},
            lmdb_db::fetch_chain_height,
            migrations::{
                manager::{Migration, MigrationStatus},
                MigrationContext,
            },
            TransactionOutputRowData,
        },
        ChainStorageError,
        LMDBDatabase,
    },
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db::migrations";

#[derive(Clone, Debug)]
struct Database {
    headers_db: DatabaseRef,
    utxos_db: DatabaseRef,
    payref_to_output_index: DatabaseRef,
}

/// MIGRATION: Add payref index or rebuild payref index to recover deleted payrefs if not done previously.
/// Note: This migration fixes the error introduced with the original payref migration where it was only added for
///       outputs in the unspent set, resulting in missing payrefs.
pub struct Migration04 {
    version: u64,
    ctx: MigrationContext,
    db: Database,
}

impl Migration04 {
    pub fn new(
        ctx: MigrationContext,
        headers_db: DatabaseRef,
        utxos_db: DatabaseRef,
        payref_to_output_index: DatabaseRef,
    ) -> Self {
        Self {
            version: 4,
            ctx,
            db: Database {
                headers_db,
                utxos_db,
                payref_to_output_index,
            },
        }
    }
}

impl Migration for Migration04 {
    fn version(&self) -> u64 {
        self.version
    }

    fn run(&self, _db: Option<&mut LMDBDatabase>) -> MigrationStatus {
        info!(target: LOG_TARGET, "[MIGRATIONS] Starting migration v{}", self.version());

        let ctx = self.ctx.clone();
        let db = self.db.clone();
        let version = self.version();
        tokio::spawn(async move {
            rebuild_payref_index(ctx, db, version).await;
        });

        MigrationStatus {
            completed_version: None,
            in_progress_version: Some(self.version()),
            migration_error: None,
        }
    }

    fn requires_backend(&self) -> bool {
        false
    }
}

fn persist_error(ctx: &MigrationContext, error: ChainStorageError, version: u64) {
    ctx.update_migration_status(MigrationStatus {
        completed_version: None,
        in_progress_version: None,
        migration_error: Some((version, error.to_string())),
    })
    .unwrap_or_else(|e| {
        error!(target: LOG_TARGET, "[MIGRATIONS] v{}: Failed to update migration status: {}", version, e);
    });
}

fn persist_completed_status(ctx: &MigrationContext, version: u64) {
    ctx.update_migration_status(MigrationStatus {
        completed_version: Some(version),
        in_progress_version: None,
        migration_error: None,
    })
    .unwrap_or_else(|e| {
        error!(target: LOG_TARGET, "[MIGRATIONS] v{}: Failed to update migration status: {}", version, e);
    });
}

async fn rebuild_payref_index(ctx: MigrationContext, db: Database, version: u64) {
    info!(target: LOG_TARGET, "[MIGRATIONS] v{}: Starting PayRef migration in background task", version);

    // Verify database consistency before starting migration
    let read_txn = match ctx.read_transaction() {
        Ok(txn) => txn,
        Err(e) => {
            persist_error(&ctx, e, version);
            return;
        },
    };
    let chain_height = match fetch_chain_height(&read_txn, &ctx.metadata_db) {
        Ok(v) => v,
        Err(_) => {
            // No chain data, skip PayRef rebuild
            persist_completed_status(&ctx, version);
            return;
        },
    };
    drop(read_txn);

    // db.set_stats_total_height(chain_height);

    for height in 0..=chain_height {
        // The average size added to the db per block for payrefs for the first 16,500 blocks was approximately
        // 4,209 bytes as measured on a mainnet node and for the next 17,500 blocks approximately 7,550 bytes.
        // The highest measured value is much less than the theoretical maximum of
        // `(1000 coinbases + 900 outputs) * 2 * 32 bytes per output = 242,200 bytes per block`. The
        // default db maspize increase is 128MB when we have less than 64MB free space left, so we should be
        // checking how long it will take to fill up 64MB. Taking the biggest measured value we end up with
        // approximately 8888 blocks to consume 64MB wirth of payref data. Theoretically, we can fill up 64MB
        // with 277 block's worth of payrefs. To test if the db needs resizing every 1000 blocks is deemed
        // practical and safe.
        if height % 1000 == 0 {
            if let Err(e) = ctx.resize_if_required() {
                persist_error(&ctx, e, version);
                return;
            }
        }
        if let Err(e) = process_payref_for_height(&ctx, height, &db) {
            persist_error(&ctx, e, version);
            return;
        }

        // if height % 50 == 0 {
        //     db.update_stats_progress(height);
        // }
    }

    info!(target: LOG_TARGET, "[MIGRATIONS] v{}: PayRef index rebuild completed", version);
    persist_completed_status(&ctx, version);
}

/// Process a batch of blocks for PayRef migration
fn process_payref_for_height(ctx: &MigrationContext, height: u64, db: &Database) -> Result<(), ChainStorageError> {
    debug!(target: LOG_TARGET, "Processing PayRef migration for {}", height);

    // Get all outputs for this block
    let read_txn = ctx.read_transaction()?;
    let read_header: Option<BlockHeader> = lmdb_get(&read_txn, &db.headers_db, &height)?;
    let header = read_header.ok_or_else(|| ChainStorageError::ValueNotFound {
        entity: "BlockHeader",
        field: "height",
        value: height.to_string(),
    })?;
    let block_hash = header.hash();
    let query_results: Vec<(Vec<u8>, TransactionOutputRowData)> =
        lmdb_fetch_matching_after(&read_txn, &db.utxos_db, block_hash.as_slice())?;
    drop(read_txn);

    // Add payrefs, replacing any existing ones
    let write_txn = ctx.write_transaction()?;
    for (_, output_data) in query_results {
        let payref = LMDBDatabase::generate_payment_reference_for_output(&block_hash, &output_data.hash);
        trace!(target: LOG_TARGET,
            "Processing payref {} and output hash {} for height {}",
            payref, output_data.hash, height
        );
        lmdb_replace(
            &write_txn,
            &db.payref_to_output_index,
            payref.as_slice(),
            &output_data.hash,
            None,
        )?;
    }

    // Commit the batch
    write_txn.commit()?;

    Ok(())
}
