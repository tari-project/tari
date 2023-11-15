// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use blake2::Blake2b;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use digest::consts::U32;
use tari_mmr::sparse_merkle_tree::{NodeKey, SparseMerkleTree, ValueHash};

fn random_key() -> NodeKey {
    let key = rand::random::<[u8; 32]>();
    NodeKey::from(key)
}

fn get_keys(n: usize) -> Vec<NodeKey> {
    (0..n).map(|_| random_key()).collect()
}

fn create_smt() -> SparseMerkleTree<Blake2b<U32>> {
    SparseMerkleTree::<Blake2b<U32>>::new()
}

pub fn benchmark_smt_insert(c: &mut Criterion) {
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

fn insert_into_smt(keys: &[NodeKey], tree: &mut SparseMerkleTree<Blake2b<U32>>) {
    keys.iter().for_each(|key| {
        tree.upsert(key.clone(), ValueHash::default()).unwrap();
    });
}

fn delete_from_smt(keys: &[NodeKey], tree: &mut SparseMerkleTree<Blake2b<U32>>) {
    keys.iter().for_each(|key| {
        tree.delete(key).unwrap();
    });
}

fn time_function(header: &str, f: impl FnOnce()) -> std::time::Duration {
    println!("Starting: {header}");
    let now = std::time::Instant::now();
    f();
    let t = now.elapsed();
    println!("Finished: {header} - {t:?}");
    t
}

pub fn root_hash(_c: &mut Criterion) {
    let size = 1_000_000;
    let half_size = size / 2;
    let keys = get_keys(size);
    let mut tree = create_smt();
    time_function(&format!("SMT: Inserting {size} keys"), || {
        insert_into_smt(&keys, &mut tree);
    });
    time_function("SMT: Calculating root hash", || {
        let size = tree.size();
        let hash = tree.hash();
        println!("Tree size: {size}. Root hash: {hash:x}");
    });
    time_function(&format!("SMT: Deleting {half_size} keys"), || {
        delete_from_smt(&keys[0..half_size], &mut tree);
    });
    time_function("SMT: Calculating root hash", || {
        let size = tree.size();
        let hash = tree.hash();
        println!("Tree size: {size}. Root hash: {hash:x}");
    });
    time_function(&format!("SMT: Deleting another {half_size} keys"), || {
        delete_from_smt(&keys[half_size..], &mut tree);
    });
    time_function("SMT: Calculating root hash", || {
        let size = tree.size();
        let hash = tree.hash();
        println!("Tree size: {size}. Root hash: {hash:x}");
    });
}

criterion_group!(smt, benchmark_smt_insert, root_hash);
criterion_main!(smt);
