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

//! Read-only scanner for merged-namespace Cuckaroo proofs (GHSA-3qmx-q9pv-f3m4).
//!
//! Walks the `headers` database of an existing base node LMDB store, re-verifies every c29 proof under *both*
//! Cuckaroo verifiers, and prints `height,timestamp` for every header the legacy merged-namespace verifier accepts
//! but the bipartite reference verifier rejects.
//!
//! ```text
//! cargo run --release --features c29_audit --example audit_c29 -- <network> <db-path> [start] [end]
//! ```
//!
//! `<db-path>` is the directory holding `data.mdb`, e.g. `~/.tari/mainnet/data/base_node/db`.
//!
//! **Point this at a stopped node, or at a filesystem copy of its database.** `create_readonly_lmdb_environment`
//! opens with `NOLOCK | RDONLY | NOTLS`. `RDONLY` does mean the tool cannot write to, corrupt or lock the store.
//! But `NOLOCK` means this reader is never registered in LMDB's reader table, so a running node's writer will
//! happily recycle pages while they are being traversed: the result is a torn read, or a segfault inside the mmap.
//! Those flags are set by the shared `create_readonly_lmdb_environment` helper and are not this tool's to change.
//!
//! Three properties of this tool are load-bearing and were all learned the hard way:
//!
//! 1. **`Network::set_current` is called before anything else.** `DomainSeparatedConsensusHasher` is network scoped, so
//!    without it every `mining_hash()` is wrong and *every* c29 block looks forged. The first run of this audit
//!    reported 100% failure for exactly this reason.
//! 2. **Read transactions are chunked.** This is *not* about reader-pinned page bloat on a live node: under `NOLOCK`
//!    this reader is invisible to the writer, so it cannot pin anything. Chunking bounds the amount of work thrown away
//!    when a torn read or a decode failure aborts a transaction, and keeps the address space a single transaction maps
//!    bounded on a multi-million-block store. The safety argument for a live node is the warning above, not this.
//! 3. **The control assertion is kept on both sides of the activation height, and it is exact on both.** Below the
//!    activation height the node validated with the legacy verifier, so it accepted every stored header by definition
//!    and *any* legacy rejection means the header was reconstructed wrong. That control is what caught the bad first
//!    run. At and above the activation height the node validated with the *bipartite* verifier, and legacy is not a
//!    superset of it: a numeric collision between one of the 21 U endpoints and one of the 21 V endpoints gives that
//!    value four neighbours in the merged adjacency map, so `verify_from_edges_legacy` returns
//!    `NodeHasMoreThanTwoEdges` on a proof the bipartite verifier correctly accepts. That is the false-negative case
//!    the fix removes, and it happens with probability `441 / 2^29` per block at `edge_bits = 29`, i.e. about 8.2e-7.
//!    So above the activation height the control is not "did legacy reject", it is "did *both* verifiers reject": a
//!    header no verifier accepts is a header no node could have stored, under either rule set, so the reconstruction is
//!    wrong. One such header is already proof - there is no threshold and no statistics here, and there must not be,
//!    because any threshold leaves a window for small scans. Without this control, scanning a range that lies entirely
//!    at or above the activation height with the wrong `<network>` would report every block as forged and exit 0.
use std::{env, path::PathBuf, process, str::FromStr};

use lmdb_zero::{Database, DatabaseOptions, ReadTransaction, db};
use tari_common::configuration::Network;
use tari_core::{
    chain_storage::create_readonly_lmdb_environment,
    proof_of_work::cuckaroo_pow::cuckaroo_audit_bipartite,
};
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::{
    consensus::consensus_constants::{
        ConsensusConstants,
        ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT,
        IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT,
        MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
        NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
        STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT,
    },
    tari_proof_of_work::PowAlgorithm,
};

/// The name of the LMDB database holding `height -> BlockHeader`. Mirrors `LMDB_DB_HEADERS`, which is private to
/// `chain_storage`.
const HEADERS_DB: &str = "headers";
/// Heights per read transaction. See property 2 in the module docs.
const CHUNK: u64 = 2_000;

/// The height from which this network validates c29 with the bipartite verifier (GHSA-3qmx-q9pv-f3m4). Below it any
/// legacy rejection is a control failure; at and above it the rate of them is. See property 3.
fn c29_bipartite_activation_height(network: Network) -> u64 {
    match network {
        // LocalNet carries the fix on its height-0 entry.
        Network::LocalNet => 0,
        Network::Igor => IGOR_C29_BIPARTITE_ACTIVATION_HEIGHT,
        Network::Esmeralda => ESMERALDA_C29_BIPARTITE_ACTIVATION_HEIGHT,
        Network::NextNet => NEXTNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
        Network::StageNet => STAGENET_C29_BIPARTITE_ACTIVATION_HEIGHT,
        Network::MainNet => MAINNET_C29_BIPARTITE_ACTIVATION_HEIGHT,
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("audit_c29: {err}");
        process::exit(1);
    }
}

const USAGE: &str = "usage: audit_c29 <network> <db-path> [start-height] [end-height]";

/// The parsed command line. `explicit_end` is kept because "the scan stopped below the end you asked for" is only
/// an error when an end height was actually asked for.
struct Args {
    network: Network,
    db_path: String,
    start: u64,
    end: u64,
    explicit_end: bool,
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = env::args().collect();
    let network_arg = argv.get(1).ok_or_else(|| USAGE.to_string())?;
    let db_path = argv.get(2).ok_or_else(|| USAGE.to_string())?.clone();

    // PROPERTY 1: this must happen before any header is hashed.
    let network = Network::from_str(network_arg).map_err(|e| format!("unknown network: {e}"))?;
    Network::set_current(network).map_err(|n| format!("network was already set to {n}"))?;

    let start: u64 = match argv.get(3) {
        Some(v) => v.parse().map_err(|_| "start-height is not a number".to_string())?,
        None => 0,
    };
    let end: u64 = match argv.get(4) {
        Some(v) => v.parse().map_err(|_| "end-height is not a number".to_string())?,
        None => u64::MAX,
    };
    if start > end {
        return Err(format!("start-height {start} is above end-height {end}"));
    }

    Ok(Args {
        network,
        db_path,
        start,
        end,
        explicit_end: argv.get(4).is_some(),
    })
}

fn run() -> Result<(), String> {
    let Args {
        network,
        db_path,
        start,
        end,
        explicit_end,
    } = parse_args()?;

    let env = create_readonly_lmdb_environment(PathBuf::from(db_path)).map_err(|e| e.to_string())?;
    let headers = Database::open(env.clone(), Some(HEADERS_DB), &DatabaseOptions::new(db::INTEGERKEY))
        .map_err(|e| format!("could not open the `{HEADERS_DB}` database: {e}"))?;

    let activation = c29_bipartite_activation_height(network);

    let mut counts = Counts::default();
    let mut scanned = 0u64;
    let mut height = start;
    let mut highest_seen = None;

    println!("height,timestamp");
    'chunks: loop {
        // PROPERTY 2: a fresh, short-lived reader per chunk.
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
                    // A gap. If nothing has been read yet this is not "past the tip", it is a `start` above the
                    // tip or a pruned/partial store - `report` turns that into a non-zero exit via `scanned == 0`.
                    break 'chunks;
                };
                let header: BlockHeader =
                    bincode::deserialize(bytes).map_err(|e| format!("could not decode header {height}: {e}"))?;
                scanned = scanned.saturating_add(1);
                highest_seen = Some(height);

                if header.pow_algo() == PowAlgorithm::Cuckaroo {
                    classify_c29(&header, network, activation, &mut counts);
                }
                height = height.saturating_add(1);
            }
        }
        drop(txn);
        if height > end {
            break;
        }
    }

    report(&Summary {
        start,
        end,
        explicit_end,
        activation,
        scanned,
        c29: counts.c29,
        c29_above_activation: counts.c29_above_activation,
        forged: counts.forged,
        control_failures_below_activation: counts.control_failures_below_activation,
        control_failures_above_activation: counts.control_failures_above_activation,
        legacy_false_negatives: counts.legacy_false_negatives,
        highest_seen,
    })
}

/// The per-header tallies the scan accumulates, so that the scan loop only moves counters and `report` only reads
/// them. Every field is explained on the matching field of `Summary`.
#[derive(Default)]
struct Counts {
    c29: u64,
    c29_above_activation: u64,
    forged: u64,
    control_failures_below_activation: u64,
    control_failures_above_activation: u64,
    legacy_false_negatives: u64,
}

/// Runs both Cuckaroo verifiers over one c29 header and files the result. This is PROPERTY 3.
fn classify_c29(header: &BlockHeader, network: Network, activation: u64, counts: &mut Counts) {
    counts.c29 = counts.c29.saturating_add(1);
    if header.height >= activation {
        counts.c29_above_activation = counts.c29_above_activation.saturating_add(1);
    }
    let constants = ConsensusConstants::for_network_at_height(network, header.height);
    let (legacy, bipartite) = cuckaroo_audit_bipartite(
        header,
        constants.cuckaroo_cycle_length(),
        constants.cuckaroo_edge_bits(),
    );
    // PROPERTY 3: the control. Which question it asks depends on which verifier the node used
    // for this height, and on that side of the fork the answer is exact - there is no threshold
    // here and there must not be one.
    //
    // Below the activation height the node validated with the legacy verifier, so it accepted
    // this header by definition: any legacy rejection means the header was reconstructed wrong.
    //
    // At and above the activation height the node validated with the *bipartite* verifier, so a
    // faithful reconstruction must see bipartite accept. The three reachable shapes are disjoint:
    //
    //   legacy Err, bipartite Ok    the U/V collision false negative. Legitimate and expected,
    //                               about 441 / 2^29 per block. Nothing to report but a note.
    //   legacy Err, bipartite Err   nobody could have accepted this header, under either rule
    //                               set, so it cannot be a block the node stored. The
    //                               reconstruction is wrong. One is already proof.
    //   legacy Ok,  bipartite Err   a merged-namespace proof, stored by a node that had not
    //                               upgraded when it validated this height. This is the thing
    //                               the tool exists to find.
    //
    // Either way the scan continues; aborting a multi-hour walk on one header, mid-incident, is
    // worse than reporting it and carrying on. The exit code at the end reflects any control
    // failure.
    let at = header.height;
    if at < activation {
        if let Err(e) = &legacy {
            counts.control_failures_below_activation = counts.control_failures_below_activation.saturating_add(1);
            eprintln!(
                "CONTROL FAILED at height {at}: the legacy verifier rejected a header the node accepted ({e}). The \
                 header was probably reconstructed incorrectly - check that the network argument matches the \
                 database. Results below this line are unreliable."
            );
        }
    } else {
        match (&legacy, &bipartite) {
            (Err(e), Ok(_)) => {
                counts.legacy_false_negatives = counts.legacy_false_negatives.saturating_add(1);
                eprintln!(
                    "note: at height {at} (at or above the activation height {activation}) the legacy verifier \
                     rejected ({e}) a header the node validated bipartitely. That is the expected U/V collision false \
                     negative and says nothing about the reconstruction; the bipartite verifier, which is the one \
                     that matters here, accepted it."
                );
            },
            (Err(e), Err(b)) => {
                counts.control_failures_above_activation = counts.control_failures_above_activation.saturating_add(1);
                eprintln!(
                    "CONTROL FAILED at height {at}: neither verifier accepts this header (legacy: {e}; bipartite: \
                     {b}), so no node could have stored it under either rule set. The header was reconstructed \
                     incorrectly - check that the network argument matches the database, that this build matches the \
                     one that wrote it, and that the store is not truncated. Results are unreliable."
                );
            },
            _ => {},
        }
    }

    // A forgery is precisely "the legacy verifier accepts it and the bipartite one does not". A
    // header both verifiers reject is not forged, it is unverifiable, and it is counted above.
    if legacy.is_ok() && bipartite.is_err() {
        counts.forged = counts.forged.saturating_add(1);
        println!("{},{}", header.height, header.timestamp.as_u64());
    }
}

/// Everything the scan learned, so that reporting and the "did this audit actually cover anything" checks live
/// outside the scan loop.
struct Summary {
    start: u64,
    end: u64,
    explicit_end: bool,
    activation: u64,
    scanned: u64,
    c29: u64,
    c29_above_activation: u64,
    forged: u64,
    /// Legacy rejections below the activation height. The node validated those heights with the legacy verifier,
    /// so a single one means the header was reconstructed wrong.
    control_failures_below_activation: u64,
    /// Headers at or above the activation height that *neither* verifier accepts. No node could have stored such
    /// a header under either rule set, so a single one means the header was reconstructed wrong.
    control_failures_above_activation: u64,
    /// Headers at or above the activation height that the legacy verifier rejects and the bipartite verifier
    /// accepts: the U/V collision false negative. Benign by construction - the verifier the node actually used
    /// accepted it - so this needs no control of its own.
    legacy_false_negatives: u64,
    highest_seen: Option<u64>,
}

/// Prints the summary and decides the exit status. An audit that covered nothing, covered less than it was asked
/// to, or failed its control must not exit 0 looking like a clean chain.
fn report(summary: &Summary) -> Result<(), String> {
    let Summary {
        start,
        end,
        explicit_end,
        activation,
        scanned,
        c29,
        c29_above_activation,
        forged,
        control_failures_below_activation,
        control_failures_above_activation,
        legacy_false_negatives,
        highest_seen,
    } = *summary;

    let legacy_accepted = c29
        .saturating_sub(control_failures_below_activation)
        .saturating_sub(control_failures_above_activation)
        .saturating_sub(legacy_false_negatives);
    eprintln!("scanned:                        {scanned}");
    eprintln!("cuckaroo (c29) blocks:          {c29}");
    eprintln!("  accepted by the legacy verifier: {legacy_accepted}");
    eprintln!("  rejected by the bipartite verifier: {forged}");
    eprintln!("  c29 blocks at or above the activation height {activation}: {c29_above_activation}");
    eprintln!("  U/V collision false negatives at or above the activation height: {legacy_false_negatives}");
    eprintln!("  CONTROL FAILURES below the activation height: {control_failures_below_activation}");
    eprintln!("  CONTROL FAILURES at or above it (neither verifier accepts): {control_failures_above_activation}");
    match highest_seen {
        Some(h) => eprintln!("highest height read:            {h}"),
        None => eprintln!("highest height read:            none - the range held no headers"),
    }

    if scanned == 0 {
        return Err(format!(
            "scanned no headers at all in [{start}, {end}]. The chain tip is below {start}, or this is not a base \
             node header store. Nothing was audited."
        ));
    }
    if let Some(h) = highest_seen.filter(|h| explicit_end && *h < end) {
        return Err(format!(
            "the scan stopped at height {h}, below the requested end-height {end}. The range was only partially \
             audited; heights {next}..={end} were not checked.",
            next = h.saturating_add(1)
        ));
    }
    if control_failures_below_activation > 0 {
        return Err(format!(
            "{control_failures_below_activation} control failures below the activation height: the reported forgery \
             count is not trustworthy"
        ));
    }
    if control_failures_above_activation > 0 {
        return Err(format!(
            "{control_failures_above_activation} of {c29_above_activation} c29 blocks at or above the activation \
             height {activation} are accepted by neither verifier, so no node could have stored them. The \
             reconstruction is wrong and the reported forgery count is not trustworthy."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    /// A scan that found nothing wrong: no legacy rejections on either side of the activation height.
    fn healthy() -> Summary {
        Summary {
            start: 0,
            end: u64::MAX,
            explicit_end: false,
            activation: 352_600,
            scanned: 400_000,
            c29: 90_000,
            c29_above_activation: 0,
            forged: 0,
            control_failures_below_activation: 0,
            control_failures_above_activation: 0,
            legacy_false_negatives: 0,
            highest_seen: Some(399_999),
        }
    }

    #[test]
    fn a_healthy_scan_succeeds() {
        assert!(report(&healthy()).is_ok());
    }

    #[test]
    fn a_scan_that_read_nothing_fails() {
        let summary = Summary {
            scanned: 0,
            c29: 0,
            highest_seen: None,
            start: 900_000,
            ..healthy()
        };
        let err = report(&summary).expect_err("an audit that covered nothing must not look clean");
        assert!(err.contains("scanned no headers at all"), "{err}");
    }

    #[test]
    fn a_scan_that_stopped_below_an_explicit_end_fails() {
        let summary = Summary {
            explicit_end: true,
            end: 500_000,
            highest_seen: Some(399_999),
            ..healthy()
        };
        let err = report(&summary).expect_err("a partially covered range must not look clean");
        assert!(err.contains("below the requested end-height"), "{err}");
    }

    #[test]
    fn a_control_failure_below_the_activation_height_fails() {
        let summary = Summary {
            control_failures_below_activation: 1,
            ..healthy()
        };
        let err = report(&summary).expect_err("a legacy rejection below the fork is a reconstruction error");
        assert!(err.contains("control failures below the activation height"), "{err}");
    }

    /// The hole the above-activation control exists to close: a range that lies entirely at or above the
    /// activation height, scanned with the wrong `<network>`. Every `mining_hash()` is wrong, so every c29 header
    /// is rejected by *both* verifiers. The below-activation control never runs.
    #[test]
    fn a_wrong_network_scan_entirely_above_the_activation_height_fails() {
        let summary = Summary {
            start: 903_000,
            scanned: 5_000,
            c29: 1_300,
            c29_above_activation: 1_300,
            forged: 0,
            control_failures_above_activation: 1_300,
            highest_seen: Some(907_999),
            ..healthy()
        };
        let err = report(&summary).expect_err("a header no verifier accepts is a broken reconstruction");
        assert!(err.contains("accepted by neither verifier"), "{err}");
    }

    /// Regression test for the window a counting threshold leaves behind. `audit_c29 mainnet <db> 352990` with the
    /// wrong network, against a tip of 353,000, covers 11 heights and about 3 c29 blocks. Under a
    /// "at least N occurrences" rule with N = 4 this exited 0. The exact discriminator has no such window: one
    /// header that neither verifier accepts is already proof.
    #[test]
    fn a_tiny_wrong_network_scan_of_three_blocks_fails() {
        for blocks in 1..=3u64 {
            let summary = Summary {
                start: 352_990,
                end: 353_000,
                explicit_end: true,
                scanned: 11,
                c29: blocks,
                c29_above_activation: blocks,
                forged: 0,
                control_failures_above_activation: blocks,
                highest_seen: Some(353_000),
                ..healthy()
            };
            let err = report(&summary).expect_err("a scan of unverifiable headers must not exit 0");
            assert!(err.contains("accepted by neither verifier"), "{blocks} blocks: {err}");
        }
    }

    /// Genuine U/V collision false negatives must never fail the scan, at any count. They are benign by
    /// construction: the bipartite verifier - the one the node actually used at these heights - accepted them.
    /// At `441 / 2^29` per block even one in a 40,000 block scan is already an outlier.
    #[test]
    fn genuine_u_v_collisions_never_fail_the_scan() {
        let one_in_forty_thousand = Summary {
            c29_above_activation: 40_000,
            legacy_false_negatives: 1,
            forged: 0,
            ..healthy()
        };
        assert!(report(&one_in_forty_thousand).is_ok());

        // Even an absurd number of them is not a control failure, because the discriminator is the *pair* of
        // verdicts, not a count. Nothing here says "neither verifier accepted it".
        let absurd = Summary {
            c29_above_activation: 1_300,
            legacy_false_negatives: 1_300,
            ..healthy()
        };
        assert!(report(&absurd).is_ok());
    }

    /// The two above-activation shapes are independent: collisions alone pass, and a single unverifiable header
    /// alongside any number of them still fails.
    #[test]
    fn one_unverifiable_header_fails_even_among_collisions() {
        let summary = Summary {
            c29_above_activation: 40_000,
            legacy_false_negatives: 500,
            control_failures_above_activation: 1,
            ..healthy()
        };
        let err = report(&summary).expect_err("one header no verifier accepts is already proof");
        assert!(err.contains("accepted by neither verifier"), "{err}");
    }
}
