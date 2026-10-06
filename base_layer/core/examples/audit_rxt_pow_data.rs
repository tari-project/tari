//  Copyright 2026, The Tari Project
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

//! Read-only scanner for non-canonical RandomXT `pow_data`.
//!
//! Walks the `headers` database of an existing base node LMDB store and prints `height,len,canonical,pow_data` for
//! every RandomXT header whose `pow_data` is not empty.
//!
//! `create_tari_mining_blob` pads `pow_data` out to 32 bytes before hashing, so only the *trailing zeros* are
//! invisible to the RandomX input - the meaningful bytes are not. A header is therefore an equal-work,
//! different-hash variant of a shorter block exactly when its `pow_data` ends in a zero byte, which is what the
//! fork rejects. A non-empty `pow_data` ending in a non-zero byte is already canonical and stays valid.
//!
//! This is the blocking check for the canonical RandomXT `pow_data` fork. It lists every non-empty `pow_data`, so
//! that both live forms show up - the empty one the rule accepts unchanged, and the 32 byte zero padded one that
//! has to drop its padding - but counts and reports the earliest offending height using the fork's own predicate,
//! so the figures can be used directly to pick an activation height for a network.
//!
//! ```text
//! cargo run --release --features rxt_pow_data_audit --example audit_rxt_pow_data -- <network> <db-path> [start] [end]
//! ```
//!
//! `<db-path>` is the directory holding `data.mdb`, e.g. `~/.tari/mainnet/data/base_node/db`.
//!
//! SCAN A STOPPED NODE, OR A COPY OF ITS DATABASE. `create_readonly_lmdb_environment` opens with `MDB_NOLOCK`, so
//! this reader is never entered in LMDB's reader table and a running node is free to recycle pages out from under
//! it. The short-lived transactions below are good manners, not protection: they bound how long a stale snapshot is
//! held, but nothing stops a concurrent writer from reusing a page mid-scan, and a torn read decodes to a
//! structurally valid but wrong header rather than to an error. Since these counts are used to choose a consensus
//! activation height, scan something that is not being written to.
//!
//! Unlike `audit_c29` nothing here is hashed, so `Network::set_current` is not load-bearing - it is still set, so
//! that the two tools are invoked identically.

use std::{env, path::PathBuf, process, str::FromStr};

use lmdb_zero::{Database, DatabaseOptions, ReadTransaction, db};
use tari_common::configuration::Network;
use tari_core::chain_storage::create_readonly_lmdb_environment;
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::tari_proof_of_work::PowAlgorithm;
use tari_utilities::hex::Hex;

/// The name of the LMDB database holding `height -> BlockHeader`. Mirrors `LMDB_DB_HEADERS`, which is private to
/// `chain_storage`.
const HEADERS_DB: &str = "headers";
/// Heights per read transaction.
const CHUNK: u64 = 2_000;

fn main() {
    if let Err(err) = run() {
        eprintln!("audit_rxt_pow_data: {err}");
        process::exit(1);
    }
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let Some(network_arg) = args.get(1) else {
        return Err("usage: audit_rxt_pow_data <network> <db-path> [start-height] [end-height]".to_string());
    };
    let Some(db_path) = args.get(2) else {
        return Err("usage: audit_rxt_pow_data <network> <db-path> [start-height] [end-height]".to_string());
    };

    let network = Network::from_str(network_arg).map_err(|e| format!("unknown network: {e}"))?;
    Network::set_current(network).map_err(|n| format!("network was already set to {n}"))?;

    let start: u64 = match args.get(3) {
        Some(v) => v.parse().map_err(|_| "start-height is not a number".to_string())?,
        None => 0,
    };
    let end: u64 = match args.get(4) {
        Some(v) => v.parse().map_err(|_| "end-height is not a number".to_string())?,
        None => u64::MAX,
    };

    let env = create_readonly_lmdb_environment(PathBuf::from(db_path)).map_err(|e| e.to_string())?;
    let headers = Database::open(env.clone(), Some(HEADERS_DB), &DatabaseOptions::new(db::INTEGERKEY))
        .map_err(|e| format!("could not open the `{HEADERS_DB}` database: {e}"))?;

    let mut scanned = 0u64;
    let mut randomxt = 0u64;
    let mut non_empty = 0u64;
    let mut offending = 0u64;
    let mut earliest_offending = None;
    let mut over_32 = 0u64;
    let mut height = start;
    let mut highest_seen = None;
    let mut stopped_at_gap = None;

    println!("height,len,canonical,pow_data");
    'chunks: loop {
        // A fresh, short-lived reader per chunk.
        let txn = ReadTransaction::new(env.clone()).map_err(|e| e.to_string())?;
        let chunk_end = height.saturating_add(CHUNK).min(end.saturating_add(1));
        {
            let access = txn.access();
            while height < chunk_end {
                let bytes: Option<&[u8]> = match access.get::<u64, [u8]>(&headers, &height) {
                    Ok(v) => Some(v),
                    Err(lmdb_zero::Error::Code(lmdb_zero::error::NOTFOUND)) => None,
                    Err(e) => return Err(format!("read failed at height {height}: {e}")),
                };
                let Some(bytes) = bytes else {
                    // A missing height is the tip on a node synced from genesis, but it is equally a hole in a
                    // pruned or still-syncing database. The two are indistinguishable from here, so record it and
                    // let the summary say so rather than silently reporting a partial scan as a complete one.
                    stopped_at_gap = Some(height);
                    break 'chunks;
                };
                let header: BlockHeader =
                    bincode::deserialize(bytes).map_err(|e| format!("could not decode header {height}: {e}"))?;
                scanned = scanned.saturating_add(1);
                highest_seen = Some(height);

                if header.pow_algo() == PowAlgorithm::RandomXT {
                    randomxt = randomxt.saturating_add(1);
                    let len = header.pow.pow_data.len();
                    if len > 0 {
                        non_empty = non_empty.saturating_add(1);
                        // The fork's actual predicate: a trailing zero byte means the header is a zero extension of
                        // a shorter, equal-work one. Anything else is already the canonical representative.
                        let canonical = header.pow.pow_data.last() != Some(&0);
                        if !canonical {
                            offending = offending.saturating_add(1);
                            if earliest_offending.is_none() {
                                earliest_offending = Some(header.height);
                            }
                        }
                        if len > 32 {
                            over_32 = over_32.saturating_add(1);
                        }
                        println!(
                            "{},{},{},{}",
                            header.height,
                            len,
                            canonical,
                            header.pow.pow_data.to_hex()
                        );
                    }
                }
                height = height.saturating_add(1);
            }
        }
        drop(txn);
        if height > end {
            break;
        }
    }

    eprintln!("scanned:                             {scanned}");
    eprintln!("randomxt (rxt) blocks:               {randomxt}");
    eprintln!("  with a non-empty pow_data:         {non_empty}");
    eprintln!("  the fork would reject:             {offending}   (pow_data ending in a zero byte)");
    eprintln!("  with a pow_data over 32 bytes:     {over_32}   (control: must be 0)");
    match earliest_offending {
        Some(h) => eprintln!("earliest offending height:           {h}   (first block the fork would reject)"),
        None => eprintln!("earliest offending height:           none"),
    }
    match highest_seen {
        Some(h) => eprintln!("highest height read:                 {h}"),
        None => eprintln!("highest height read:                 none - the range held no headers"),
    }
    match stopped_at_gap {
        Some(h) => eprintln!(
            "scan stopped at:                     no header at height {h}\n\
             \x20                                    this is the tip on a node synced from genesis, but it is a hole \
             in a pruned\n\
             \x20                                    or still-syncing database - check it against the node's reported \
             tip before\n\
             \x20                                    treating the counts above as complete"
        ),
        None => eprintln!("scan stopped at:                     end of the requested range ({end})"),
    }
    // An explicit end that the scan never reached means the counts cover less than what was asked for. Exit non-zero
    // so a script cannot mistake a truncated scan for a clean one.
    if let Some(h) = stopped_at_gap.filter(|h| end != u64::MAX && *h <= end) {
        return Err(format!(
            "incomplete scan: asked for heights {start}..={end} but there is no header at {h}; the counts above cover \
             only {start}..{h}"
        ));
    }
    Ok(())
}
