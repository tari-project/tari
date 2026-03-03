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
use primitive_types::U512;
use tari_common_types::types::FixedHash;
use tari_metrics::{Gauge, IntCounter, IntCounterVec, IntGauge, IntGaugeVec};
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

pub fn target_difficulty_monero_randomx() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::target_difficulty_monero",
            "The current miner target difficulty for the monero PoW algo",
        )
        .unwrap()
    });

    &METER
}
pub fn target_difficulty_tari_randomx() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::target_difficulty_tari_rx",
            "The current miner target difficulty for the tari rx PoW algo",
        )
        .unwrap()
    });

    &METER
}

pub fn target_difficulty_cuckaroo() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::target_difficulty_cuckaroo",
            "The current miner target difficulty for the cuckaroo PoW algo",
        )
        .unwrap()
    });

    &METER
}

/// The target difficulty at the given height
pub fn target_difficulty() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge("base_node::blockchain::target_diff", "target_difficulty at height").unwrap()
    });

    &METER
}

/// The accumulated difficulty indicator (log_2 scale) at the given height
pub fn accumulated_difficulty_indicator() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::acc_diff_indicator",
            "log2(accumulated_difficulty at height) * 1000",
        )
        .unwrap()
    });

    &METER
}

/// The target difficulty indicator (log_2 scale) at the given height
pub fn target_difficulty_indicator() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::target_diff_indicator",
            "log2(target_difficulty at height) * 1000",
        )
        .unwrap()
    });

    &METER
}

/// The block height associated with the current difficulty indicators
pub fn difficulty_indicator_height() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::diff_indicator_height",
            "block height associated with difficulty indicators",
        )
        .unwrap()
    });
    &METER
}

/// floor(log2(total_accumulated_difficulty)) at height [reconstruction: (acc_diff_sig53 / 2^52) * 2^acc_diff_exp2]
pub fn accumulated_difficulty_exp2() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::acc_diff_exp2",
            "floor(log2(total_accumulated_difficulty)) at height [reconstruction: (acc_diff_sig53 / 2^52) * \
             2^acc_diff_exp2]",
        )
        .unwrap()
    });
    &METER
}

/// Top 53 bits of total_accumulated_difficulty at height [reconstruction: (acc_diff_sig53 / 2^52) * 2^acc_diff_exp2]
pub fn accumulated_difficulty_sig53() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "base_node::blockchain::acc_diff_sig53",
            "Top 53 bits of total_accumulated_difficulty at height [reconstruction: (acc_diff_sig53 / 2^52) * \
             2^acc_diff_exp2]",
        )
        .unwrap()
    });
    &METER
}

/// Approximate total_accumulated_difficulty at height as an f64 [reconstruction: (acc_diff_sig53 / 2^52) *
/// 2^acc_diff_exp2] Note: This is less accurate than doing the computation directly in the client, but is provided for
/// convenience.
pub fn accumulated_difficulty_as_f64() -> &'static Gauge {
    static METER: Lazy<Gauge> = Lazy::new(|| {
        tari_metrics::register_gauge(
            "base_node::blockchain::acc_diff_as_f64",
            "Approximate total_accumulated_difficulty at height as an f64 [approximation: (acc_diff_sig53 / 2^52) * \
             2^acc_diff_exp2]",
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

// Ref: IEEE 754-2008, binary64 format.
// f64 has a 53-bit significand (52 explicit + 1 implicit bit).
// Scaling by 1/2^52 ensures we map the 53-bit integer into [1, 2),
// which is exactly representable in f64 without rounding.
const INV_2P52: f64 = 1.0 / ((1u64 << 52) as f64);

/// Computes log₂(value) as f64 for a U512 using a 53-bit normalized significand.
/// Uses IEEE 754 binary64 rules to avoid precision loss:
/// - Exact powers of two return an integer result.
/// - Non-powers-of-two are guaranteed to be strictly less than the next integer, preventing rounding-up artifacts.
#[allow(clippy::cast_possible_truncation)]
pub fn log2_u512(value_u512: &U512) -> Option<f64> {
    if value_u512.is_zero() {
        return None;
    }

    let (total_bits, sig53) = u512_into_parts(value_u512);

    // Converts the 53-bit integer significand into a floating-point number in the range [1, 2)
    let x = (sig53 as f64) * INV_2P52;

    let frac = x.log2();
    let mut res = (f64::from(total_bits) - 1.0) + frac;

    // If not an exact power of two (sig53 > 2^52), ensure we never round up to the next integer.
    if sig53 > (1u64 << 52) {
        res = next_down(res);
    }
    Some(res)
}

// Returns (exp2, sig53) where:
//   - exp2 = floor(log2(value)) (u32)
//   - sig53 = top 53 bits (u64)
#[allow(clippy::cast_possible_truncation)]
fn u512_into_parts(value_u512: &U512) -> (u32, u64) {
    let total_bits: u32 = value_u512.bits() as u32; // total bits

    // Build exact 53-bit significand with MSB at bit 52 into u64
    let sig53: u64 = if total_bits > 53 {
        // Keep only the top 53 bits
        (value_u512 >> (total_bits - 53)).as_u64()
    } else {
        // Move the most significant bit to position 52
        (value_u512 << (53 - total_bits)).as_u64()
    };
    debug_assert!(((1u64 << 52)..(1u64 << 53)).contains(&sig53));
    (total_bits, sig53)
}

/// Returns (exp2, sig53) where:
///   - exp2 = floor(log2(value)) (i64)
///   - sig53 = top 53 bits (i64, always positive, safe up to 2^53-1)
///   - Returns None if value == 0.
#[allow(clippy::cast_possible_truncation)]
pub fn u512_exp2_sig53(value: &U512) -> Option<(i64, i64)> {
    if value.is_zero() {
        return None;
    }
    let (total_bits, sig53) = u512_into_parts(value);
    Some((i64::from(total_bits) - 1, i64::try_from(sig53).unwrap_or(i64::MAX)))
}

/// Approximate a U512 as f64 using a 53-bit significand and exponent as `(sig53 / 2^52) * 2^exp2`.
///   - exp2 = floor(log2(value)) (i64)
///   - sig53 = top 53 bits (i64, always positive, safe up to 2^53-1)
///   - Returns None if value == 0.
#[allow(clippy::cast_possible_truncation)]
pub fn approximate_u512_with_f64(value: &U512) -> Option<f64> {
    if value.is_zero() {
        return None;
    }
    let (exp2, sig53) = u512_exp2_sig53(value).unwrap();

    // Grafana-style reconstruction: (sig53 / 2^52) * 2^exp2
    // This is not exact (limited by f64), but should be within a tiny relative error.
    const TWO_P52: f64 = 4503599627370496.0; // 2^52
    // Build 2^exp2 by setting the exponent (bias 1023), mantissa 0
    let two_pow_exp2 = f64::from_bits(((exp2 + 1023) as u64) << 52);
    Some((sig53 as f64 / TWO_P52) * two_pow_exp2)
}

// Nudge one floating point unit (ULP) down to counter rounding-up at integer boundaries.
#[inline]
fn next_down(x: f64) -> f64 {
    // Move to the next representable float toward -∞
    f64::from_bits(x.to_bits().saturating_sub(1))
}

/// Computes log₂(value) as f64 for a u128 using a 53-bit normalized significand.
/// Uses IEEE 754 binary64 rules to avoid precision loss:
/// - Exact powers of two return an integer result.
/// - Non-powers-of-two are guaranteed to be strictly less than the next integer, preventing rounding-up artifacts.
#[inline]
#[allow(clippy::cast_possible_truncation)]
pub fn log2_u128(value_u128: u128) -> Option<f64> {
    if value_u128 == 0 {
        return None;
    }
    let total_bits: u32 = 128 - value_u128.leading_zeros(); // In the range 1..=128

    // Build exact 53-bit significand with MSB at bit 52 into u64
    let sig53: u64 = if total_bits > 53 {
        // Keep only the top 53 bits
        (value_u128 >> (total_bits - 53)) as u64
    } else {
        // Move the most significant bit to position 52
        (value_u128 << (53 - total_bits)) as u64
    };
    debug_assert!(((1u64 << 52)..(1u64 << 53)).contains(&sig53));

    // Converts the 53-bit integer significand into a floating-point number in the range [1, 2)
    let x = (sig53 as f64) * INV_2P52;

    let frac = x.log2();
    let mut res = (f64::from(total_bits) - 1.0) + frac;

    // If not an exact power of two (sig53 > 2^52), ensure we never round up to the next integer.
    if sig53 > (1u64 << 52) {
        res = next_down(res);
    }
    Some(res)
}

/// Converts a floating point number of bits into an integer number of milli-bits (1/1000 bits).
#[inline]
#[allow(clippy::cast_possible_truncation)]
pub fn milli_bits(x: f64) -> i64 {
    (x * 1000.0).round() as i64
}

#[cfg(test)]
mod tests {
    use primitive_types::U512;

    use super::*;

    #[test]
    fn test_log2_u512() {
        // log2(1) == 0
        assert_eq!(log2_u512(&U512::from(1u64)), Some(0.0));

        // Exact powers of two
        assert_eq!(log2_u512(&(U512::from(2u64))), Some(1.0));
        assert_eq!(log2_u512(&(U512::from(8u64))), Some(3.0));
        assert_eq!(log2_u512(&(U512::from(1024u64))), Some(10.0));

        // 2^200 exactly
        let value = U512::from(1u64) << 200;
        assert_eq!(log2_u512(&value), Some(200.0));

        // 2^200 - 1  => strictly less than 200
        let value = (U512::from(1u64) << 200) - U512::from(1u64);
        assert!(log2_u512(&value).unwrap() < 200.0);

        // u128::MAX = 2^128 - 1  => strictly less than 128
        assert!(log2_u512(&U512::from(u128::MAX)).unwrap() < 128.0);

        // Test U512 max value = 2^512 - 1  => strictly less than 512
        let u512_max = U512::MAX;
        let log2_u512_max = log2_u512(&u512_max).unwrap();
        assert!(log2_u512_max < 512.0);
        assert!(log2_u512_max > 511.0);

        // log2(0) == None
        assert!(log2_u512(&U512::from(0u64)).is_none());
    }

    #[test]
    fn test_log2_u128() {
        // log2(1) == 0
        assert_eq!(log2_u128(1), Some(0.0));

        // exact powers of two
        assert_eq!(log2_u128(2), Some(1.0));
        assert_eq!(log2_u128(8), Some(3.0));
        assert_eq!(log2_u128(1024), Some(10.0));

        // 2^100 exactly
        let value = 1u128 << 100;
        assert_eq!(log2_u128(value), Some(100.0));

        // 2^100 - 1  => strictly less than 100
        let value = (1u128 << 100) - 1;
        assert!(log2_u128(value).unwrap() < 100.0);

        // u128::MAX = 2^128 - 1  => strictly less than 128
        assert!(log2_u128(u128::MAX).unwrap() < 128.0);

        // log2(0) == None
        assert!(log2_u128(0).is_none());
    }

    #[test]
    fn millibit_correctly_handles_small_difficulty_growth() {
        // Accumulated difficulty (U512)
        let acc_diff_v1 = U512::from_dec_str("3872628503165662556508806093911347954645375156922").unwrap();
        // Target difficulty (fits in u128)
        let target_diff_v1: u128 = 33_208_643_413_617_919;

        // --- Baseline milli-bits ---
        let acc_diff_v1_mbits = milli_bits(log2_u512(&acc_diff_v1).unwrap());
        let target_diff_v1_mbits = milli_bits(log2_u128(target_diff_v1).unwrap());

        // --- Apply +0.15% change (δ = 0.0015) --- (1/0.0015 = 666.666666667 ~= 667)
        let acc_diff_v2 = acc_diff_v1 + acc_diff_v1 / U512::from(667u64);
        let target_diff_v2 = target_diff_v1 + (target_diff_v1 / 667);

        let acc_diff_v2_mbits = milli_bits(log2_u512(&acc_diff_v2).unwrap());
        let target_diff_v2_mbits = milli_bits(log2_u128(target_diff_v2).unwrap());

        assert!(acc_diff_v2_mbits > acc_diff_v1_mbits);
        assert!(target_diff_v2_mbits > target_diff_v1_mbits);
    }

    #[test]
    fn exp2_sig53_power_of_two_and_neighbors() {
        // 2^200
        let v = U512::from(1u64) << 200;
        let (e, s) = u512_exp2_sig53(&v).unwrap();
        assert_eq!(e, 200);
        assert_eq!(s, 1i64 << 52, "exact power of two => normalized sig53 == 2^52");

        // 2^200 - 1  (just below)
        let v = (U512::from(1u64) << 200) - U512::from(1u64);
        let (e, s) = u512_exp2_sig53(&v).unwrap();
        assert_eq!(e, 199, "floor(log2(2^200-1)) = 199");
        assert!(((1i64 << 52)..(1i64 << 53)).contains(&s));

        // 2^200 + 1  (just above)
        let v = (U512::from(1u64) << 200) + U512::from(1u64);
        let (e, s) = u512_exp2_sig53(&v).unwrap();
        assert_eq!(e, 200);
        assert!(((1i64 << 52)..(1i64 << 53)).contains(&s));
    }

    #[test]
    fn it_can_reconstruct_u512_from_exp2_sig53_parts_as_f64() {
        use primitive_types::U512;

        let original_u512 = U512::from_dec_str("3872628503165662556508806093911347954645375156922").unwrap();
        let approximate_u512 = approximate_u512_with_f64(&original_u512).unwrap();

        // Expected bits using our robust log2 (with next_down ULP guard)
        let expected_bits = log2_u512(&original_u512).unwrap();

        // The two log2 values should match within a tiny epsilon (a few ULPs)
        let approx_bits = approximate_u512.log2();
        assert!(
            (approx_bits - expected_bits).abs() < 1e-9,
            "reconstructed log2 mismatch: approx={}, expected={}",
            approx_bits,
            expected_bits
        );

        // Relative error via logs (safer for huge values):
        let relative_err = (approximate_u512.log2() - log2_u512(&original_u512).unwrap()).abs() /
            log2_u512(&original_u512).unwrap().abs();
        assert!(relative_err < 1e-12);

        // println!("approx f64 = {}\noriginal   = {}", reconstructed_u512, original_u512);
    }
}
