// Copyright 2021. The Tari Project
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

use std::convert::TryFrom;

use chrono::{NaiveDateTime, Utc};
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SqliteConnection};
use tari_common_types::types::FixedHash;
use tari_utilities::ByteArray;

use crate::{
    diesel::OptionalExtension,
    error::WalletStorageError,
    schema::scanned_blocks,
    utxo_scanner_service::service::{
        SCANNED_BLOCK_CACHE_SIZE,
        SCANNED_BLOCK_CACHE_SIZE_MEDIUM,
        SCANNED_BLOCK_CACHE_SIZE_SPARSE,
        ScannedBlock,
    },
};

#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[diesel(table_name = scanned_blocks)]
pub struct ScannedBlockSql {
    header_hash: Vec<u8>,
    height: i64,
    num_outputs: Option<i64>,
    amount: Option<i64>,
    timestamp: NaiveDateTime,
}

impl ScannedBlockSql {
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<ScannedBlockSql>, WalletStorageError> {
        Ok(scanned_blocks::table
            .order(scanned_blocks::height.desc())
            .load::<ScannedBlockSql>(conn)?)
    }

    pub fn last_height(conn: &mut SqliteConnection) -> Result<Option<i64>, WalletStorageError> {
        let result = scanned_blocks::table
            .select(scanned_blocks::height)
            .order(scanned_blocks::height.desc())
            .first::<i64>(conn)
            .optional()?;
        Ok(result)
    }

    pub fn new(header_hash: Vec<u8>, height: i64) -> Self {
        Self {
            header_hash,
            height,
            amount: None,
            num_outputs: None,
            timestamp: Utc::now().naive_utc(),
        }
    }

    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        // The wallet only ever needs a single scanned-block entry per height: the canonical block
        // it scanned at that height. The base node can legitimately re-deliver an already scanned
        // block - an overlapping `sync_utxos_by_block` stream, a retried scanning round, or a reorg
        // that produces a *different* block at a height we have already saved.
        //
        // `scanned_blocks` is keyed on `header_hash`, not `height`, which makes both of those cases
        // misbehave:
        //   - A plain `insert_into` of a re-delivered (identical) header hash fails with
        //     `UNIQUE constraint failed: scanned_blocks.header_hash`, aborting the whole scan round.
        //   - A `replace_into` on the hash silences that case, but when the new block at the height
        //     has a *different* hash it inserts a second row, leaving a stale duplicate-height entry
        //     behind that desyncs last-scanned-block detection and the sparse-pruning schedule.
        //
        // Delete any existing row at this height first, then insert, in a single transaction so the
        // table always holds exactly one row per height regardless of which case we hit. `replace_into`
        // is retained as a defensive measure - a given header hash commits to its height so it can
        // never legitimately appear at another height, but this guarantees we never throw here.
        conn.transaction::<_, WalletStorageError, _>(|conn| {
            diesel::delete(scanned_blocks::table.filter(scanned_blocks::height.eq(self.height))).execute(conn)?;
            diesel::replace_into(scanned_blocks::table)
                .values(self)
                .execute(conn)?;
            Ok(())
        })
    }

    pub fn clear_all(conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::delete(scanned_blocks::table).execute(conn)?;
        Ok(())
    }

    /// Clear Scanned Blocks from the given height and higher
    pub fn clear_from_and_higher(height: u64, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::delete(scanned_blocks::table.filter(scanned_blocks::height.ge(height as i64))).execute(conn)?;
        Ok(())
    }

    pub fn clear_before_height(height: u64, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::delete(scanned_blocks::table.filter(scanned_blocks::height.lt(height as i64))).execute(conn)?;
        Ok(())
    }

    /// Apply a sparse storage schedule to scanned blocks, keeping:
    /// - All blocks within `tip - 720` to `tip`
    /// - Every 100th block from `tip - 10,000` to `tip - 720`
    /// - Every 1,000th block from `tip - 100,000` to `tip - 10,000`
    /// - Every 5,000th block from genesis to `tip - 100,000`
    ///
    /// Implemented as a single `DELETE ... WHERE` so the scanned_blocks table
    /// is not materialised in memory even when the wallet has scanned tens of
    /// thousands of headers.
    pub fn apply_sparse_schedule(tip_height: u64, conn: &mut SqliteConnection) -> Result<(), WalletStorageError> {
        // Pre-compute the retention-window boundaries once and clamp to i64.
        // `saturating_sub` keeps the query well-defined at low tip heights
        // (e.g. immediately after a fresh scan or in unit tests).
        let tip = i64::try_from(tip_height).unwrap_or(i64::MAX);
        let recent_boundary = tip.saturating_sub(SCANNED_BLOCK_CACHE_SIZE as i64);
        let medium_boundary = tip.saturating_sub(SCANNED_BLOCK_CACHE_SIZE_MEDIUM as i64);
        let sparse_boundary = tip.saturating_sub(SCANNED_BLOCK_CACHE_SIZE_SPARSE as i64);

        // Boundary semantics match the original Rust logic:
        //   keep                depth ∈ [0, 720]            → height >= recent_boundary
        //   band 1 (%100)       depth ∈ (720, 10_000]       → height ∈ [medium_boundary, recent_boundary)
        //   band 2 (%1_000)     depth ∈ (10_000, 100_000]   → height ∈ [sparse_boundary, medium_boundary)
        //   band 3 (%5_000)     depth ∈ (100_000, ∞)        → height <  sparse_boundary
        //
        // The outer `height < recent_boundary` guard excludes the last 720
        // blocks from any deletion. Each inner branch deletes only the rows
        // whose height is NOT on the modulus appropriate for their band.
        diesel::sql_query(
            "DELETE FROM scanned_blocks WHERE height < ? AND ( (height >= ? AND (height % 100) != 0) OR (height <  ? \
             AND height >= ? AND (height % 1000) != 0) OR (height <  ? AND (height % 5000) != 0) )",
        )
        .bind::<diesel::sql_types::BigInt, _>(recent_boundary)
        .bind::<diesel::sql_types::BigInt, _>(medium_boundary)
        .bind::<diesel::sql_types::BigInt, _>(medium_boundary)
        .bind::<diesel::sql_types::BigInt, _>(sparse_boundary)
        .bind::<diesel::sql_types::BigInt, _>(sparse_boundary)
        .execute(conn)?;

        Ok(())
    }
}

impl From<ScannedBlock> for ScannedBlockSql {
    fn from(sb: ScannedBlock) -> Self {
        Self {
            header_hash: sb.header_hash.to_vec(),
            height: sb.height as i64,
            amount: None,
            num_outputs: None,
            timestamp: sb.timestamp,
        }
    }
}

impl TryFrom<ScannedBlockSql> for ScannedBlock {
    type Error = String;

    fn try_from(sb: ScannedBlockSql) -> Result<Self, Self::Error> {
        Ok(Self {
            header_hash: FixedHash::try_from(sb.header_hash).map_err(|err| err.to_string())?,
            height: sb.height as u64,
            timestamp: sb.timestamp,
        })
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use diesel::SqliteConnection;
    use tari_common_sqlite::sqlite_connection_pool::PooledDbConnection;

    use super::*;
    use crate::storage::sqlite_utilities::run_migration_and_create_sqlite_memory_connection;

    /// Keep the `WalletDbConnection` alive for the lifetime of the test so the
    /// `:memory:` SQLite pool doesn't drop between calls.
    struct TestDb {
        _conn: crate::storage::sqlite_utilities::WalletDbConnection,
    }

    impl TestDb {
        fn new() -> (
            Self,
            diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<SqliteConnection>>,
        ) {
            let conn = run_migration_and_create_sqlite_memory_connection().unwrap();
            let pooled = conn.get_pooled_connection().unwrap();
            (Self { _conn: conn }, pooled)
        }
    }

    fn insert_heights(conn: &mut SqliteConnection, heights: &[i64]) {
        for &h in heights {
            // Encode the full i64 height into the first 8 bytes of the hash so
            // every height gets a distinct primary key — `header_hash` is the PK
            // of scanned_blocks and `replace_into` would silently overwrite
            // earlier rows if they shared the same hash (which the old `(h & 0xff)`
            // approach caused for heights 256 apart).
            let mut hash = [0u8; 32];
            hash[..8].copy_from_slice(&h.to_le_bytes());
            ScannedBlockSql::new(hash.to_vec(), h).commit(conn).unwrap();
        }
    }

    fn remaining_heights(conn: &mut SqliteConnection) -> Vec<i64> {
        let mut blocks = ScannedBlockSql::index(conn).unwrap();
        blocks.sort_by_key(|b| b.height);
        blocks.into_iter().map(|b| b.height).collect()
    }

    fn hash_for(height: i64, variant: u8) -> Vec<u8> {
        // Distinct hash per (height, variant) so a re-save with a different variant models a
        // fork: the base node serving a *different* block at a height we already scanned.
        let mut hash = [0u8; 32];
        hash[..8].copy_from_slice(&height.to_le_bytes());
        hash[8] = variant;
        hash.to_vec()
    }

    #[test]
    fn commit_replaces_identical_block_without_unique_constraint_error() {
        let (_db, mut conn) = TestDb::new();
        // Re-saving the exact same block (same hash, same height) - e.g. an overlapping sync
        // stream or a retried scanning round - must not raise a UNIQUE constraint error.
        ScannedBlockSql::new(hash_for(100, 0), 100).commit(&mut conn).unwrap();
        ScannedBlockSql::new(hash_for(100, 0), 100).commit(&mut conn).unwrap();

        let blocks = ScannedBlockSql::index(&mut conn).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].height, 100);
        assert_eq!(blocks[0].header_hash, hash_for(100, 0));
    }

    #[test]
    fn commit_keeps_a_single_row_per_height_on_fork() {
        let (_db, mut conn) = TestDb::new();
        // Save the original block at height 100, then a reorged block with a *different* hash at
        // the same height. The stale fork row must be replaced, not accumulated alongside it.
        ScannedBlockSql::new(hash_for(100, 0), 100).commit(&mut conn).unwrap();
        ScannedBlockSql::new(hash_for(100, 1), 100).commit(&mut conn).unwrap();

        let blocks = ScannedBlockSql::index(&mut conn).unwrap();
        assert_eq!(blocks.len(), 1, "duplicate-height rows must not accumulate on a fork");
        assert_eq!(blocks[0].header_hash, hash_for(100, 1), "the latest block must win");
    }

    #[test]
    fn sparse_schedule_keeps_last_720_blocks_intact() {
        let (_db, mut conn) = TestDb::new();
        let heights: Vec<i64> = (0..=720).collect();
        insert_heights(&mut conn, &heights);

        ScannedBlockSql::apply_sparse_schedule(720, &mut conn).unwrap();

        // Every single height in [0..=720] has depth <= 720 so nothing is pruned.
        assert_eq!(remaining_heights(&mut conn), heights);
    }

    #[test]
    fn sparse_schedule_prunes_medium_band_to_every_100th_block() {
        let (_db, mut conn) = TestDb::new();
        // Tip = 5000 → heights 4281..=5000 are within the 720-block window and
        // must survive verbatim, while heights 0..=4280 are pruned to every 100th.
        let heights: Vec<i64> = (0..=5000).collect();
        insert_heights(&mut conn, &heights);

        ScannedBlockSql::apply_sparse_schedule(5000, &mut conn).unwrap();

        let remaining = remaining_heights(&mut conn);
        for h in 4281..=5000 {
            assert!(remaining.contains(&h), "missing in-window height {h}");
        }
        for h in (0..=4200).step_by(100) {
            assert!(remaining.contains(&h), "missing modulus-100 height {h}");
        }
        assert!(!remaining.contains(&123));
        assert!(!remaining.contains(&4279));
    }

    #[test]
    fn sparse_schedule_prunes_deep_bands_to_every_1000th_and_5000th_blocks() {
        let (_db, mut conn) = TestDb::new();
        let mut heights: Vec<i64> = Vec::new();
        heights.extend((0..=200).map(|i| i * 5_000));
        heights.extend([999_999, 999_500, 999_000, 900_500, 900_000, 100_123, 1_001, 1_000]);
        heights.sort();
        heights.dedup();
        insert_heights(&mut conn, &heights);

        let tip = 1_000_000u64;
        ScannedBlockSql::apply_sparse_schedule(tip, &mut conn).unwrap();

        let remaining = remaining_heights(&mut conn);
        // depth <= 720 → always kept.
        assert!(remaining.contains(&999_500));
        assert!(remaining.contains(&999_999));
        // depth 1_000 (720..10_000 band) on the 100-modulus grid → kept.
        assert!(remaining.contains(&999_000));
        // depth 100_000 (10_000..100_000 band); 900_000 % 1000 == 0 → kept.
        assert!(remaining.contains(&900_000));
        // In 10_000..100_000 band but not on a 1000 multiple → removed.
        assert!(!remaining.contains(&900_500));
        // Deepest band, not on 5000 multiple → removed.
        assert!(!remaining.contains(&100_123));
        assert!(!remaining.contains(&1_001));
        assert!(!remaining.contains(&1_000));
        // 0 is on every modulus → kept.
        assert!(remaining.contains(&0));
        assert!(remaining.contains(&5_000));
    }

    #[test]
    fn sparse_schedule_is_idempotent() {
        let (_db, mut conn) = TestDb::new();
        let heights: Vec<i64> = (0..=2_000).collect();
        insert_heights(&mut conn, &heights);

        ScannedBlockSql::apply_sparse_schedule(2_000, &mut conn).unwrap();
        let after_first = remaining_heights(&mut conn);
        ScannedBlockSql::apply_sparse_schedule(2_000, &mut conn).unwrap();
        let after_second = remaining_heights(&mut conn);

        assert_eq!(after_first, after_second);
    }

    #[test]
    fn sparse_schedule_handles_small_tip_without_saturating_underflow() {
        let (_db, mut conn) = TestDb::new();
        let heights: Vec<i64> = (0..=100).collect();
        insert_heights(&mut conn, &heights);

        // tip < 720: every block is within the retention window.
        ScannedBlockSql::apply_sparse_schedule(100, &mut conn).unwrap();

        assert_eq!(remaining_heights(&mut conn), heights);
    }
}
