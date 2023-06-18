// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use tari_crypto::hash::blake2::Blake256;
use tari_mmr::sparse_merkle_tree::{NodeKey, SparseMerkleTree, ValueHash};

fn random_key() -> NodeKey {
    let key = rand::random::<[u8; 32]>();
    NodeKey::from(key)
}

fn get_keys(n: usize) -> Vec<NodeKey> {
    (0..n).map(|_| random_key()).collect()
}

fn create_smt() -> SparseMerkleTree<Blake256> {
    SparseMerkleTree::<Blake256>::new()
}

pub fn benchmark_sparse_merkle_trees(c: &mut Criterion) {
    let sizes = [100, 10_000];
    for size in sizes {
        c.bench_function(&format!("SMT: Insert {size} keys"), move |b| {
            let keys = get_keys(size);
            let mut smt = create_smt();
            b.iter_batched(
                || keys.clone(),
                |hashes| {
                    hashes.into_iter().for_each(|key| {
                        smt.upsert(key, ValueHash::default()).unwrap();
                    });
                },
                BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(smt, benchmark_sparse_merkle_trees);
criterion_main!(smt);
