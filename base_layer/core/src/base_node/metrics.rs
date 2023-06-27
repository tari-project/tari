//  Copyright 2022, The Tari Project
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

use once_cell::sync::Lazy;
use tari_common_types::types::FixedHash;
use tari_metrics::{IntCounter, IntCounterVec, IntGauge, IntGaugeVec};
use tari_utilities::hex::Hex;

pub fn tip_height() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge("base_node::blockchain::tip_height", "The current tip height").unwrap()
    });

    &METER
}

pub fn target_difficulty_sha() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::target_difficulty_sha",
            "The current miner target difficulty for the sha3 PoW algo",
        )
        .unwrap()
    });

    &METER
}

pub fn target_difficulty_randomx() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::target_difficulty_monero",
            "The current miner target difficulty for the monero PoW algo",
        )
        .unwrap()
    });

    &METER
}

pub fn reorg(fork_height: u64, num_added: usize, num_removed: usize) -> IntGauge {
    static METER: Lazy<IntGaugeVec> = Lazy::new(|| {
        tari_metrics::register_int_gauge_vec("base_node::blockchain::reorgs", "Reorg stats", &[
            "fork_height",
            "num_added",
            "num_removed",
        ])
        .unwrap()
    });

    METER.with_label_values(&[
        &fork_height.to_string(),
        &num_added.to_string(),
        &num_removed.to_string(),
    ])
}

pub fn compact_block_tx_misses(height: u64) -> IntGauge {
    static METER: Lazy<IntGaugeVec> = Lazy::new(|| {
        tari_metrics::register_int_gauge_vec(
            "base_node::blockchain::compact_block_unknown_transactions",
            "Number of unknown transactions from the incoming compact block",
            &["height"],
        )
        .unwrap()
    });

    METER.with_label_values(&[&height.to_string()])
}

pub fn compact_block_full_misses(height: u64) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "base_node::blockchain::compact_block_miss",
            "Number of full blocks that had to be requested",
            &["height"],
        )
        .unwrap()
    });

    METER.with_label_values(&[&height.to_string()])
}

pub fn compact_block_mmr_mismatch(height: u64) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "base_node::blockchain::compact_block_mmr_mismatch",
            "Number of full blocks that had to be requested because of MMR mismatch",
            &["height"],
        )
        .unwrap()
    });

    METER.with_label_values(&[&height.to_string()])
}

pub fn orphaned_blocks() -> IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "base_node::blockchain::orphaned_blocks",
            "Number of valid orphan blocks accepted by the base node",
        )
        .unwrap()
    });

    METER.clone()
}

pub fn rejected_blocks(height: u64, hash: &FixedHash) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "base_node::blockchain::rejected_blocks",
            "Number of block rejected by the base node",
            &["height", "block_hash"],
        )
        .unwrap()
    });

    METER.with_label_values(&[&height.to_string(), &hash.to_hex()])
}

pub fn rejected_local_blocks(height: u64, hash: &FixedHash) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "base_node::blockchain::rejected_local_blocks",
            "Number of local block rejected by the base node",
            &["height", "block_hash"],
        )
        .unwrap()
    });

    METER.with_label_values(&[&height.to_string(), &hash.to_hex()])
}

pub fn active_sync_peers() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::sync::active_peers",
            "Number of active peers syncing from this node",
        )
        .unwrap()
    });

    &METER
}

pub fn utxo_set_size() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::utxo_set_size",
            "The number of UTXOs in the current UTXO set",
        )
        .unwrap()
    });

    &METER
}
