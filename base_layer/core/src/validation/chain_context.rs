// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{cmp, collections::HashMap};

use tari_common_types::types::FixedHash;

use crate::chain_storage::{BlockchainBackend, ChainStorageError};

/// The heights at which Monero RandomX seeds were first used by a chain that is not (yet) in the database.
///
/// Header sync validates a candidate chain well ahead of committing it - the whole of the first batch before it
/// switches chains at all, and in batches after that - so the database cannot answer "when did this chain first use
/// this seed" for the part of the chain that is still pending. This is the caller's record of that, built up header
/// by header as the candidate chain is proven.
#[derive(Debug, Clone, Default)]
pub struct MoneroSeedHeights {
    first_seen: HashMap<Vec<u8>, u64>,
}

impl MoneroSeedHeights {
    /// Records `height` as the height at which `seed` was used, keeping the lowest height seen for that seed.
    pub fn record(&mut self, seed: Vec<u8>, height: u64) {
        self.first_seen
            .entry(seed)
            .and_modify(|first_seen| *first_seen = cmp::min(*first_seen, height))
            .or_insert(height);
    }

    /// The lowest height at which `seed` has been recorded, if it has been recorded at all.
    pub fn first_seen(&self, seed: &[u8]) -> Option<u64> {
        self.first_seen.get(seed).copied()
    }

    /// The number of distinct seeds recorded.
    pub fn len(&self) -> usize {
        self.first_seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.first_seen.is_empty()
    }
}

/// The facts about the chain a header belongs to that header validation must not read from the database.
///
/// A header is not always validated against a database that holds the chain it extends:
///
/// * Header sync validates the whole of a candidate chain *before* `switch_to_pending_chain` rewinds, so for the
///   duration of validation the database still returns this node's own (possibly losing) fork.
/// * The orphan pool holds chains that fork below the current tip, which are validated against a database holding the
///   main chain.
///
/// Every chain dependent input therefore has to be supplied by the caller, which is what this type carries. Getting
/// this wrong is expensive: a header wrongly rejected here costs the peer a `BanPeriod::Long` ban, and on some paths
/// the header is also written to the bad block list, which outlives the sync attempt (it is restart scoped - cleared
/// on startup by default - and height pruned).
#[derive(Debug, Clone, Copy)]
pub struct HeaderChainContext<'a> {
    vm_key: FixedHash,
    trusted_db_height: u64,
    pending_monero_seeds: Option<&'a MoneroSeedHeights>,
}

impl<'a> HeaderChainContext<'a> {
    /// The header belongs to a candidate chain that the database does not hold.
    ///
    /// This is the only constructor, on purpose. There is deliberately no "the database holds this chain, trust all
    /// of it" variant: no production path can honour that precondition, because header sync validates a whole chain
    /// before it rewinds and the orphan pool holds chains that fork below the tip. A caller that genuinely extends
    /// the chain in the database passes that chain's tip height as `fork_height`, which says the same thing without
    /// being a blanket licence.
    ///
    /// `fork_height` is the highest height at which the candidate chain and the chain in the database are known to
    /// agree - the chain split for header sync, the fork point with the main chain for an orphan. Chain dependent
    /// data recorded above it belongs to the other chain and is ignored. Pass 0 when the agreement height cannot be
    /// established: that ignores the database entirely, which is always safe.
    ///
    /// `pending_monero_seeds` is what the caller has proven about the candidate chain above `fork_height`.
    pub fn candidate_chain(
        vm_key: FixedHash,
        fork_height: u64,
        pending_monero_seeds: Option<&'a MoneroSeedHeights>,
    ) -> Self {
        Self {
            vm_key,
            trusted_db_height: fork_height,
            pending_monero_seeds,
        }
    }

    /// The Tari RandomX VM key for the band this header falls in.
    pub fn vm_key(&self) -> FixedHash {
        self.vm_key
    }

    /// The height at which the chain being validated first used `seed`, or 0 if this chain has not been seen to use
    /// it before.
    ///
    /// The database index is keyed by seed alone and holds the lowest height at which *any* committed header used
    /// it, with no record of which chain that header was on, so it is only consulted at or below the height the two
    /// chains are known to agree on. Above that, only what the caller has proven about the candidate chain counts.
    pub fn monero_seed_first_seen_height<B: BlockchainBackend>(
        &self,
        db: &B,
        seed: &[u8],
    ) -> Result<u64, ChainStorageError> {
        let committed = db.fetch_monero_seed_first_seen_height(seed)?;
        // A committed height at or below the fork point is on the shared part of both chains, and it is the earliest
        // use there is, so it wins over anything the caller has recorded.
        if committed != 0 && committed <= self.trusted_db_height {
            return Ok(committed);
        }
        Ok(self
            .pending_monero_seeds
            .and_then(|seeds| seeds.first_seen(seed))
            .unwrap_or(0))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_keeps_the_lowest_height_per_seed() {
        let mut seeds = MoneroSeedHeights::default();
        assert!(seeds.is_empty());
        seeds.record(vec![1], 10);
        seeds.record(vec![1], 20);
        seeds.record(vec![2], 5);
        assert_eq!(seeds.first_seen(&[1]), Some(10));
        assert_eq!(seeds.first_seen(&[2]), Some(5));
        assert_eq!(seeds.first_seen(&[3]), None);
        assert_eq!(seeds.len(), 2);

        // A lower height replaces a higher one
        seeds.record(vec![1], 3);
        assert_eq!(seeds.first_seen(&[1]), Some(3));
    }
}
