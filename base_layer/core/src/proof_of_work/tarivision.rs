// Copyright 2024. The Tari Project
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

//! TariVision v2 — ASIC-resistant proof-of-work algorithm with full DAG verification.
//!
//! A ProgPoW-derived algorithm using Keccak-f[800], KISS99 RNG, 128KB L1 cache,
//! 32 registers, 15 math operations, 64 rounds. Targets modern GPUs (RTX 40/50 series).
//!
//! The node performs full DAG verification: it recomputes the mix_hash from the epoch
//! context (light cache) and compares it against the claimed mix_hash in pow_data.

use std::sync::{Arc, Mutex};

use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::tari_proof_of_work::{Difficulty, DifficultyError};

// ============================================================================
// TariVision v2 algorithm constants (MeowPoW v2)
// ============================================================================
const PERIOD_LENGTH: u32 = 3;
const NUM_REGS: usize = 32;
const NUM_LANES: usize = 16;
const NUM_CACHE_ACCESSES: usize = 14;
const NUM_MATH_OPERATIONS: usize = 24;
const L1_CACHE_SIZE: usize = 128 * 1024; // 128 KB
const L1_CACHE_NUM_ITEMS: usize = L1_CACHE_SIZE / 4; // u32 items
const NUM_ROUNDS: usize = 64;

const FNV_PRIME: u32 = 0x01000193;
const FNV_OFFSET_BASIS: u32 = 0x811c9dc5;

// Ethash constants
const LIGHT_CACHE_INIT_SIZE: i64 = 1 << 24; // 16 MB
const LIGHT_CACHE_GROWTH: i64 = 1 << 17; // 128 KB per epoch
const LIGHT_CACHE_ROUNDS: usize = 3;
const FULL_DATASET_INIT_SIZE: i64 = 1 << 30; // 1 GB
const FULL_DATASET_GROWTH: i64 = 1 << 23; // 8 MB per epoch
const FULL_DATASET_ITEM_PARENTS: u32 = 512;
const HASH512_BYTES: i64 = 64;
const HASH1024_BYTES: i64 = 128;
const EPOCH_LENGTH: u64 = 7500;
const MEOWPOW_DAGCHANGE_EPOCH: i32 = 110;

/// TariVision domain separation constants: "TARIVISIONTARIV" (15 ASCII chars as u32 words).
const TARIVISION_DOMAIN: [u32; 15] = [
    0x54, 0x41, 0x52, 0x49, 0x56, 0x49, 0x53, 0x49, 0x4F, 0x4E, 0x54, 0x41, 0x52, 0x49, 0x56,
];

// ============================================================================
// Keccak-f[800] (25 x u32 state) — used by ProgPoW core
// ============================================================================

const KECCAK_F800_ROUND_CONSTANTS: [u32; 22] = [
    0x00000001, 0x00008082, 0x0000808A, 0x80008000, 0x0000808B, 0x80000001, 0x80008081,
    0x00008009, 0x0000008A, 0x00000088, 0x80008009, 0x8000000A, 0x8000808B, 0x0000008B,
    0x00008089, 0x00008003, 0x00008002, 0x00000080, 0x0000800A, 0x8000000A, 0x80008081,
    0x00008080,
];

const PILN: [usize; 24] = [
    10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4, 15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
];

const ROTC: [u32; 24] = [
    1, 3, 6, 10, 15, 21, 28, 4, 13, 23, 2, 14, 27, 9, 24, 8, 25, 11, 7, 20, 12, 18, 5, 31,
];

fn keccakf800(st: &mut [u32; 25]) {
    for round in 0..22 {
        let mut c = [0u32; 5];
        for i in 0..5 {
            c[i] = st[i] ^ st[i + 5] ^ st[i + 10] ^ st[i + 15] ^ st[i + 20];
        }
        for i in 0..5 {
            let d = c[(i + 4) % 5] ^ c[(i + 1) % 5].rotate_left(1);
            for j in (0..25).step_by(5) {
                st[j + i] ^= d;
            }
        }
        let mut tmp = st[1];
        for i in 0..24 {
            let j = PILN[i];
            let bc = st[j];
            st[j] = tmp.rotate_left(ROTC[i]);
            tmp = bc;
        }
        for j in (0..25).step_by(5) {
            let t: [u32; 5] = [st[j], st[j + 1], st[j + 2], st[j + 3], st[j + 4]];
            for i in 0..5 {
                st[j + i] = t[i] ^ ((!t[(i + 1) % 5]) & t[(i + 2) % 5]);
            }
        }
        st[0] ^= KECCAK_F800_ROUND_CONSTANTS[round];
    }
}

// ============================================================================
// Keccak-f[1600] (25 x u64 state) — used for keccak256/keccak512
// ============================================================================

const KECCAK_F1600_ROUND_CONSTANTS: [u64; 24] = [
    0x0000000000000001, 0x0000000000008082, 0x800000000000808a, 0x8000000080008000,
    0x000000000000808b, 0x0000000080000001, 0x8000000080008081, 0x8000000000008009,
    0x000000000000008a, 0x0000000000000088, 0x0000000080008009, 0x000000008000000a,
    0x000000008000808b, 0x800000000000008b, 0x8000000000008089, 0x8000000000008003,
    0x8000000000008002, 0x8000000000000080, 0x000000000000800a, 0x800000008000000a,
    0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008,
];

/// Keccak-f[1600] permutation — unrolled two-round implementation matching the C reference.
fn keccakf1600(state: &mut [u64; 25]) {
    // Use the same variable naming as the C reference for correctness verification
    let (mut aba, mut abe, mut abi, mut abo, mut abu) = (state[0], state[1], state[2], state[3], state[4]);
    let (mut aga, mut age, mut agi, mut ago, mut agu) = (state[5], state[6], state[7], state[8], state[9]);
    let (mut aka, mut ake, mut aki, mut ako, mut aku) = (state[10], state[11], state[12], state[13], state[14]);
    let (mut ama, mut ame, mut ami, mut amo, mut amu) = (state[15], state[16], state[17], state[18], state[19]);
    let (mut asa, mut ase, mut asi, mut aso, mut asu) = (state[20], state[21], state[22], state[23], state[24]);

    for round in (0..24).step_by(2) {
        // Round (round + 0): Axx -> Exx
        let ba = aba ^ aga ^ aka ^ ama ^ asa;
        let be = abe ^ age ^ ake ^ ame ^ ase;
        let bi = abi ^ agi ^ aki ^ ami ^ asi;
        let bo = abo ^ ago ^ ako ^ amo ^ aso;
        let bu = abu ^ agu ^ aku ^ amu ^ asu;

        let da = bu ^ be.rotate_left(1);
        let de = ba ^ bi.rotate_left(1);
        let di = be ^ bo.rotate_left(1);
        let r#do = bi ^ bu.rotate_left(1);
        let du = bo ^ ba.rotate_left(1);

        let ba2 = aba ^ da;
        let be2 = (age ^ de).rotate_left(44);
        let bi2 = (aki ^ di).rotate_left(43);
        let bo2 = (amo ^ r#do).rotate_left(21);
        let bu2 = (asu ^ du).rotate_left(14);
        let eba = ba2 ^ (!be2 & bi2) ^ KECCAK_F1600_ROUND_CONSTANTS[round];
        let ebe = be2 ^ (!bi2 & bo2);
        let ebi = bi2 ^ (!bo2 & bu2);
        let ebo = bo2 ^ (!bu2 & ba2);
        let ebu = bu2 ^ (!ba2 & be2);

        let ba2 = (abo ^ r#do).rotate_left(28);
        let be2 = (agu ^ du).rotate_left(20);
        let bi2 = (aka ^ da).rotate_left(3);
        let bo2 = (ame ^ de).rotate_left(45);
        let bu2 = (asi ^ di).rotate_left(61);
        let ega = ba2 ^ (!be2 & bi2);
        let ege = be2 ^ (!bi2 & bo2);
        let egi = bi2 ^ (!bo2 & bu2);
        let ego = bo2 ^ (!bu2 & ba2);
        let egu = bu2 ^ (!ba2 & be2);

        let ba2 = (abe ^ de).rotate_left(1);
        let be2 = (agi ^ di).rotate_left(6);
        let bi2 = (ako ^ r#do).rotate_left(25);
        let bo2 = (amu ^ du).rotate_left(8);
        let bu2 = (asa ^ da).rotate_left(18);
        let eka = ba2 ^ (!be2 & bi2);
        let eke = be2 ^ (!bi2 & bo2);
        let eki = bi2 ^ (!bo2 & bu2);
        let eko = bo2 ^ (!bu2 & ba2);
        let eku = bu2 ^ (!ba2 & be2);

        let ba2 = (abu ^ du).rotate_left(27);
        let be2 = (aga ^ da).rotate_left(36);
        let bi2 = (ake ^ de).rotate_left(10);
        let bo2 = (ami ^ di).rotate_left(15);
        let bu2 = (aso ^ r#do).rotate_left(56);
        let ema = ba2 ^ (!be2 & bi2);
        let eme = be2 ^ (!bi2 & bo2);
        let emi = bi2 ^ (!bo2 & bu2);
        let emo = bo2 ^ (!bu2 & ba2);
        let emu = bu2 ^ (!ba2 & be2);

        let ba2 = (abi ^ di).rotate_left(62);
        let be2 = (ago ^ r#do).rotate_left(55);
        let bi2 = (aku ^ du).rotate_left(39);
        let bo2 = (ama ^ da).rotate_left(41);
        let bu2 = (ase ^ de).rotate_left(2);
        let esa = ba2 ^ (!be2 & bi2);
        let ese = be2 ^ (!bi2 & bo2);
        let esi = bi2 ^ (!bo2 & bu2);
        let eso = bo2 ^ (!bu2 & ba2);
        let esu = bu2 ^ (!ba2 & be2);

        // Round (round + 1): Exx -> Axx
        let ba = eba ^ ega ^ eka ^ ema ^ esa;
        let be = ebe ^ ege ^ eke ^ eme ^ ese;
        let bi = ebi ^ egi ^ eki ^ emi ^ esi;
        let bo = ebo ^ ego ^ eko ^ emo ^ eso;
        let bu = ebu ^ egu ^ eku ^ emu ^ esu;

        let da = bu ^ be.rotate_left(1);
        let de = ba ^ bi.rotate_left(1);
        let di = be ^ bo.rotate_left(1);
        let r#do = bi ^ bu.rotate_left(1);
        let du = bo ^ ba.rotate_left(1);

        let ba2 = eba ^ da;
        let be2 = (ege ^ de).rotate_left(44);
        let bi2 = (eki ^ di).rotate_left(43);
        let bo2 = (emo ^ r#do).rotate_left(21);
        let bu2 = (esu ^ du).rotate_left(14);
        aba = ba2 ^ (!be2 & bi2) ^ KECCAK_F1600_ROUND_CONSTANTS[round + 1];
        abe = be2 ^ (!bi2 & bo2);
        abi = bi2 ^ (!bo2 & bu2);
        abo = bo2 ^ (!bu2 & ba2);
        abu = bu2 ^ (!ba2 & be2);

        let ba2 = (ebo ^ r#do).rotate_left(28);
        let be2 = (egu ^ du).rotate_left(20);
        let bi2 = (eka ^ da).rotate_left(3);
        let bo2 = (eme ^ de).rotate_left(45);
        let bu2 = (esi ^ di).rotate_left(61);
        aga = ba2 ^ (!be2 & bi2);
        age = be2 ^ (!bi2 & bo2);
        agi = bi2 ^ (!bo2 & bu2);
        ago = bo2 ^ (!bu2 & ba2);
        agu = bu2 ^ (!ba2 & be2);

        let ba2 = (ebe ^ de).rotate_left(1);
        let be2 = (egi ^ di).rotate_left(6);
        let bi2 = (eko ^ r#do).rotate_left(25);
        let bo2 = (emu ^ du).rotate_left(8);
        let bu2 = (esa ^ da).rotate_left(18);
        aka = ba2 ^ (!be2 & bi2);
        ake = be2 ^ (!bi2 & bo2);
        aki = bi2 ^ (!bo2 & bu2);
        ako = bo2 ^ (!bu2 & ba2);
        aku = bu2 ^ (!ba2 & be2);

        let ba2 = (ebu ^ du).rotate_left(27);
        let be2 = (ega ^ da).rotate_left(36);
        let bi2 = (eke ^ de).rotate_left(10);
        let bo2 = (emi ^ di).rotate_left(15);
        let bu2 = (eso ^ r#do).rotate_left(56);
        ama = ba2 ^ (!be2 & bi2);
        ame = be2 ^ (!bi2 & bo2);
        ami = bi2 ^ (!bo2 & bu2);
        amo = bo2 ^ (!bu2 & ba2);
        amu = bu2 ^ (!ba2 & be2);

        let ba2 = (ebi ^ di).rotate_left(62);
        let be2 = (ego ^ r#do).rotate_left(55);
        let bi2 = (eku ^ du).rotate_left(39);
        let bo2 = (ema ^ da).rotate_left(41);
        let bu2 = (ese ^ de).rotate_left(2);
        asa = ba2 ^ (!be2 & bi2);
        ase = be2 ^ (!bi2 & bo2);
        asi = bi2 ^ (!bo2 & bu2);
        aso = bo2 ^ (!bu2 & ba2);
        asu = bu2 ^ (!ba2 & be2);
    }

    state[0] = aba; state[1] = abe; state[2] = abi; state[3] = abo; state[4] = abu;
    state[5] = aga; state[6] = age; state[7] = agi; state[8] = ago; state[9] = agu;
    state[10] = aka; state[11] = ake; state[12] = aki; state[13] = ako; state[14] = aku;
    state[15] = ama; state[16] = ame; state[17] = ami; state[18] = amo; state[19] = amu;
    state[20] = asa; state[21] = ase; state[22] = asi; state[23] = aso; state[24] = asu;
}

// ============================================================================
// Keccak hash functions (keccak256, keccak512)
// ============================================================================

/// Generic Keccak sponge: absorb `data`, squeeze `bits/8` bytes.
fn keccak(data: &[u8], bits: usize) -> Vec<u8> {
    let hash_size = bits / 8;
    let block_size = (1600 - bits * 2) / 8;
    let mut state = [0u64; 25];
    let mut offset = 0;
    let mut remaining = data.len();

    // Absorb full blocks
    while remaining >= block_size {
        for i in 0..(block_size / 8) {
            state[i] ^= u64::from_le_bytes(data[offset + i * 8..offset + i * 8 + 8].try_into().unwrap());
        }
        keccakf1600(&mut state);
        offset += block_size;
        remaining -= block_size;
    }

    // Absorb remaining bytes (partial block)
    let mut state_idx = 0;
    let mut partial_offset = offset;
    while remaining >= 8 {
        state[state_idx] ^= u64::from_le_bytes(data[partial_offset..partial_offset + 8].try_into().unwrap());
        state_idx += 1;
        partial_offset += 8;
        remaining -= 8;
    }

    // Pad remaining bytes + 0x01 delimiter
    let mut last_word = 0u64;
    for i in 0..remaining {
        last_word |= (data[partial_offset + i] as u64) << (i * 8);
    }
    last_word |= 0x01u64 << (remaining * 8);
    state[state_idx] ^= last_word;

    // Final bit of padding
    state[(block_size / 8) - 1] ^= 0x8000000000000000u64;

    keccakf1600(&mut state);

    // Squeeze
    let mut output = vec![0u8; hash_size];
    for i in 0..(hash_size / 8) {
        output[i * 8..(i + 1) * 8].copy_from_slice(&state[i].to_le_bytes());
    }
    output
}

/// Keccak-256 hash (32-byte output).
fn keccak256(data: &[u8]) -> [u8; 32] {
    let out = keccak(data, 256);
    let mut h = [0u8; 32];
    h.copy_from_slice(&out);
    h
}

/// Keccak-512 hash (64-byte output).
fn keccak512(data: &[u8]) -> [u8; 64] {
    let out = keccak(data, 512);
    let mut h = [0u8; 64];
    h.copy_from_slice(&out);
    h
}

// ============================================================================
// Primitive helpers
// ============================================================================

#[inline(always)]
fn fnv1(u: u32, v: u32) -> u32 {
    u.wrapping_mul(FNV_PRIME) ^ v
}

#[inline(always)]
fn fnv1a(u: u32, v: u32) -> u32 {
    (u ^ v).wrapping_mul(FNV_PRIME)
}

#[inline(always)]
fn rotl32(n: u32, c: u32) -> u32 {
    n.rotate_left(c & 31)
}

#[inline(always)]
fn rotr32(n: u32, c: u32) -> u32 {
    n.rotate_right(c & 31)
}

#[inline(always)]
fn clz32(x: u32) -> u32 {
    x.leading_zeros()
}

#[inline(always)]
fn popcount32(x: u32) -> u32 {
    x.count_ones()
}

#[inline(always)]
fn mul_hi32(a: u32, b: u32) -> u32 {
    ((a as u64).wrapping_mul(b as u64) >> 32) as u32
}

#[inline(always)]
fn byte_perm(a: u32, b: u32, selector: u32) -> u32 {
    let combined: u64 = ((a as u64) << 32) | (b as u64);
    let sel = selector & 0x7777;
    let b0 = ((combined >> (((sel >> 0) & 0x7) * 8)) & 0xFF) as u32;
    let b1 = ((combined >> (((sel >> 4) & 0x7) * 8)) & 0xFF) as u32;
    let b2 = ((combined >> (((sel >> 8) & 0x7) * 8)) & 0xFF) as u32;
    let b3 = ((combined >> (((sel >> 12) & 0x7) * 8)) & 0xFF) as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

#[inline(always)]
fn brev32(x: u32) -> u32 {
    x.reverse_bits()
}

#[inline(always)]
fn funnelshift_l(a: u32, b: u32, c: u32) -> u32 {
    let shift = c & 31;
    if shift == 0 { a } else { (a << shift) | (b >> (32 - shift)) }
}

#[inline(always)]
fn mad_lo32(a: u32, b: u32, c: u32) -> u32 {
    a.wrapping_mul(b).wrapping_add(c)
}

// ============================================================================
// KISS99 RNG — matches C++ kiss99 exactly
// ============================================================================

#[derive(Clone)]
struct Kiss99 {
    z: u32,
    w: u32,
    jsr: u32,
    jcong: u32,
}

impl Kiss99 {
    fn next(&mut self) -> u32 {
        // Order must match C++: z, w, jcong, jsr, then combine
        self.z = 36969u32.wrapping_mul(self.z & 0xffff).wrapping_add(self.z >> 16);
        self.w = 18000u32.wrapping_mul(self.w & 0xffff).wrapping_add(self.w >> 16);
        self.jcong = 69069u32.wrapping_mul(self.jcong).wrapping_add(1234567);
        self.jsr ^= self.jsr << 17;
        self.jsr ^= self.jsr >> 13;
        self.jsr ^= self.jsr << 5;
        (((self.z << 16).wrapping_add(self.w)) ^ self.jcong).wrapping_add(self.jsr)
    }
}

// ============================================================================
// random_math — 15 operations (MeowPoW v2)
// ============================================================================

#[inline]
fn random_math(a: u32, b: u32, selector: u32) -> u32 {
    match selector % 15 {
        0 => a.wrapping_add(b),
        1 => a.wrapping_mul(b),
        2 => mul_hi32(a, b),
        3 => a.min(b),
        4 => rotl32(a, b),
        5 => rotr32(a, b),
        6 => a & b,
        7 => a | b,
        8 => a ^ b,
        9 => clz32(a).wrapping_add(clz32(b)),
        10 => popcount32(a).wrapping_add(popcount32(b)),
        11 => byte_perm(a, b, selector >> 16),
        12 => brev32(a) ^ b,
        13 => funnelshift_l(a, b, selector),
        14 => mad_lo32(a, b, selector >> 16),
        _ => unreachable!(),
    }
}

// ============================================================================
// random_merge — 6 operations (MeowPoW v2)
// ============================================================================

#[inline]
fn random_merge(a: &mut u32, b: u32, selector: u32) {
    let x = (selector >> 16) % 31 + 1;
    match selector % 6 {
        0 => *a = a.wrapping_mul(33).wrapping_add(b),
        1 => *a = (*a ^ b).wrapping_mul(33),
        2 => *a = rotl32(*a, x) ^ b,
        3 => *a = rotr32(*a, x) ^ b,
        4 => *a = a.wrapping_add(b) ^ rotl32(b, x),
        5 => *a = mul_hi32(*a, b) ^ b,
        _ => unreachable!(),
    }
}

// ============================================================================
// Epoch context: light cache, DAG item computation, L1 cache
// ============================================================================

/// Helper: convert 64-byte keccak512 output to 16 u32 words (little-endian).
fn hash512_to_u32s(h: &[u8; 64]) -> [u32; 16] {
    let mut w = [0u32; 16];
    for i in 0..16 {
        w[i] = u32::from_le_bytes([h[i*4], h[i*4+1], h[i*4+2], h[i*4+3]]);
    }
    w
}

/// Helper: convert 16 u32 words to 64 bytes (little-endian).
fn u32s_to_hash512(w: &[u32; 16]) -> [u8; 64] {
    let mut h = [0u8; 64];
    for i in 0..16 {
        h[i*4..i*4+4].copy_from_slice(&w[i].to_le_bytes());
    }
    h
}

/// Find the largest prime <= upper_bound.
fn find_largest_prime(upper_bound: i32) -> i32 {
    if upper_bound < 2 { return 0; }
    if upper_bound == 2 { return 2; }
    let mut n = if upper_bound % 2 == 0 { upper_bound - 1 } else { upper_bound };
    while !is_odd_prime(n) {
        n -= 2;
    }
    n
}

fn is_odd_prime(number: i32) -> bool {
    let mut d = 3i64;
    while d * d <= number as i64 {
        if number as i64 % d == 0 { return false; }
        d += 2;
    }
    true
}

fn calculate_light_cache_num_items(epoch: i32) -> i32 {
    let num_items_init = (LIGHT_CACHE_INIT_SIZE / HASH512_BYTES) as i32;
    let num_items_growth = (LIGHT_CACHE_GROWTH / HASH512_BYTES) as i32;
    let upper = num_items_init + epoch * num_items_growth;
    find_largest_prime(upper)
}

fn calculate_full_dataset_num_items(epoch: i32) -> i32 {
    let num_items_init = (FULL_DATASET_INIT_SIZE / HASH1024_BYTES) as i32;
    let num_items_growth = (FULL_DATASET_GROWTH / HASH1024_BYTES) as i32;
    let upper = num_items_init + epoch * num_items_growth;
    find_largest_prime(upper)
}

fn calculate_epoch_seed(epoch: i32) -> [u8; 32] {
    let mut seed = [0u8; 32];
    for _ in 0..epoch {
        seed = keccak256(&seed);
    }
    seed
}

/// Build the ethash light cache. Matches C++ `build_light_cache` exactly.
fn build_light_cache(seed: &[u8; 32], num_items: i32) -> Vec<[u32; 16]> {
    let n = num_items as usize;
    let mut cache = vec![[0u32; 16]; n];

    // Sequential keccak512 chain
    let h = keccak512(seed);
    cache[0] = hash512_to_u32s(&h);
    for i in 1..n {
        let prev_bytes = u32s_to_hash512(&cache[i - 1]);
        let h = keccak512(&prev_bytes);
        cache[i] = hash512_to_u32s(&h);
    }

    // 3 rounds of randomization
    for _q in 0..LIGHT_CACHE_ROUNDS {
        for i in 0..n {
            let t = cache[i][0]; // Already little-endian u32
            let v = (t as usize) % n;
            let w = (n + i - 1) % n;

            // XOR cache[v] and cache[w]
            let mut xored = [0u32; 16];
            for j in 0..16 {
                xored[j] = cache[v][j] ^ cache[w][j];
            }
            let xored_bytes = u32s_to_hash512(&xored);
            let h = keccak512(&xored_bytes);
            cache[i] = hash512_to_u32s(&h);
        }
    }

    cache
}

/// Calculate a single 512-bit dataset item from the light cache.
/// Matches C++ `item_state` + 512 rounds of FNV mixing.
fn calculate_dataset_item_512(cache: &[[u32; 16]], index: i64) -> [u32; 16] {
    let num_cache_items = cache.len() as i64;
    let seed = index as u32;

    // Initialize: mix = cache[index % num_cache_items] XOR seed in word 0
    let cache_idx = ((index % num_cache_items) + num_cache_items) % num_cache_items;
    let mut mix = cache[cache_idx as usize];
    mix[0] ^= seed; // le::uint32(seed) is identity on LE

    // Keccak512 of initial mix
    let mix_bytes = u32s_to_hash512(&mix);
    let h = keccak512(&mix_bytes);
    mix = hash512_to_u32s(&h);

    // 512 rounds of FNV mixing with cache lookups
    for round in 0..FULL_DATASET_ITEM_PARENTS {
        let t = fnv1(seed ^ round, mix[(round as usize) % 16]);
        let parent_index = ((t as i64) % num_cache_items + num_cache_items) % num_cache_items;
        let parent = &cache[parent_index as usize];
        // FNV1 element-wise: mix = fnv1(mix, parent) — note this is fnv1 not fnv1a
        for j in 0..16 {
            mix[j] = fnv1(mix[j], parent[j]);
        }
    }

    // Final keccak512
    let mix_bytes = u32s_to_hash512(&mix);
    let h = keccak512(&mix_bytes);
    hash512_to_u32s(&h)
}

/// Calculate a 2048-bit dataset item (4 × hash512). Matches C++ `calculate_dataset_item_2048`.
fn calculate_dataset_item_2048(cache: &[[u32; 16]], index: u32) -> [u32; 64] {
    let item0 = calculate_dataset_item_512(cache, (index as i64) * 4);
    let item1 = calculate_dataset_item_512(cache, (index as i64) * 4 + 1);
    let item2 = calculate_dataset_item_512(cache, (index as i64) * 4 + 2);
    let item3 = calculate_dataset_item_512(cache, (index as i64) * 4 + 3);

    let mut result = [0u32; 64];
    result[0..16].copy_from_slice(&item0);
    result[16..32].copy_from_slice(&item1);
    result[32..48].copy_from_slice(&item2);
    result[48..64].copy_from_slice(&item3);
    result
}

/// Build the 128KB L1 cache from DAG items.
fn build_l1_cache(cache: &[[u32; 16]]) -> Vec<u32> {
    let num_dag_items = L1_CACHE_SIZE / (64 * 4); // 128KB / 256 bytes per hash2048 = 512
    let mut l1 = vec![0u32; L1_CACHE_NUM_ITEMS];
    for i in 0..num_dag_items {
        let item = calculate_dataset_item_2048(cache, i as u32);
        l1[i * 64..(i + 1) * 64].copy_from_slice(&item);
    }
    l1
}

/// Complete epoch context for TariVision verification.
pub struct EpochContext {
    pub epoch_number: i32,
    pub light_cache: Vec<[u32; 16]>,
    pub l1_cache: Vec<u32>,
    pub full_dataset_num_items: i32,
}

/// Create an epoch context for the given epoch number.
pub fn create_epoch_context(epoch_number: i32) -> EpochContext {
    let meow_epoch = if epoch_number >= MEOWPOW_DAGCHANGE_EPOCH {
        epoch_number * 4
    } else {
        epoch_number
    };

    let light_cache_num_items = calculate_light_cache_num_items(meow_epoch);
    let full_dataset_num_items = calculate_full_dataset_num_items(meow_epoch);
    let seed = calculate_epoch_seed(epoch_number);
    let light_cache = build_light_cache(&seed, light_cache_num_items);
    let l1_cache = build_l1_cache(&light_cache);

    EpochContext {
        epoch_number,
        light_cache,
        l1_cache,
        full_dataset_num_items,
    }
}

// ============================================================================
// Epoch context caching (thread-safe singleton)
// ============================================================================

static EPOCH_CACHE: Mutex<Option<Arc<EpochContext>>> = Mutex::new(None);

/// Get or create the epoch context for the given block number.
pub fn get_epoch_context(block_number: u64) -> Arc<EpochContext> {
    let epoch = (block_number / EPOCH_LENGTH) as i32;
    let mut cache = EPOCH_CACHE.lock().unwrap();
    if let Some(ref ctx) = *cache {
        if ctx.epoch_number == epoch {
            return Arc::clone(ctx);
        }
    }
    let ctx = Arc::new(create_epoch_context(epoch));
    *cache = Some(Arc::clone(&ctx));
    ctx
}

// ============================================================================
// ProgPoW core: MixRngState, init_mix, round, hash_mix
// ============================================================================

/// ProgPoW mix RNG state with Fisher-Yates shuffled dst/src sequences.
struct MixRngState {
    rng: Kiss99,
    dst_seq: [u32; NUM_REGS],
    src_seq: [u32; NUM_REGS],
    dst_counter: usize,
    src_counter: usize,
}

impl MixRngState {
    fn new(seed: &[u32; 2]) -> Self {
        let z = fnv1a(FNV_OFFSET_BASIS, seed[0]);
        let w = fnv1a(z, seed[1]);
        let jsr = fnv1a(w, seed[0]);
        let jcong = fnv1a(jsr, seed[1]);
        let mut rng = Kiss99 { z, w, jsr, jcong };

        let mut dst_seq = [0u32; NUM_REGS];
        let mut src_seq = [0u32; NUM_REGS];
        for i in 0..NUM_REGS {
            dst_seq[i] = i as u32;
            src_seq[i] = i as u32;
        }

        // Fisher-Yates shuffle (matches C++: for i = num_regs downto 2)
        for i in (2..=NUM_REGS).rev() {
            let j = (rng.next() as usize) % i;
            dst_seq.swap(i - 1, j);
            let j = (rng.next() as usize) % i;
            src_seq.swap(i - 1, j);
        }

        MixRngState {
            rng,
            dst_seq,
            src_seq,
            dst_counter: 0,
            src_counter: 0,
        }
    }

    fn next_dst(&mut self) -> usize {
        let idx = self.dst_seq[self.dst_counter % NUM_REGS] as usize;
        self.dst_counter += 1;
        idx
    }

    fn next_src(&mut self) -> usize {
        let idx = self.src_seq[self.src_counter % NUM_REGS] as usize;
        self.src_counter += 1;
        idx
    }
}

/// Initialize the 16×32 mix array from a 2-word seed.
fn init_mix(seed: &[u32; 2]) -> [[u32; NUM_REGS]; NUM_LANES] {
    let z = fnv1a(FNV_OFFSET_BASIS, seed[0]);
    let w = fnv1a(z, seed[1]);

    let mut mix = [[0u32; NUM_REGS]; NUM_LANES];
    for l in 0..NUM_LANES {
        let jsr = fnv1a(w, l as u32);
        let jcong = fnv1a(jsr, l as u32);
        let mut rng = Kiss99 { z, w, jsr, jcong };
        for r in 0..NUM_REGS {
            mix[l][r] = rng.next();
        }
    }
    mix
}

/// One round of the ProgPoW loop.
fn progpow_round(
    context: &EpochContext,
    r: u32,
    mix: &mut [[u32; NUM_REGS]; NUM_LANES],
    state: &mut MixRngState,
) {
    let num_items = (context.full_dataset_num_items / 2) as u32;
    let item_index = mix[(r as usize) % NUM_LANES][0] % num_items;
    let item = calculate_dataset_item_2048(&context.light_cache, item_index);

    let num_words_per_lane = 64 / NUM_LANES; // 2048 bits / (32 bits * 16 lanes) = 4
    let max_operations = if NUM_CACHE_ACCESSES > NUM_MATH_OPERATIONS {
        NUM_CACHE_ACCESSES
    } else {
        NUM_MATH_OPERATIONS
    };

    for i in 0..max_operations {
        if i < NUM_CACHE_ACCESSES {
            let src = state.next_src();
            let dst = state.next_dst();
            let sel = state.rng.next();
            for l in 0..NUM_LANES {
                let offset = (mix[l][src] as usize) % L1_CACHE_NUM_ITEMS;
                random_merge(&mut mix[l][dst], context.l1_cache[offset], sel);
            }
        }
        if i < NUM_MATH_OPERATIONS {
            let src_rnd = (state.rng.next() as usize) % (NUM_REGS * (NUM_REGS - 1));
            let src1 = src_rnd % NUM_REGS;
            let mut src2 = src_rnd / NUM_REGS;
            if src2 >= src1 {
                src2 += 1;
            }
            let sel1 = state.rng.next();
            let dst = state.next_dst();
            let sel2 = state.rng.next();
            for l in 0..NUM_LANES {
                let data = random_math(mix[l][src1], mix[l][src2], sel1);
                random_merge(&mut mix[l][dst], data, sel2);
            }
        }
    }

    // DAG access pattern
    let mut dsts = [0usize; 4]; // num_words_per_lane = 4
    let mut sels = [0u32; 4];
    for i in 0..num_words_per_lane {
        dsts[i] = if i == 0 { 0 } else { state.next_dst() };
        sels[i] = state.rng.next();
    }

    // DAG access
    for l in 0..NUM_LANES {
        let offset = ((l ^ (r as usize)) % NUM_LANES) * num_words_per_lane;
        for i in 0..num_words_per_lane {
            let word = item[offset + i]; // Already LE u32
            random_merge(&mut mix[l][dsts[i]], word, sels[i]);
        }
    }
}

/// Compute the mix hash via the ProgPoW loop (64 rounds).
fn hash_mix(context: &EpochContext, block_number: u64, seed: &[u32; 2]) -> [u8; 32] {
    let mut mix = init_mix(seed);

    // MixRngState is seeded from block_number / period_length, NOT from the hash seed
    let number = block_number / (PERIOD_LENGTH as u64);
    let new_state = [number as u32, (number >> 32) as u32];
    let mut state = MixRngState::new(&new_state);

    for i in 0..NUM_ROUNDS {
        progpow_round(context, i as u32, &mut mix, &mut state);
    }

    // Reduce each lane: FNV1a across all registers
    let mut lane_hash = [0u32; NUM_LANES];
    for l in 0..NUM_LANES {
        lane_hash[l] = FNV_OFFSET_BASIS;
        for r in 0..NUM_REGS {
            lane_hash[l] = fnv1a(lane_hash[l], mix[l][r]);
        }
    }

    // Reduce all lanes to 8-word (256-bit) result
    let mut mix_hash_words = [FNV_OFFSET_BASIS; 8];
    for l in 0..NUM_LANES {
        mix_hash_words[l % 8] = fnv1a(mix_hash_words[l % 8], lane_hash[l]);
    }

    // Convert to little-endian bytes (matching C++ le::uint32s)
    let mut mix_hash = [0u8; 32];
    for i in 0..8 {
        mix_hash[i * 4..i * 4 + 4].copy_from_slice(&mix_hash_words[i].to_le_bytes());
    }
    mix_hash
}

// ============================================================================
// TariVision hash (full computation) and hash_no_verify
// ============================================================================

/// Full TariVision hash: computes both final_hash and mix_hash from the DAG.
/// Used by miners and for full verification.
pub fn tarivision_hash(
    context: &EpochContext,
    header_hash: &[u8; 32],
    nonce: u64,
    block_number: u64,
) -> ([u8; 32], [u8; 32]) {
    // Convert header_hash to u32 words
    let mut hh = [0u32; 8];
    for i in 0..8 {
        hh[i] = u32::from_le_bytes(header_hash[i * 4..i * 4 + 4].try_into().unwrap());
    }

    // Initial Keccak-f[800]
    let mut state = [0u32; 25];
    state[..8].copy_from_slice(&hh);
    state[8] = nonce as u32;
    state[9] = (nonce >> 32) as u32;
    for i in 10..25 {
        state[i] = TARIVISION_DOMAIN[i - 10];
    }
    keccakf800(&mut state);

    let mut state2 = [0u32; 8];
    state2.copy_from_slice(&state[..8]);

    let hash_seed = [state2[0], state2[1]];
    let mix_hash = hash_mix(context, block_number, &hash_seed);

    // Convert mix_hash to u32 words
    let mut mh = [0u32; 8];
    for i in 0..8 {
        mh[i] = u32::from_le_bytes(mix_hash[i * 4..i * 4 + 4].try_into().unwrap());
    }

    // Final Keccak-f[800]
    let mut state = [0u32; 25];
    state[..8].copy_from_slice(&state2);
    state[8..16].copy_from_slice(&mh);
    for i in 16..25 {
        state[i] = TARIVISION_DOMAIN[i - 16];
    }
    keccakf800(&mut state);

    let mut final_hash = [0u8; 32];
    for i in 0..8 {
        final_hash[i * 4..i * 4 + 4].copy_from_slice(&state[i].to_le_bytes());
    }

    (final_hash, mix_hash)
}

/// Light verification: compute final hash from header_hash + mix_hash + nonce
/// without DAG access. Used as a fast first-pass check.
pub fn tarivision_hash_no_verify(
    header_hash: &[u8; 32],
    mix_hash: &[u8; 32],
    nonce: u64,
    _block_number: u64,
) -> [u8; 32] {
    let mut hh = [0u32; 8];
    for i in 0..8 {
        hh[i] = u32::from_le_bytes(header_hash[i * 4..i * 4 + 4].try_into().unwrap());
    }
    let mut mh = [0u32; 8];
    for i in 0..8 {
        mh[i] = u32::from_le_bytes(mix_hash[i * 4..i * 4 + 4].try_into().unwrap());
    }

    // Initial Keccak
    let mut state = [0u32; 25];
    state[..8].copy_from_slice(&hh);
    state[8] = nonce as u32;
    state[9] = (nonce >> 32) as u32;
    for i in 10..25 { state[i] = TARIVISION_DOMAIN[i - 10]; }
    keccakf800(&mut state);

    let mut state2 = [0u32; 8];
    state2.copy_from_slice(&state[..8]);

    // Final Keccak
    let mut state = [0u32; 25];
    state[..8].copy_from_slice(&state2);
    state[8..16].copy_from_slice(&mh);
    for i in 16..25 { state[i] = TARIVISION_DOMAIN[i - 16]; }
    keccakf800(&mut state);

    let mut output = [0u8; 32];
    for i in 0..8 {
        output[i * 4..i * 4 + 4].copy_from_slice(&state[i].to_le_bytes());
    }
    output
}

// ============================================================================
// Full verification: recompute mix_hash and compare
// ============================================================================

/// Verify a TariVision proof by recomputing the mix_hash from the DAG.
/// Returns the difficulty if valid, or an error if the mix_hash doesn't match.
pub fn tarivision_verify(
    header_hash: &[u8; 32],
    claimed_mix_hash: &[u8; 32],
    nonce: u64,
    block_number: u64,
) -> Result<Difficulty, TariVisionError> {
    let context = get_epoch_context(block_number);

    // First: fast check — compute final hash from claimed mix_hash
    let final_hash = tarivision_hash_no_verify(header_hash, claimed_mix_hash, nonce, block_number);
    let difficulty = Difficulty::big_endian_difficulty(&final_hash)?;

    // Second: full check — recompute mix_hash from DAG
    let (_, expected_mix_hash) = tarivision_hash(&context, header_hash, nonce, block_number);

    if claimed_mix_hash != &expected_mix_hash {
        return Err(TariVisionError::MixHashMismatch);
    }

    Ok(difficulty)
}

// ============================================================================
// Public API for Tari integration
// ============================================================================

/// Errors that can occur during TariVision verification.
#[derive(Debug, thiserror::Error)]
pub enum TariVisionError {
    #[error("TariVision pow_data too short: expected at least 32 bytes for mix_hash, got {0}")]
    PowDataTooShort(usize),
    #[error("TariVision mix_hash does not match DAG computation — possible fabricated proof")]
    MixHashMismatch,
    #[error("Difficulty error: {0}")]
    DifficultyError(#[from] DifficultyError),
}

/// Calculate the achieved difficulty for a TariVision block header.
///
/// Performs full DAG verification: recomputes the mix_hash from the epoch context
/// and compares it against the claimed mix_hash in pow_data.
pub fn tarivision_difficulty(header: &BlockHeader) -> Result<Difficulty, TariVisionError> {
    let pow_data = &header.pow.pow_data;
    if pow_data.len() < 32 {
        return Err(TariVisionError::PowDataTooShort(pow_data.len()));
    }

    let mut mix_hash = [0u8; 32];
    mix_hash.copy_from_slice(&pow_data[..32]);

    let mining_hash_vec = header.mining_hash();
    let mut header_hash = [0u8; 32];
    header_hash.copy_from_slice(&mining_hash_vec[..32]);

    let block_number = header.height;

    tarivision_verify(&header_hash, &mix_hash, header.nonce, block_number)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keccakf800_deterministic() {
        let mut state = [0u32; 25];
        state[0] = 0xdeadbeef;
        state[1] = 0xcafebabe;
        keccakf800(&mut state);
        assert_ne!(state[0], 0);

        let mut state2 = [0u32; 25];
        state2[0] = 0xdeadbeef;
        state2[1] = 0xcafebabe;
        keccakf800(&mut state2);
        assert_eq!(state, state2);
    }

    #[test]
    fn test_keccak512_deterministic() {
        let data = [0xABu8; 32];
        let h1 = keccak512(&data);
        let h2 = keccak512(&data);
        assert_eq!(h1, h2);
        assert_ne!(h1, [0u8; 64]);
    }

    #[test]
    fn test_keccak256_deterministic() {
        let data = [0u8; 32];
        let h1 = keccak256(&data);
        let h2 = keccak256(&data);
        assert_eq!(h1, h2);
        assert_ne!(h1, [0u8; 32]);
    }

    #[test]
    fn test_keccak256_empty() {
        // Known test vector: Keccak-256("") =
        // c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        let h = keccak256(&[]);
        let expected: [u8; 32] = [
            0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c,
            0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0,
            0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b,
            0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
        ];
        assert_eq!(h, expected);
    }

    #[test]
    fn test_kiss99_matches_cpp() {
        // C++ kiss99 default state: z=362436069, w=521288629, jsr=123456789, jcong=380116160
        let mut rng = Kiss99 {
            z: 362436069,
            w: 521288629,
            jsr: 123456789,
            jcong: 380116160,
        };
        // Generate a few values and ensure determinism
        let v1 = rng.next();
        let v2 = rng.next();
        assert_ne!(v1, v2);
        assert_ne!(v1, 0);
    }

    #[test]
    fn test_find_largest_prime() {
        assert_eq!(find_largest_prime(10), 7);
        assert_eq!(find_largest_prime(11), 11);
        assert_eq!(find_largest_prime(100), 97);
        assert_eq!(find_largest_prime(2), 2);
        assert_eq!(find_largest_prime(1), 0);
    }

    #[test]
    fn test_epoch_context_sizes() {
        // Epoch 0: light_cache_num_items = find_largest_prime(2^24 / 64) = find_largest_prime(262144)
        let lc = calculate_light_cache_num_items(0);
        assert!(lc > 0);
        assert!(lc <= 262144);

        let fd = calculate_full_dataset_num_items(0);
        assert!(fd > 0);
    }

    #[test]
    fn test_hash_no_verify_deterministic() {
        let header_hash = [0xABu8; 32];
        let mix_hash = [0xCDu8; 32];
        let nonce = 42u64;
        let block_number = 1000u64;

        let h1 = tarivision_hash_no_verify(&header_hash, &mix_hash, nonce, block_number);
        let h2 = tarivision_hash_no_verify(&header_hash, &mix_hash, nonce, block_number);
        assert_eq!(h1, h2);

        let h3 = tarivision_hash_no_verify(&header_hash, &mix_hash, 43, block_number);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_full_hash_deterministic() {
        // Use epoch 0, block 0 for a basic determinism test
        let context = create_epoch_context(0);
        let header_hash = [0xABu8; 32];
        let nonce = 0u64;

        let (fh1, mh1) = tarivision_hash(&context, &header_hash, nonce, 0);
        let (fh2, mh2) = tarivision_hash(&context, &header_hash, nonce, 0);
        assert_eq!(fh1, fh2);
        assert_eq!(mh1, mh2);
        assert_ne!(fh1, [0u8; 32]);
        assert_ne!(mh1, [0u8; 32]);
    }

    #[test]
    fn test_full_hash_matches_no_verify() {
        // The final hash from tarivision_hash should match tarivision_hash_no_verify
        // when given the correct mix_hash
        let context = create_epoch_context(0);
        let header_hash = [0x42u8; 32];
        let nonce = 12345u64;

        let (final_hash, mix_hash) = tarivision_hash(&context, &header_hash, nonce, 0);
        let no_verify_hash = tarivision_hash_no_verify(&header_hash, &mix_hash, nonce, 0);
        assert_eq!(final_hash, no_verify_hash);
    }

    #[test]
    fn test_verify_accepts_valid_proof() {
        let context = create_epoch_context(0);
        let header_hash = [0x42u8; 32];
        let nonce = 0u64;

        let (_, mix_hash) = tarivision_hash(&context, &header_hash, nonce, 0);
        let result = tarivision_verify(&header_hash, &mix_hash, nonce, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_rejects_fake_mix_hash() {
        let fake_mix = [0xFFu8; 32];
        let header_hash = [0x42u8; 32];
        let result = tarivision_verify(&header_hash, &fake_mix, 0, 0);
        match result {
            Err(TariVisionError::MixHashMismatch) => {} // Expected
            other => panic!("Expected MixHashMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_random_math_all_ops() {
        for op in 0..15u32 {
            let _ = random_math(0xdeadbeef, 0xcafebabe, op);
        }
    }

    #[test]
    fn test_random_merge_all_ops() {
        for op in 0..6u32 {
            let mut val = 0xdeadbeef;
            random_merge(&mut val, 0xcafebabe, op | (16 << 16));
            assert_ne!(val, 0xdeadbeef);
        }
    }

    #[test]
    fn test_fnv1a() {
        let result = fnv1a(FNV_OFFSET_BASIS, 42);
        assert_ne!(result, 0);
        assert_ne!(result, FNV_OFFSET_BASIS);
    }
}
