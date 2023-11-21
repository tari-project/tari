// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use blake2::Blake2b;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use digest::consts::U32;
use tari_mmr::sparse_merkle_tree::{NodeKey, SparseMerkleTree, ValueHash};

type TestSmt = SparseMerkleTree<Blake2b<U32>>;

// The number of keys to use for full trees
const SIZES: [usize; 4] = [100, 1_000, 10_000, 100_000];

// Helper to generate a single random key
fn random_key() -> NodeKey {
    let key = rand::random::<[u8; 32]>();
    NodeKey::from(key)
}

// Helper to generate a set of random keys
fn get_keys(n: usize) -> Vec<NodeKey> {
    (0..n).map(|_| random_key()).collect()
}

// Helper to upsert keys
fn upsert_keys(smt: &mut TestSmt, keys: Vec<NodeKey>) {
    keys.into_iter().for_each(|key| {
        smt.upsert(key, ValueHash::default()).unwrap();
    });
}

// Helper to delete keys
fn delete_keys(smt: &mut TestSmt, keys: &[NodeKey]) {
    keys.iter().for_each(|key| {
        smt.delete(key).unwrap();
    });
}

// Build an SMT by inserting keys
pub fn build_smt(c: &mut Criterion) {
    for size in SIZES {
        c.bench_function(&format!("SMT: Insert {size} keys"), move |b| {
            let keys = get_keys(size);
            b.iter_batched(
                || {
                    // Set up a fresh tree for this iteration
                    (TestSmt::new(), keys.clone())
                },
                |(mut smt, hashes)| {
                    upsert_keys(&mut smt, hashes);
                },
                BatchSize::SmallInput,
            );
        });
    }
}

// Compute the root hash of a full tree
pub fn full_root_hash(c: &mut Criterion) {
    for size in SIZES {
        c.bench_function(&format!("SMT: Full root hash on {size}-key tree"), move |b| {
            // We can reuse the same tree between iterations
            let keys = get_keys(size);
            let mut smt = TestSmt::new();
            upsert_keys(&mut smt, keys);

            b.iter(|| {
                smt.root();
            });
        });
    }
}

// Delete half of the keys from a full tree
pub fn delete_half_keys(c: &mut Criterion) {
    for size in SIZES {
        c.bench_function(&format!("SMT: Delete half of keys on {size}-key tree"), move |b| {
            let keys = get_keys(size);
            b.iter_batched(
                || {
                    // Build a a fresh tree for this iteration
                    let mut smt = TestSmt::new();
                    upsert_keys(&mut smt, keys.clone());

                    (smt, keys.clone())
                },
                |(mut smt, keys)| {
                    delete_keys(&mut smt, &keys[..size / 2]);
                },
                BatchSize::SmallInput,
            );
        });
    }
}

// Compute the root hash of a half-empty tree
pub fn half_root_hash(c: &mut Criterion) {
    for size in SIZES {
        c.bench_function(&format!("SMT: Half-empty root hash on {size}-key tree"), move |b| {
            // We can reuse the same tree between iterations
            let keys = get_keys(size);
            let mut smt = TestSmt::new();
            upsert_keys(&mut smt, keys.clone());
            delete_keys(&mut smt, &keys[..size / 2]);

            b.iter(|| {
                smt.root();
            });
        });
    }
}

// Delete remaining half of the keys from a half-empty tree
pub fn delete_remaining_keys(c: &mut Criterion) {
    for size in SIZES {
        c.bench_function(&format!("SMT: Delete half of keys on {size}-key tree"), move |b| {
            let keys = get_keys(size);
            b.iter_batched(
                || {
                    // Build a a fresh tree for this iteration
                    let mut smt = TestSmt::new();
                    upsert_keys(&mut smt, keys.clone());
                    delete_keys(&mut smt, &keys[..size / 2]);

                    (smt, keys.clone())
                },
                |(mut smt, keys)| {
                    delete_keys(&mut smt, &keys[size / 2..]);
                },
                BatchSize::SmallInput,
            );
        });
    }
}

// Compute the root hash of an empty tree
pub fn empty_root_hash(c: &mut Criterion) {
    for size in SIZES {
        c.bench_function(&format!("SMT: Half-empty root hash on {size}-key tree"), move |b| {
            // We can reuse the same tree between iterations
            let keys = get_keys(size);
            let mut smt = TestSmt::new();
            upsert_keys(&mut smt, keys.clone());
            delete_keys(&mut smt, &keys[..size / 2]);
            delete_keys(&mut smt, &keys[size / 2..]);

            b.iter(|| {
                smt.root();
            });
        });
    }
}

criterion_group!(
    smt,
    build_smt,
    full_root_hash,
    delete_half_keys,
    half_root_hash,
    delete_remaining_keys,
    empty_root_hash
);
criterion_main!(smt);
