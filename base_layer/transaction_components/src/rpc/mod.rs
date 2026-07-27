// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod models;

/// The maximum number of items (output hashes, signatures, commitments, ...) that a single batch query to the base
/// node may contain. Any request carrying more than this is rejected outright so that a single call can never force
/// the node into an unbounded amount of database work.
///
/// Callers are expected to split larger sets into multiple requests. The wallet's own batch sizes
/// (`tx_validator_batch_size` and `max_tx_query_batch_size`) default to well below this.
pub const MAX_ALLOWED_QUERY_SIZE: usize = 512;
